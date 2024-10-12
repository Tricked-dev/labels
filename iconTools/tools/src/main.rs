use std::{
    fs::{DirEntry, File},
    io::{Cursor, Seek},
    sync::{Arc, Mutex},
};

use image::{DynamicImage, ImageDecoder, ImageReader, Limits};
use rayon::{prelude::*, ThreadPoolBuilder};
use resvg::{tiny_skia, usvg};
use serde_json::Value;
use tar::Builder;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let file = File::create("out/images.tar").unwrap();
    let a = Arc::new(Mutex::new(Builder::new(file)));

    let name_blacklist = [
        "flag", "covid", "moji", "devicon", "cif", "park", "noto", "crypto", "market", "skill",
    ];

    ThreadPoolBuilder::new()
        .num_threads(12)
        .build_global()
        .unwrap();

    std::fs::read_dir("../icon-sets/json")
        .unwrap()
        .flatten()
        .collect::<Vec<DirEntry>>()
        .into_par_iter()
        .for_each(|file| {
            if name_blacklist.iter().any(|&v| file.file_name().to_str().unwrap().contains(v)) {
                return;
            }
            let data: Value =
                serde_json::from_str(&std::fs::read_to_string(file.path()).unwrap()).unwrap();
            let name = data["prefix"].as_str().unwrap();
            let height = data["info"]["height"].as_u64();
            let width = data["info"]["width"].as_u64();

            let _ = data["icons"].as_object().unwrap().into_iter().collect::<Vec<_>>().chunks(20).map(|chunk| {
                chunk.into_par_iter().for_each(|(key, value)| {
                let svg = value["body"].as_str().unwrap();
                let height = value["height"].as_u64().unwrap_or(height.unwrap_or(24));
                let width = value["width"].as_u64().unwrap_or(width.unwrap_or(height));
                let svg = &format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{svg}</svg>",
                );

                let mut buffer = {
                    let mut img = {
                        let encode_png = {
                            let tree = usvg::Tree::from_str(svg, &opt).unwrap();
                            let pixmap_size = tree.size().to_int_size();
                            let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
                            resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());


                            pixmap.encode_png().unwrap()
                        };

                        let cursor = std::io::Cursor::new(&encode_png);
                        let mut reader = ImageReader::new(cursor);
                        reader.set_format(image::ImageFormat::Png);
                        let mut decoder = reader.into_decoder().unwrap();
                        decoder.set_limits(Limits::no_limits()).unwrap();
                        let dynamic_image = DynamicImage::from_decoder(decoder).unwrap();
                        dynamic_image.to_rgba8()
                    };


                    let threshold = 128u16;
                    let mut colored_pixels = 0;

                    for pixel in img.pixels_mut() {
                        let avg = (pixel[0] as u16 + pixel[1] as u16 + pixel[2] as u16) / 3;
                        if avg > threshold {
                            *pixel = image::Rgba([0, 0, 0, 255]);
                            colored_pixels += 1;
                        } else {
                            *pixel = image::Rgba([0, 0, 0, 0]);
                        }
                    }

                    if colored_pixels < 20 {
                        return;
                    }

                    let mut buffer = Cursor::new(Vec::new());
                    if img.write_to(&mut buffer, image::ImageFormat::WebP).is_err() {
                        return;
                    };
                    buffer
                };

                let mut header = tar::Header::new_gnu();
                buffer.seek(std::io::SeekFrom::Start(0)).unwrap();
                header.set_size(buffer.get_ref().len() as u64);
                header.set_cksum();
                header.set_mode(0o644);

                a.lock().unwrap().append_data(&mut header, format!("{name}:{key}.webp"), &mut buffer).unwrap();
             })
            }).collect::<Vec<_>>();
        });
}
