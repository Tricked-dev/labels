use std::collections::HashMap;
use std::fs::File;
use std::io::{read_to_string, Cursor, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, LazyLock, Mutex, RwLock};
use std::{env, thread};

use ab_glyph::PxScale;
use ab_glyph::{point, Glyph, Point, ScaleFont};
use ab_glyph::{Font, FontArc};
use anyhow::{anyhow, Result};
use bytes::Bytes;
use circe::Client;
use image_webp::WebPDecoder;
use minifb::{Key, Scale, Window, WindowOptions};
use rustyline::DefaultEditor;
use tar_wasi::Archive;
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};
use tinyjson::JsonValue;

mod circe;
mod config;
mod layout_paragraph;

use config::Config;
use layout_paragraph::layout_paragraph;

#[derive(Clone, Debug)]
pub struct EfficientEntry {
    pub path: String,
    pub bytes: Bytes,
}

use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub struct Data {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub size: u32,
}

pub fn parse_json_to_data(json: &str) -> Option<Data> {
    let data: JsonValue = json.parse().ok()?;
    let content: &String = data["choices"][0]["message"]["content"].get()?;
    let data_content: JsonValue = content.parse().ok()?;
    let x: &f64 = data_content["x"].get()?;
    let y: &f64 = data_content["y"].get()?;
    let size: &f64 = data_content["size"].get()?;
    let rest_text: &String = data_content["rest_text"].get()?;

    let data = Data {
        text: rest_text.clone(),
        x: *x as u32,
        y: *y as u32,
        size: *size as u32,
    };
    Some(data)
}

static BG: LazyLock<PremultipliedColorU8> =
    LazyLock::new(|| PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
static FG: LazyLock<PremultipliedColorU8> =
    LazyLock::new(|| PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap());

fn draw_text(pixmap: &mut Pixmap, text: &str, size: u32, posx: u32, posy: u32) -> Result<()> {
    let font_data: &[u8] = include_bytes!("../BerkeleyMonoTrial-Regular.otf");
    let font = FontArc::try_from_slice(font_data)?;

    let scale = PxScale::from((15 * size) as f32);

    let scaled_font = font.as_scaled(scale);

    let mut glyphs = Vec::new();
    layout_paragraph(scaled_font, point(20.0, 20.0), 9999.0, text, &mut glyphs);

    let outlined: Vec<_> = glyphs
        .into_iter()
        .filter_map(|g| font.outline_glyph(g))
        .collect();

    let Some(all_px_bounds) = outlined
        .iter()
        .map(|g| g.px_bounds())
        .reduce(|mut b, next| {
            b.min.x = b.min.x.min(next.min.x);
            b.max.x = b.max.x.max(next.max.x);
            b.min.y = b.min.y.min(next.min.y);
            b.max.y = b.max.y.max(next.max.y);
            b
        })
    else {
        return Err(anyhow!("No outlined glyphs?"));
    };

    for glyph in outlined {
        let bounds = glyph.px_bounds();
        let img_left = bounds.min.x as u32 - all_px_bounds.min.x as u32;
        let img_top = bounds.min.y as u32 - all_px_bounds.min.y as u32;
        glyph.draw(|x, y, v| {
            let w = pixmap.width();
            let pos = (img_left + x + posx) as usize + (img_top + y + posy) as usize * w as usize;
            let px = pixmap.pixels()[pos];

            let alpha = px.alpha().saturating_add((v * 255.0) as u8);
            let write = alpha > 128;
            if !write {
                // pixmap.pixels_mut()[pos] = *BG;
                return;
            }

            pixmap.pixels_mut()[pos] = *FG;
        });
    }

    Ok(())
}

static IMAGES: LazyLock<Vec<Arc<EfficientEntry>>> = LazyLock::new(|| {
    let mut file = File::open("images.tar").unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();

    let cursor = Cursor::new(buffer);
    let mut archive = Archive::new(cursor);

    archive
        .entries()
        .unwrap()
        .flatten()
        .map(|v| {
            let path = v.path().unwrap().to_string_lossy().to_string();
            let bytes = v.bytes().flatten().collect::<Vec<u8>>();
            Arc::new(EfficientEntry {
                path: path.replace(".webp", ""),
                bytes: Bytes::from(bytes),
            })
        })
        .collect()
});

fn find_icon(name: &str) -> Option<&'_ Bytes> {
    if name.contains(":") {
        if let Some(file) = IMAGES.iter().find(|c| c.path == name) {
            Some(&file.bytes)
        } else {
            None
        }
    } else if let Some(file) = IMAGES.iter().find(|c| c.path.contains(name)) {
        Some(&file.bytes)
    } else {
        None
    }
}

fn place_item(pixmap: &mut Pixmap, data: Data) -> Result<()> {
    match find_icon(&data.text) {
        Some(bytes) => draw_image(pixmap, bytes, data.size, data.x, data.y),
        None => draw_text(pixmap, &data.text, data.size, data.x, data.y),
    }
}

fn draw_image(pixmap: &mut Pixmap, image: &Bytes, size: u32, posx: u32, posy: u32) -> Result<()> {
    let cursor = std::io::Cursor::new(image);
    let mut decoder = WebPDecoder::new(cursor)?;
    let (width, height) = decoder.dimensions();
    let bytes_per_pixel = if decoder.has_alpha() { 4 } else { 3 };
    let mut data = vec![0; width as usize * height as usize * bytes_per_pixel];
    decoder.read_image(&mut data)?;

    let pixmap_width = pixmap.width();
    let pixmap_height = pixmap.height();

    for y in 0..height.min(pixmap_height) {
        for x in 0..width.min(pixmap_width) {
            let index = (y as usize * width as usize + x as usize) * bytes_per_pixel;
            let pixel = &data[index..index + bytes_per_pixel];
            let a = pixel[3];

            let pos = ((y + posy) * pixmap_width + x + posx) as usize;

            if a > 128 {
                continue;
            }

            if let Some(value) = pixmap.pixels_mut().get_mut(pos) {
                *value = *FG;
            }
        }
    }
    Ok(())
}

fn escape_json_string(input: &str) -> String {
    let mut escaped = String::new();
    for c in input.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            _ if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            _ => escaped.push(c),
        }
    }
    escaped
}
static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

fn text_to_date(text: &str) -> Result<Data> {
    let body = [
        r##"
{
  "model": "gpt-4o-mini",
  "messages": [
    {
      "role": "system",
      "content": ""##,
        &CONFIG.prompt,
        r##""
    },
    {
      "role": "user",
      "content": "QUERY"
    }
  ],
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "extract_schema",
      "schema": {
        "type": "object",
        "properties": {
          "rest_text": {
            "description": "The rest of the text",
            "type": "string"
          },
          "x": {
            "description": "The x location",
            "type": "integer"
          },
          "y": {
            "description": "The y location",
            "type": "integer"
          },
          "size": {
            "description": "The size",
            "type": "integer"
          }
        },
        "additionalProperties": false
      }
    }
  }
}

    "##,
    ]
    .join("");

    let req = attohttpc::post("https://api.openai.com/v1/chat/completions")
        .text(body.replace("QUERY", &escape_json_string(text)))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", CONFIG.openai_api_key.clone()),
        )
        .send()?;

    let data = parse_json_to_data(req.text()?.as_str()).ok_or(anyhow!("Failed to parse JSON"))?;

    Ok(data)
}

enum UICommand {
    Clear,
    Draw(Data),
    Quit,
}

static SHOULD_QUIT: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

fn main() -> Result<()> {
    dbg!(&*CONFIG);
    let config = Config::load();

    // loop {
    //     let line = client.read()?;
    //     println!("{:?}", line);
    // }

    // panic!("Done");

    let width = 500;
    let height = 500;

    // Create a pixmap
    let mut pixmap = Pixmap::new(width, height).unwrap();

    // Create a window
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

    let rl = DefaultEditor::new().unwrap();

    pixmap.fill(Color::from_rgba8(
        BG.red(),
        BG.green(),
        BG.blue(),
        BG.alpha(),
    ));

    // place_item(
    //     &mut pixmap,
    //     text_to_date("Place Hello World at 20,20 with size 2")?,
    // )?;

    // place_item(&mut pixmap, text_to_date("0,60 cat x4")?)?;

    let (tx, rx) = mpsc::channel::<UICommand>();

    let tx = Arc::new(tx);
    let tx_clone = tx.clone();
    let thread = std::thread::spawn(move || {
        let tx = tx_clone;

        let result = move || {
            let mut client = Client::new(circe::Config {
                channels: vec![CONFIG.irc_channel.clone()],
                host: CONFIG.irc_host.clone(),
                port: 6697,
                username: CONFIG.irc_username.clone(),
                ..Default::default()
            })?;
            client.write_command(circe::commands::Command::PASS(CONFIG.irc_token.clone()))?;
            client.identify()?;

            client.privmsg(&CONFIG.irc_channel, ":Hello, world!")?;

            loop {
                let line = match client.read() {
                    Ok(line) => line,
                    Err(..) => {
                        thread::sleep(std::time::Duration::from_millis(200));
                        if SHOULD_QUIT.load(Ordering::Relaxed) {
                            break;
                        }
                        continue;
                    }
                };

                match line {
                    circe::commands::Command::PRIVMSG(nick, channel, message) => {
                        println!("PRIVMSG received from {}: {} {}", nick, channel, message);
                        tx.send(UICommand::Draw(text_to_date(&message)?))?;
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
        println!("Thread done: {:?}", out);
    });

    let cleaner_thread = std::thread::spawn(move || {
        let mut iter = 0;
        let count = 60 * 5;
        loop {
            if SHOULD_QUIT.load(Ordering::Relaxed) {
                break;
            }
            if iter == count {
                tx.send(UICommand::Clear).ok();
                iter = 0
            } else {
                iter += 1;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });

    while window.is_open() && !window.is_key_down(Key::Escape) {
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

        window
            .update_with_buffer(&buffer, width as usize, height as usize)
            .unwrap();
    }

    SHOULD_QUIT.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    cleaner_thread.join().unwrap();

    Ok(())
}
