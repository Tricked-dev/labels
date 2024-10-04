use std::time::Instant;

use ab_glyph::{point, Glyph, Point, ScaleFont};
use ab_glyph::{Font, FontArc};
use minifb::{Key, Scale, Window, WindowOptions};
use rustyline::DefaultEditor;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Transform};

use ab_glyph::PxScale;

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

    // to work out the exact size needed for the drawn glyphs we need to outline
    // them and use their `px_bounds` which hold the coords of their render bounds.
    let outlined: Vec<_> = glyphs
        .into_iter()
        // Note: not all layout glyphs have outlines (e.g. " ")
        .filter_map(|g| font.outline_glyph(g))
        .collect();

    // combine px_bounds to get min bounding coords for the entire layout
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

    // create a new rgba image using the combined px bound width and height

    // Loop through the glyphs in the text, positing each one on a line
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

            pixmap.pixels_mut()[pos] = PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap();
        });
    }
}

fn main() {
    let width = 400;
    let height = 400;

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
        let Ok(line) = rl.readline("> ") else {
            continue;
        };

        println!("{}", line);
        //clear pixmap make everything black
        pixmap.fill(Color::from_rgba8(0, 0, 0, 0));
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
