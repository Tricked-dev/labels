use std::{
    fs::File,
    io::{Cursor, Read},
    sync::{Arc, LazyLock},
};

use bytes::Bytes;
use color_eyre::{eyre::anyhow, Result};

#[derive(Clone, Debug)]
pub struct EfficientEntry {
    pub path: String,
    pub bytes: Bytes,
}

#[derive(Debug, PartialEq, Default)]
pub struct Data {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub size: u32,
}

pub mod fallback_parser;
mod layout_paragraph;
use ab_glyph::{point, Font, FontArc, PxScale};
use image_webp::WebPDecoder;
use layout_paragraph::layout_paragraph;
use tar_wasi::Archive;

use crate::CONFIG;

pub fn draw_text(pixmap: &mut [u32], text: &str, size: u32, posx: u32, posy: u32) -> Result<()> {
    let font_data: &[u8] = include_bytes!("../../Roboto-Regular.ttf");
    let font = FontArc::try_from_slice(font_data)?;

    let scale = PxScale::from((12 * size) as f32);

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

    let w = CONFIG.width as u32;

    let mut x_offset: i32 = 0;
    let mut y_offset: i32 = 0;
    for glyph in outlined {
        let bounds = glyph.px_bounds();
        let img_left = bounds.min.x as u32 - all_px_bounds.min.x as u32;
        let img_top = bounds.min.y as u32 - all_px_bounds.min.y as u32;
        glyph.draw(|x, y, v| {
            let x_loc = (img_left + x + posx) as i32 + x_offset;
            let y_loc = (img_top + y + posy) as i32 + y_offset;
            if x_loc + bounds.width() as i32 > CONFIG.width as i32 {
                log::debug!("Glyph is too wide, wrapping");
                x_offset -= CONFIG.width as i32 - posx as i32;
                y_offset += ((bounds.max.y - bounds.min.y) * 1.2) as i32;
            }
            let pos = ((x_loc) + (y_loc) * w as i32) as usize;

            if pixmap.get(pos).is_none() {
                return;
            };

            let write = v > 0.5;
            if !write {
                return;
            }
            if pixmap[pos] == u32::MAX {
                pixmap[pos] = u32::MIN;
            } else {
                pixmap[pos] = u32::MAX;
            }
        });
    }

    Ok(())
}

static IMAGES: LazyLock<Vec<Arc<EfficientEntry>>> = LazyLock::new(|| {
    if !std::fs::exists("images.tar").unwrap() {
        log::error!("Image Database not found not running with image support...");
        return Vec::new();
    }

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
            log::debug!("Found image name: {}", file.path);
            return Some(&file.bytes);
        }
    } else if let Some(file) = IMAGES
        .iter()
        .find(|c| c.path.split(":").last().unwrap() == name)
    {
        log::debug!("Found image name: {}", file.path);
        return Some(&file.bytes);
    }

    None
}

pub fn place_item(pixmap: &mut [u32], data: Data) -> Result<()> {
    match find_icon(&data.text) {
        Some(bytes) => draw_image(pixmap, bytes, data.size, data.x, data.y),
        None => draw_text(pixmap, &data.text, data.size, data.x, data.y),
    }
}

fn draw_image(pixmap: &mut [u32], image: &Bytes, size: u32, posx: u32, posy: u32) -> Result<()> {
    let cursor = std::io::Cursor::new(image);
    let mut decoder = WebPDecoder::new(cursor)?;
    let (width, height) = decoder.dimensions();
    let bytes_per_pixel = if decoder.has_alpha() { 4 } else { 3 };
    let mut data = vec![0; width as usize * height as usize * bytes_per_pixel];
    decoder.read_image(&mut data)?;

    // let scaled_width = width * size;
    // let scaled_height = height * size;

    let pixmap_width = CONFIG.width as u32;
    let pixmap_height = CONFIG.height as u32;

    for y in 0..height {
        for x in 0..width {
            let index = (y as usize * width as usize + x as usize) * bytes_per_pixel;
            let pixel = &data[index..index + bytes_per_pixel];
            let a = pixel[3];

            if a > 128 {
                continue;
            }

            for sy in 0..size {
                for sx in 0..size {
                    let scaled_x = x * size + sx;
                    let scaled_y = y * size + sy;

                    if scaled_x < pixmap_width && scaled_y < pixmap_height {
                        let pos = ((scaled_y + posy) * pixmap_width + scaled_x + posx) as usize;
                        if let Some(value) = pixmap.get_mut(pos) {
                            *value = u32::MIN;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
