use adapters::{NiimbotPrinterAdapter, UsbAdapter};
use color_eyre::{eyre::anyhow, Result};
use serialport::SerialPort;
use std::{thread::sleep, time::Duration};

pub mod adapters;

pub fn get_usb_adapter() -> Result<UsbAdapter> {
    let devices = rusb::devices().unwrap();
    // NIIMBOT B1: 3513:0002
    let niimbot = devices.iter().find(|d| {
        d.device_descriptor()
            .map(|desc| desc.vendor_id() == 0x3513)
            .unwrap_or(false)
    });

    match niimbot {
        Some(device) => {
            let handle = device.open()?;
            if handle.kernel_driver_active(0)? {
                handle.detach_kernel_driver(0)?;
            }
            handle.claim_interface(0)?;
            Ok(UsbAdapter::new(handle)?)
        }
        None => Err(anyhow!("No Niimbot found")),
    }
}

#[derive(Debug)]
pub struct NiimbotPacket {
    pub packet_type: u8,
    pub data: Vec<u8>,
}

impl NiimbotPacket {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes[..2] != [0x55, 0x55] || bytes[bytes.len() - 2..] != [0xaa, 0xaa] {
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
        let mut bytes = vec![0x55, 0x55, self.packet_type, self.data.len() as u8];
        bytes.extend(&self.data);

        let checksum: u8 =
            self.packet_type ^ bytes[3] ^ self.data.iter().copied().fold(0, |acc, x| acc ^ x);
        bytes.push(checksum);
        bytes.extend(&[0xaa, 0xaa]);

        bytes
    }
}

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
        header[2] = (mid_point as i32 - left) as u8;
        header[3] = (mid_point as i32 - right) as u8;
        header[4..6].copy_from_slice(&1_u16.to_be_bytes());

        image_data.push([header.to_vec(), line_data].concat());
    }

    image_data
}

pub struct NiimbotPrinterClient {
    pub adapter: Box<dyn NiimbotPrinterAdapter>,
}

impl NiimbotPrinterClient {
    pub fn new(adapter: Box<dyn NiimbotPrinterAdapter>) -> Result<Self> {
        Ok(Self { adapter })
    }

    pub fn send(&mut self, packet: NiimbotPacket) -> Result<usize> {
        let bytes = packet.to_bytes();
        self.adapter.send(&bytes)?;
        std::thread::sleep(Duration::from_millis(10));
        Ok(0)
    }

    fn recv(&mut self) -> Result<Vec<NiimbotPacket>> {
        let mut packets = Vec::new();
        let mut buffer = [0u8; 1024];
        let start_bytes = [0x55, 0x55];
        let end_bytes = [0xaa, 0xaa];
        let mut packet_buffer = Vec::new();

        let bytes_read = self.adapter.recv(&mut buffer)?;
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

    pub fn heartbeat(&mut self) -> Result<()> {
        self.transceive(220, &[0x01], 1)?;
        Ok(())
    }

    pub fn get_info(&mut self, info_type: u8) -> Result<Vec<u8>> {
        let response = self.transceive(64, &[info_type], 0)?;
        Ok(response.data)
    }

    pub fn transceive(
        &mut self,
        request_code: u8,
        data: &[u8],
        response_offset: u8,
    ) -> Result<NiimbotPacket> {
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

        Err(anyhow!("No response"))?
    }

    pub fn print_label(
        &mut self,
        image: &[u32],
        width: usize,
        height: usize,
        label_qty: u8,
        label_type: u8,
        label_density: u8,
    ) -> Result<()> {
        self.set_label_type(label_type)?;
        self.set_label_density(label_density)?;
        log::debug!("Starting print");
        self.start_print()?;
        log::debug!("Starting page print");
        self.start_page_print()?;
        log::debug!("Setting page size");

        self.set_page_size_v3(height as u16, width as u16, label_qty as u16)?;

        let packets = NiimbotPrinterClient::naive_encoder(width, height, image);
        dbg!(packets.len());
        for packet in packets {
            self.send(packet)?;
        }

        log::debug!("Start Print");
        self.end_page_print()?;
        log::debug!("Get Status");
        while self
            .get_print_status(label_qty as usize)?
            .get("page")
            .copied()
            .unwrap_or(0)
            != label_qty.into()
        {
            sleep(Duration::from_millis(100));
        }
        log::debug!("End Print");
        // self.end_print()?;

        Ok(())
    }

    fn set_label_type(&mut self, label_type: u8) -> Result<()> {
        self.transceive(35, &[label_type], 16).map(|_| ())
    }

    fn set_label_density(&mut self, density: u8) -> Result<()> {
        self.transceive(33, &[density], 16).map(|_| ())
    }

    fn start_print(&mut self) -> Result<()> {
        self.transceive(1, &[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00], 1)
            .map(|v| dbg!(v))
            .map(|_| ())
    }

    fn allow_print_clear(&mut self) -> Result<()> {
        self.transceive(32, &[0x01], 16).map(|_| ())
    }

    fn start_page_print(&mut self) -> Result<()> {
        self.transceive(3, &[0x01], 1).map(|_| ())
    }

    fn set_page_size_v3(&mut self, rows: u16, cols: u16, copies: u16) -> Result<()> {
        let bytes: Vec<u8> =
            [rows.to_be_bytes(), cols.to_be_bytes(), copies.to_be_bytes()].concat();

        // let bytes = [0x00, 0xf0, 0x01, 0x90, 0x00, 0x01];
        let packet = NiimbotPacket {
            packet_type: 0x13,
            data: bytes,
        };

        self.send(packet)?;

        Ok(())
    }

    fn end_page_print(&mut self) -> Result<()> {
        self.transceive(0xe3, &[0x01], 1).map(|_| ())
    }

    fn end_print(&mut self) -> Result<()> {
        self.transceive(243, &[0x01], 1).map(|_| ())
    }

    fn set_dimension(&mut self, width: u16, height: u16) -> Result<()> {
        let dimension_bytes = [
            (width >> 8) as u8,
            width as u8,
            (height >> 8) as u8,
            height as u8,
        ];
        self.transceive(19, &dimension_bytes, 1).map(|_| ())
    }

    fn set_quantity(&mut self, quantity: u8) -> Result<()> {
        self.transceive(21, &[quantity], 1).map(|_| ())
    }

    pub fn get_print_status(
        &mut self,
        quantity: usize,
    ) -> Result<std::collections::HashMap<String, usize>> {
        // dumbass printer stop responding to print status packets after its done printing but that is usually ver7 quickly
        let response = match self.transceive(0xb3, &[0x01], 16) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to get print status: {:?}", e);
                return Ok([
                    ("page".into(), quantity),
                    ("progress1".into(), 100),
                    ("progress2".into(), 100),
                ]
                .iter()
                .cloned()
                .collect());
            }
        };
        let data = response.data;
        if data.len() < 4 {
            return Err(anyhow!("Invalid response"));
        }
        let page = u16::from_be_bytes([data[0], data[1]]) as usize;
        let progress1 = data[2] as usize;
        let progress2 = data[3] as usize;

        Ok([
            ("page".into(), page),
            ("progress1".into(), progress1),
            ("progress2".into(), progress2),
        ]
        .iter()
        .cloned()
        .collect())
    }
}
