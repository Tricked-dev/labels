use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, LazyLock};
use std::time::{Duration, Instant};
use std::{env, thread};

use ai::text_to_data;
use circe::Client;
use color_eyre::eyre::anyhow;
use color_eyre::Result;
use drawing::{place_item, Data};
use minifb::{Key, Scale, Window, WindowOptions};
use niimbot::{get_usb_adapter, NiimbotPrinterClient};

mod ai;
mod circe;
mod config;
mod drawing;
mod niimbot;
mod ntfy;

#[cfg(test)]
mod tests;

use config::Config;
use rustrict::{Censor, Type};

static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

enum UICommand {
    Clear,
    Draw(Data),
    Quit,
}

enum PrinterCommand {
    Print(Vec<u32>),
}

fn main() -> Result<()> {
    color_eyre::install()?;
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "debug");
    }

    let running = Arc::new(AtomicBool::new(true));

    env_logger::init();
    dbg!(&*CONFIG);
    let width = CONFIG.width;
    let height = CONFIG.height;

    let mut window = Window::new(
        "H",
        width as usize,
        height as usize,
        WindowOptions {
            resize: false,
            scale: Scale::X1,
            borderless: true,
            ..WindowOptions::default()
        },
    )
    .unwrap_or_else(|e| {
        panic!("{}", e);
    });

    let (tx, rx) = mpsc::channel::<UICommand>();
    let (printer_tx, printer_rx) = mpsc::channel::<PrinterCommand>();

    let tx = Arc::new(tx);
    let tx_clone = tx.clone();

    let running_thread = Arc::clone(&running);

    let printer_thread = std::thread::spawn(move || {
        let tx = tx_clone;
        let mut last_hb = Instant::now();
        let mut printer_task = || {
            let mut printer = NiimbotPrinterClient::new(Box::new(get_usb_adapter()?))?;
            printer.heartbeat()?;

            let mut hb_failures = 0;

            while running_thread.load(Ordering::Relaxed) {
                let now = Instant::now();
                if now.duration_since(last_hb) > Duration::from_secs(15) {
                    last_hb = now;
                    if let Err(e) = printer.heartbeat() {
                        hb_failures += 1;
                        log::warn!(
                            "Failed to heartbeat printer, retrying: {e:?}, failures: {hb_failures}"
                        );
                        if hb_failures > 5 {
                            log::error!("Failed to heartbeat printer, exiting");
                            Err(anyhow!(
                                "Failed to heartbeat printer 5 times, exiting\n{e:?}",
                            ))?;
                        }
                    } else {
                        hb_failures = 0;
                    }
                }

                if let Ok(data) = printer_rx.try_recv() {
                    match data {
                        PrinterCommand::Print(data) => {
                            if let Err(printer_e) = printer.print_label(
                                &data,
                                CONFIG.width as usize,
                                CONFIG.height as usize,
                                1,
                                1,
                                5,
                            ) {
                                log::error!("Error printing: {:?}", printer_e);
                                log::debug!(
                                    "Waiting 500ms to send heartbeat to see if printer is dead"
                                );
                                thread::sleep(Duration::from_millis(500));
                                match printer.heartbeat() {
                                    Ok(_) => {
                                        log::debug!(
                                            "Heartbeat successful, everything might just be fine"
                                        );
                                    }
                                    Err(e) => {
                                        log::error!("Heartbeat failed: {:?}", e);
                                        log::debug!("Retrying heartbeat in 500ms");
                                        thread::sleep(Duration::from_millis(500));
                                        if let Err(e) = printer.heartbeat() {
                                            log::error!("Failed to heartbeat printer, exiting");
                                            Err(anyhow!(
                                                "Heartbeat failed after printing multiple times exiting\n{e:?}\n\nprinter error: {printer_e:?}",
                                            ))?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }

            Ok(())
        };
        let out: Result<()> = printer_task();
        if CONFIG.disable_printer {
            return;
        }
        running_thread.store(false, Ordering::Relaxed);
        tx.send(UICommand::Quit).ok();
        if let Err(e) = out {
            log::error!("Error in printer thread: {:?}", e);
            if !CONFIG.notify_url.is_empty() {
                ntfy::NotifyBuilder::new(format!("Error in printer thread: {:?}", e))
                    .send(&CONFIG.notify_url)
                    .expect("Failed to send notification");
            }
        }
    });

    let tx_clone = tx.clone();

    let running_thread = Arc::clone(&running);

    std::thread::spawn(move || {
        let tx = tx_clone;

        let result = || {
            let mut client = Client::new(circe::Config {
                channels: vec![CONFIG.irc_channel.clone()],
                host: CONFIG.irc_host.clone(),
                port: CONFIG.irc_port as u16,
                username: CONFIG.irc_username.clone(),
                ..Default::default()
            })?;

            client.write_command(circe::commands::Command::PASS(CONFIG.irc_token.clone()))?;
            client.identify()?;

            // client.privmsg(&CONFIG.irc_channel, ":Hello, world!")?;

            while running_thread.load(Ordering::Relaxed) {
                let line = match client.read() {
                    Ok(line) => line,
                    Err(..) => {
                        thread::sleep(std::time::Duration::from_millis(200));
                        continue;
                    }
                };

                match line {
                    circe::commands::Command::PRIVMSG(nick, channel, message) => {
                        let analysis = Censor::from_str(
                            "Hello World testing this should be very safe jij bent slim you are smart",
                        )
                            .with_censor_threshold(Type::INAPPROPRIATE)
                            .with_censor_first_character_threshold(Type::OFFENSIVE & Type::SEVERE)
                            .with_ignore_false_positives(false)
                            .with_ignore_self_censoring(false)
                            .with_censor_replacement('*')
                            .analyze();
                        if analysis.is(Type::INAPPROPRIATE) {
                            client.privmsg(
                                &CONFIG.irc_channel,
                                &format!(":Hey {}, i will not print that", nick),
                            )?;
                            log::warn!(
                                "PRIVMSG received from {}: {} {} is {analysis:?}, will not print",
                                nick,
                                channel,
                                message
                            );
                        } else {
                            log::info!("PRIVMSG received from {}: {} {}", nick, channel, message);
                            tx.send(UICommand::Draw(text_to_data(&message)?))?;
                        }
                    }
                    circe::commands::Command::QUIT(message) => {
                        println!("QUIT received from {}", message);
                    }
                    _ => {}
                }
            }

            Ok(())
        };
        let out: Result<()> = result();

        running_thread.store(false, Ordering::Relaxed);
        tx.send(UICommand::Quit).ok();
        if let Err(e) = out {
            log::error!("Error in irc thread: {:?}", e);
            if !CONFIG.notify_url.is_empty() {
                ntfy::NotifyBuilder::new(format!("Error in irc thread: {:?}", e))
                    .send(&CONFIG.notify_url)
                    .expect("Failed to send notification");
            }
        }
    });

    let running_thread = Arc::clone(&running);

    let counting_thread = std::thread::spawn(move || {
        let mut iter = 0;
        let count = CONFIG.clock_time as usize;
        while running_thread.load(Ordering::Relaxed) {
            if iter == count {
                tx.send(UICommand::Clear).ok();
                iter = 0
            } else {
                iter += 1;
            }
            let time_left = count - iter;
            let time_left_str = format!(
                "{}{:02}:{:02}",
                CONFIG.timer_prefix,
                time_left / 60,
                time_left % 60
            );

            std::fs::write(&CONFIG.timer_file, time_left_str).ok();

            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });

    window.set_target_fps(60);

    let mut label_data: Vec<u32> = vec![u32::MAX; CONFIG.width() * CONFIG.height()];

    while window.is_open() && !window.is_key_down(Key::Escape) && running.load(Ordering::Relaxed) {
        match rx.try_recv() {
            Ok(UICommand::Clear) => {
                if label_data.iter().any(|&v| v != u32::MAX)
                    && printer_tx
                        .send(PrinterCommand::Print(label_data.clone()))
                        .is_err()
                {
                    log::error!("Sending Failed Printer thread might be dead?");
                    break;
                }
                label_data.fill(u32::MAX);
            }
            Ok(UICommand::Draw(data)) => {
                if let Err(e) = place_item(&mut label_data, data) {
                    log::error!("Failed to place item: {:?}", e);
                    if !CONFIG.notify_url.is_empty() {
                        ntfy::NotifyBuilder::new(format!("Failed to place item: {:?}", e))
                            .set_priority("low".to_owned())
                            .send(&CONFIG.notify_url)?;
                    }
                }
            }
            Ok(UICommand::Quit) => {
                dbg!("Quit Received");
                break;
            }
            Err(..) => {}
        }

        window.update_with_buffer(&label_data, width as usize, height as usize)?;
    }

    log::debug!("Ending counting thread");
    counting_thread.join().unwrap();
    log::debug!("Ending printer thread");
    printer_thread.join().unwrap();
    log::debug!("Ending IRC thread, jk im killing it");
    // yeah uh irc thread doesn't want to join cause the read operation blocks for forever so we can just exit the process bye twitch :wave:
    Ok(())
}
