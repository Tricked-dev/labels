use std::fs::File;
use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, LazyLock};
use std::time::Duration;
use std::{env, thread};

use ab_glyph::point;
use ab_glyph::PxScale;
use ab_glyph::{Font, FontArc};
use ai::text_to_data;
use bytes::Bytes;
use circe::Client;
use color_eyre::{eyre::anyhow, Result};
use drawing::{place_item, Data, BG};
use image_webp::WebPDecoder;
use minifb::{Key, Scale, Window, WindowOptions};
use niimbot::adapters::UsbAdapter;
use niimbot::NiimbotPrinterClient;
use tar_wasi::Archive;
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};
use tinyjson::JsonValue;

mod ai;
mod circe;
mod config;
mod drawing;
mod niimbot;
mod ntfy;

#[cfg(test)]
mod tests;

use config::Config;

static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

enum UICommand {
    Clear,
    Draw(Data),
    Quit,
}

fn main() -> Result<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    let running = Arc::new(AtomicBool::new(true));

    env_logger::init();
    dbg!(&*CONFIG);
    let width = CONFIG.width;
    let height = CONFIG.height;

    let mut pixmap = Pixmap::new(width as u32, height as u32).unwrap();

    pixmap.fill(Color::from_rgba8(
        BG.red(),
        BG.green(),
        BG.blue(),
        BG.alpha(),
    ));

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

    let tx = Arc::new(tx);
    let tx_clone = tx.clone();

    let running_thread = Arc::clone(&running);
    let irc_thread = std::thread::spawn(move || {
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

            client.privmsg(&CONFIG.irc_channel, ":Hello, world!")?;

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
                        println!("PRIVMSG received from {}: {} {}", nick, channel, message);
                        tx.send(UICommand::Draw(text_to_data(&message)?))?;
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
        let count = 60 * 5;
        while running_thread.load(Ordering::Relaxed) {
            if iter == count {
                tx.send(UICommand::Clear).ok();
                iter = 0
            } else {
                iter += 1;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });

    while window.is_open() && !window.is_key_down(Key::Escape) && running.load(Ordering::Relaxed) {
        match rx.recv().unwrap() {
            UICommand::Clear => {
                pixmap.fill(Color::from_rgba8(
                    BG.red(),
                    BG.green(),
                    BG.blue(),
                    BG.alpha(),
                ));
            }
            UICommand::Draw(data) => {
                place_item(&mut pixmap, data)?;
            }
            UICommand::Quit => {
                break;
            }
        }
        let buffer: Vec<u32> = pixmap
            .data()
            .chunks(4)
            .map(|rgba| {
                let r = 255 - rgba[0] as u32;
                let g = 255 - rgba[1] as u32;
                let b = 255 - rgba[2] as u32;
                let a = rgba[3] as u32; // Keep the alpha value the same
                (a << 24) | (b << 16) | (g << 8) | r
            })
            .collect();

        window.update_with_buffer(&buffer, width as usize, height as usize)?;
    }

    irc_thread.join().unwrap();
    counting_thread.join().unwrap();

    Ok(())
}
