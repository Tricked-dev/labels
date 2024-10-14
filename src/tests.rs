// use crate::{
//     drawing::draw_text,
//     niimbot::{get_usb_adapter, NiimbotPrinterClient},
//     CONFIG,
// };
use color_eyre::Result;

use niimbot::{get_usb_adapter, NiimbotPrinterClient};

#[test]
fn test_shutdowns() -> Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let mut printer = NiimbotPrinterClient::new(Box::new(get_usb_adapter()?))?;
    printer.heartbeat()?;

    printer.set_autoshutdown_time(1)?;

    Ok(())
}

// #[test]
// fn main() -> Result<()> {
//     let width = CONFIG.width;
//     let height = CONFIG.height;

//     dbg!(width);
//     dbg!(height);

//     // Create a pixmap
//     let mut pixmap = Pixmap::new(width as u32, height as u32).unwrap();

//     // draw_text(&mut pixmap, "Hello world", 4, 0, 0)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 30)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 60)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 90)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 100)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 120)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 150)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 200)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 220)?;
//     // draw_text(&mut pixmap, "Hello world", 4, 0, 230)?;

//     let buffer: Vec<u32> = pixmap
//         .data()
//         .chunks(4)
//         .map(|rgba| {
//             let r = 255 - rgba[0] as u32;
//             let g = 255 - rgba[1] as u32;
//             let b = 255 - rgba[2] as u32;
//             let a = rgba[3] as u32; // Keep the alpha value the same
//             (a << 24) | (b << 16) | (g << 8) | r
//         })
//         .collect();

//     let adapter = get_usb_adapter()?;

//     let mut client = NiimbotPrinterClient::new(Box::new(adapter))?;

//     client.heartbeat().unwrap();
//     // client.get_print_status().unwrap();
//     client.print_label(&buffer, width as usize, height as usize, 1, 1, 5)?;

//     Ok(())
// }
