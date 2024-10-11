use rusb::{Context, DeviceHandle, Error as UsbError, GlobalContext, UsbContext};
use std::{thread::sleep, time::Duration};

//ISSUE: starts printing but then printer stops responding and i have to replug it

#[derive(Debug)]
pub struct NiimbotPacket {
    pub packet_type: u8,
    pub data: Vec<u8>,
}
use std::io::Cursor;

fn prepare_image(image: &[u32], width: usize, height: usize) -> Vec<Vec<u8>> {
    if width % 8 != 0 {
        eprintln!("Image width not a multiple of 8");
    }

    let mid_point = width / 2;
    let mut image_data = Vec::new();

    for y in 0..height {
        let col_index = y * width;
        let pixels = &image[col_index..col_index + width];

        let mut bits = String::new();
        let mut bytes = Vec::new();
        let mut left = 0;
        let mut right = 0;

        for (index, &pixel) in pixels.iter().enumerate() {
            let bit = if pixel & 0xFF > 0 { '0' } else { '1' };

            if bit == '1' {
                if index < mid_point {
                    left += 1;
                } else {
                    right += 1;
                }
            }

            bits.push(bit);

            if bits.len() == 8 {
                let byte = u8::from_str_radix(&bits, 2).unwrap();
                bytes.push(byte);
                bits.clear();
            }
        }

        let line_data = bytes;
        let mut header = [0u8; 6];

        header[0..2].copy_from_slice(&(y as u16).to_be_bytes());
        // header[2] = 0;
        // header[3] = 0;
        header[2] = (mid_point as i32 - left as i32) as u8;
        header[3] = (mid_point as i32 - right as i32) as u8;
        dbg!(mid_point as i32 - left as i32);
        dbg!(mid_point as i32 - right as i32);
        header[4..6].copy_from_slice(&(1 as u16).to_be_bytes());

        image_data.push([header.to_vec(), line_data].concat());
    }

    image_data
}

impl NiimbotPacket {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if &bytes[..2] != &[0x55, 0x55] || &bytes[bytes.len() - 2..] != &[0xaa, 0xaa] {
            return Err("Invalid packet boundaries".to_string());
        }

        let packet_type = bytes[2];
        let len = bytes[3] as usize;
        let data = bytes[4..4 + len].to_vec();

        let computed_checksum =
            packet_type ^ (len as u8) ^ data.iter().copied().fold(0, |acc, x| acc ^ x);
        if bytes[4 + len] != computed_checksum {
            return Err("Invalid checksum".to_string());
        }

        Ok(NiimbotPacket { packet_type, data })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        dbg!(self.data.len());
        let mut bytes = vec![0x55, 0x55, self.packet_type, self.data.len() as u8];
        bytes.extend(&self.data);

        let checksum: u8 =
            self.packet_type ^ bytes[3] ^ self.data.iter().copied().fold(0, |acc, x| acc ^ x);
        bytes.push(checksum);
        bytes.extend(&[0xaa, 0xaa]);

        bytes
    }
}

pub struct NiimbotPrinterClient {
    pub device_handle: DeviceHandle<GlobalContext>,
}

impl NiimbotPrinterClient {
    pub fn new(device_handle: DeviceHandle<GlobalContext>) -> Result<Self, UsbError> {
        Ok(Self { device_handle })
    }

    pub fn send(&mut self, packet: NiimbotPacket) -> Result<usize, UsbError> {
        let bytes = packet.to_bytes();
        self.device_handle
            .write_bulk(0x01, &bytes, Duration::from_secs(1))
    }

    fn recv(&mut self) -> Result<Vec<NiimbotPacket>, UsbError> {
        let mut packets = Vec::new();
        let mut buffer = [0u8; 1024];
        let start_bytes = [0x55, 0x55];
        let end_bytes = [0xaa, 0xaa];
        let mut packet_buffer = Vec::new();

        let bytes_read = self
            .device_handle
            .read_bulk(0x81, &mut buffer, Duration::from_secs(1))?;
        // dbg!("Bytes read: {}", bytes_read);
        let mut position = 0;
        while position < bytes_read {
            // Accumulate packets
            packet_buffer.push(buffer[position]);
            position += 1;

            // Check if we have a potential packet start
            if packet_buffer.len() >= 4
                && packet_buffer[0..2] == start_bytes
                && packet_buffer.ends_with(&end_bytes)
            {
                if let Ok(packet) = NiimbotPacket::from_bytes(&packet_buffer) {
                    packets.push(packet);
                    packet_buffer.clear(); // Reset for the next packet
                } else {
                    // If invalid packet, clear buffer and continue searching
                    packet_buffer.clear();
                }
            }
        }
        Ok(packets)
    }

    pub fn naive_encoder(width: usize, height: usize, img: &[u32]) -> Vec<NiimbotPacket> {
        prepare_image(img, width, height)
            .into_iter()
            .map(|d| NiimbotPacket {
                packet_type: 0x85,
                data: d,
            })
            .collect()
    }

    pub fn heartbeat(&mut self) -> Result<(), UsbError> {
        let response = self.transceive(220, &[0x01], 1)?;
        // Process the response data
        println!("Heartbeat response: {:?}", response);
        Ok(())
    }

    pub fn get_info(&mut self, info_type: u8) -> Result<Vec<u8>, UsbError> {
        let response = self.transceive(64, &[info_type], 0)?;
        Ok(response.data)
    }

    pub fn transceive(
        &mut self,
        request_code: u8,
        data: &[u8],
        response_offset: u8,
    ) -> Result<NiimbotPacket, UsbError> {
        let packet = NiimbotPacket {
            packet_type: request_code,
            data: data.to_vec(),
        };

        self.send(packet)?;

        // dbg!("Packet send");

        // Simple retry loop for receiving the expected response
        for _ in 0..5 {
            if let Ok(response) = self.recv() {
                for packet in response {
                    // dbg!(&packet);
                    if packet.packet_type == request_code + response_offset {
                        return Ok(packet);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        Err(rusb::Error::Io)
    }

    pub fn print_label(
        &mut self,
        image: &[u32],
        width: usize,
        height: usize,
        label_qty: u8,
        label_type: u8,
        label_density: u8,
    ) -> Result<(), UsbError> {
        // Resize and rotate the image if necessary
        // let packets = NiimbotPrinterClient::naive_encoder(width, height, image);

        // self.set_label_type(label_type)?;
        // self.set_label_density(label_density)?;
        self.start_print()?;
        // self.allow_print_clear()?;
        self.start_page_print()?;
        self.set_page_size_v3(width as u16, height as u16, label_qty as u16)?;
        // self.set_dimension(width as u16, height as u16)?;
        // self.set_quantity(label_qty)?;

        // Convert the image to packets using naive_encoder and send them
        let packets = NiimbotPrinterClient::naive_encoder(width, height, image);
        for packet in packets {
            self.send(packet)?;
        }

        // dbg!("Image Packet send!");
        // dbg!("End Psage");
        self.end_page_print()?;
        // dbg!("Send Page print");
        // while self.get_print_status()?.get("page").copied().unwrap_or(0) != label_qty.into() {
        //     sleep(Duration::from_millis(300));
        // }
        // dbg!("End Print");

        // // dbg!("End print!");
        // self.end_print()?;

        Ok(())
    }

    fn set_label_type(&mut self, label_type: u8) -> Result<(), UsbError> {
        self.transceive(35, &[label_type], 16).map(|_| ())
    }

    fn set_label_density(&mut self, density: u8) -> Result<(), UsbError> {
        self.transceive(33, &[density], 16).map(|_| ())
    }

    fn start_print(&mut self) -> Result<(), UsbError> {
        self.transceive(1, &[0x01], 1).map(|_| ())
    }

    fn allow_print_clear(&mut self) -> Result<(), UsbError> {
        self.transceive(32, &[0x01], 16).map(|_| ())
    }

    fn start_page_print(&mut self) -> Result<(), UsbError> {
        self.transceive(3, &[0x01], 1).map(|_| ())
    }

    fn set_page_size_v3(&mut self, rows: u16, cols: u16, copies: u16) -> Result<(), UsbError> {
        let bytes: Vec<u8> =
            [rows.to_be_bytes(), cols.to_be_bytes(), copies.to_be_bytes()].concat();

        let packet = NiimbotPacket {
            packet_type: 0x13,
            data: bytes,
        };

        self.send(packet)?;

        Ok(())
    }

    fn end_page_print(&mut self) -> Result<(), UsbError> {
        self.transceive(0xe3, &[0x01], 1).map(|_| ())
    }

    fn end_print(&mut self) -> Result<(), UsbError> {
        self.transceive(243, &[0x01], 1).map(|_| ())
    }

    fn set_dimension(&mut self, width: u16, height: u16) -> Result<(), UsbError> {
        let dimension_bytes = [
            (width >> 8) as u8,
            width as u8,
            (height >> 8) as u8,
            height as u8,
        ];
        self.transceive(19, &dimension_bytes, 1).map(|_| ())
    }

    fn set_quantity(&mut self, quantity: u8) -> Result<(), UsbError> {
        self.transceive(21, &[quantity], 1).map(|_| ())
    }

    fn get_print_status(&mut self) -> Result<std::collections::HashMap<String, usize>, UsbError> {
        let response = self.transceive(163, &[0x01], 16)?;
        let data = response.data;
        if data.len() < 4 {
            return Err(rusb::Error::Io);
        }
        let page = u16::from_be_bytes([data[0], data[1]]) as usize;
        let progress1 = data[2] as usize;
        let progress2 = data[3] as usize;

        dbg!(page);
        dbg!(progress1);
        dbg!(progress2);

        Ok([
            ("page".into(), page),
            ("progress1".into(), progress1),
            ("progress2".into(), progress2),
        ]
        .iter()
        .cloned()
        .collect())
    }

    // Similar implementations for other commands such as setLabelType, startPrint, etc. can be added here
}

// fn main() -> Result<(), UsbError> {
//     let mut printer = NiimbotPrinterClient::new(0x1234, 0x5678)?;
//     let img = image::open("path/to/image.png").expect("Failed to open image");

//     printer.heartbeat()?;
//     let info = printer.get_info(1)?;
//     println!("Printer info: {:?}", info);

//     printer.print_label(img, 300, 300)?;

//     Ok(())
// }
