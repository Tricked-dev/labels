use std::time::Instant;

use ab_glyph::PxScale;
use ab_glyph::{point, Glyph, Point, ScaleFont};
use ab_glyph::{Font, FontArc};
use image_webp::WebPDecoder;
use minifb::{Key, Scale, Window, WindowOptions};
use once_cell::sync::Lazy;
use rustyline::DefaultEditor;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Transform};

static BG: Lazy<PremultipliedColorU8> =
    Lazy::new(|| PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
static FG: Lazy<PremultipliedColorU8> =
    Lazy::new(|| PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap());

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

fn draw_text(pixmap: &mut Pixmap, text: &str, posx: u32, posy: u32) {
    let font_data: &[u8] = include_bytes!("../BerkeleyMonoTrial-Regular.otf");
    let font = FontArc::try_from_slice(font_data).unwrap();

    let scale = PxScale::from(45.0);

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
                return;
            }

            pixmap.pixels_mut()[pos] = *FG;
        });
    }
}

fn draw_image(pixmap: &mut Pixmap, posx: u32, posy: u32) {
    let image_data: &[u8] = include_bytes!("../images/output.webp");
    let cursor = std::io::Cursor::new(image_data);
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
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];

            if r > 128 || g > 128 || b > 128 {
                continue;
            }

            let pos = ((y + posy) * pixmap_width + x + posx) as usize;

            if let Some(value) = pixmap.pixels_mut().get_mut(pos) {
                *value = *FG;
            }
        }
    }
}

fn main() {
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

    let mut rl = DefaultEditor::new().unwrap();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // let Ok(line) = rl.readline("> ") else {
        //     continue;
        // };

        // println!("{}", line);
        //clear pixmap make everything black
        pixmap.fill(Color::from_rgba8(
            BG.red(),
            BG.green(),
            BG.blue(),
            BG.alpha(),
        ));
        draw_image(&mut pixmap, 60, 60);
        // pixmap.

        draw_text(&mut pixmap, "Hello, world!", 10, 10);

        let buffer: Vec<u32> = pixmap
            .data()
            .chunks(4)
            .map(|rgba| {
                let r = rgba[0] as u32;
                let g = rgba[1] as u32;
                let b = rgba[2] as u32;
                let a = rgba[3] as u32;
                (a << 24) | (b << 16) | (g << 8) | r
            })
            .collect();

        window
            .update_with_buffer(&buffer, width as usize, height as usize)
            .unwrap();
    }
}
