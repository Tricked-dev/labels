use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{read_to_string, Cursor, Read};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use ab_glyph::PxScale;
use ab_glyph::{point, Glyph, Point, ScaleFont};
use ab_glyph::{Font, FontArc};
use bytes::Bytes;
use image_webp::WebPDecoder;
use minifb::{Key, Scale, Window, WindowOptions};
use rustyline::DefaultEditor;
use tar_wasi::Archive;
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};
use tinyjson::JsonValue;

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
    dbg!(&data);
    Some(data)
}

static BG: LazyLock<PremultipliedColorU8> =
    LazyLock::new(|| PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
static FG: LazyLock<PremultipliedColorU8> =
    LazyLock::new(|| PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap());

pub fn layout_paragraph<F, SF>(
    font: SF,
    position: Point,
    max_width: f32,
    text: &str,
    target: &mut Vec<Glyph>,
) where
    F: Font,
    SF: ScaleFont<F>,
{
    let v_advance = font.height() + font.line_gap();
    let mut caret = position + point(0.0, font.ascent());
    let mut last_glyph: Option<Glyph> = None;
    for c in text.chars() {
        if c.is_control() {
            if c == '\n' {
                caret = point(position.x, caret.y + v_advance);
                last_glyph = None;
            }
            continue;
        }
        let mut glyph = font.scaled_glyph(c);
        if let Some(previous) = last_glyph.take() {
            caret.x += font.kern(previous.id, glyph.id);
        }
        glyph.position = caret;

        last_glyph = Some(glyph.clone());
        caret.x += font.h_advance(glyph.id);

        if !c.is_whitespace() && caret.x > position.x + max_width {
            caret = point(position.x, caret.y + v_advance);
            glyph.position = caret;
            last_glyph = None;
        }

        target.push(glyph);
    }
}

fn draw_text(pixmap: &mut Pixmap, text: &str, size: u32, posx: u32, posy: u32) {
    let font_data: &[u8] = include_bytes!("../BerkeleyMonoTrial-Regular.otf");
    let font = FontArc::try_from_slice(font_data).unwrap();

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
        println!("No outlined glyphs?");
        return;
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

fn place_item(pixmap: &mut Pixmap, data: Data) {
    match find_icon(&data.text) {
        Some(bytes) => {
            draw_image(pixmap, bytes, data.size, data.x, data.y);
        }
        None => {
            draw_text(pixmap, &data.text, data.size, data.x, data.y);
        }
    }
}

fn draw_image(pixmap: &mut Pixmap, image: &Bytes, size: u32, posx: u32, posy: u32) {
    let cursor = std::io::Cursor::new(image);
    let mut decoder = WebPDecoder::new(cursor).unwrap();
    let (width, height) = decoder.dimensions();
    let bytes_per_pixel = if decoder.has_alpha() { 4 } else { 3 };
    let mut data = vec![0; width as usize * height as usize * bytes_per_pixel];
    decoder.read_image(&mut data).unwrap();

    let pixmap_width = pixmap.width();
    let pixmap_height = pixmap.height();

    for y in 0..height.min(pixmap_height) {
        for x in 0..width.min(pixmap_width) {
            let index = (y as usize * width as usize + x as usize) * bytes_per_pixel;
            let pixel = &data[index..index + bytes_per_pixel];
            let a = pixel[3];

            dbg!(a);

            let pos = ((y + posy) * pixmap_width + x + posx) as usize;

            if a > 128 {
                continue;
            }

            if let Some(value) = pixmap.pixels_mut().get_mut(pos) {
                *value = *FG;
            }
        }
    }
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

struct Config {
    model: String,
    prompt: String,
    openai_api_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            prompt: "You extract the x y location and size from a text, the x and y can appear anywhere in the text and the size can be nothing in which case you set it to 5, remove the indication words such as Place and At".to_owned(),
            openai_api_key: None,
        }
    }
}

impl Config {
    fn load() -> Self {
        let mut config = Config::default();
        if let Some(model) = env::var("MODEL").ok() {
            config.model = model;
        }
        if let Some(prompt) = env::var("PROMPT").ok() {
            config.prompt = prompt;
        }
        if let Some(openai_api_key) = env::var("OPENAI_API_KEY").ok() {
            config.openai_api_key = Some(openai_api_key);
        };

        let config_path = env::var("CONFIG_PATH").unwrap_or("config.json".to_string());
        if Path::new(&config_path).exists() {
            let file = std::fs::read_to_string(config_path).unwrap();
            let data: JsonValue = file.parse().unwrap();
            let parsed: &HashMap<_, _> = data.get().unwrap();
            if let Some(model) = parsed.get("model") {
                config.model = model.get::<String>().unwrap().to_string();
            }
            if let Some(prompt) = parsed.get("prompt") {
                config.prompt = prompt.get::<String>().unwrap().to_string();
            }
            if let Some(openai_api_key) = parsed.get("openai_api_key") {
                config.openai_api_key = Some(openai_api_key.get::<String>().unwrap().to_string());
            }
        };

        if config.openai_api_key.is_none() {
            panic!("No OpenAI API key found");
        };

        config
    }
}

fn main() {
    let config = Config::load();
    let body = [
        r##"
{
  "model": "gpt-4o-mini",
  "messages": [
    {
      "role": "system",
      "content": ""##,
        &config.prompt,
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
        .text(body.replace(
            "QUERY",
            &escape_json_string("Place Hello World at 20,20 with size 10"),
        ))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            &format!("Bearer {}", config.openai_api_key.unwrap()),
        )
        .send()
        .unwrap();

    let data = parse_json_to_data(req.text().unwrap().as_str()).unwrap();
    // println!("{}", req.text().unwrap());

    panic!("Done");

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

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // let Ok(line) = rl.readline("> ") else {
        //     continue;
        // };

        // println!("{}", line);
        //clear pixmap make everything black

        // draw_image(&mut pixmap, "gear", 60, 60);
        // draw_image(&mut pixmap, "binary", 0, 60);
        // draw_image(&mut pixmap, "binary", 0, 90);
        // // pixmap.

        // draw_text(&mut pixmap, "Hello, world!", 10, 10);
        // draw_text(&mut pixmap, "HIHI!", 10, 90);
        // draw_text(&mut pixmap, "TEXTST!", 10, 190);

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
}
