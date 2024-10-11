use color_eyre::{eyre::anyhow, Result};
use rusb::{Context, Device, DeviceHandle, GlobalContext, UsbContext};
use std::error::Error;
use std::thread;
use std::time::Duration;

// ISSUE: i forgor

const VENDOR_ID: u16 = 0x0483; // Replace with actual vendor ID
const PRODUCT_ID: u16 = 0x5740; // Replace with actual product ID

#[derive(Debug, Clone, Copy)]
enum InfoEnum {
    Density = 1,
    PrintSpeed = 2,
    LabelType = 3,
    LanguageType = 6,
    AutoShutdownTime = 7,
    DeviceType = 8,
    SoftVersion = 9,
    Battery = 10,
    DeviceSerial = 11,
    HardVersion = 12,
}

#[derive(Debug, Clone, Copy)]
enum RequestCodeEnum {
    GetInfo = 0x40,
    GetRfid = 0x1A,
    Heartbeat = 0xDC,
    SetLabelType = 0x23,
    SetLabelDensity = 0x21,
    StartPrint = 0x01,
    EndPrint = 0xF3,
    StartPagePrint = 0x03,
    EndPagePrint = 0xE3,
    AllowPrintClear = 0x20,
    SetDimension = 0x13,
    SetQuantity = 0x15,
    GetPrintStatus = 0xA3,
}

pub struct NiimbotPacket {
    packet_type: u8,
    data: Vec<u8>,
}

impl NiimbotPacket {
    fn new(packet_type: u8, data: Vec<u8>) -> Self {
        Self { packet_type, data }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if &bytes[..2] != &[0x55, 0x55] || &bytes[bytes.len() - 2..] != &[0xaa, 0xaa] {
            return Err(anyhow!("Invalid packet boundaries"));
        }

        let packet_type = bytes[2];
        let len = bytes[3] as usize;
        let data = bytes[4..4 + len].to_vec();

        let computed_checksum =
            packet_type ^ (len as u8) ^ data.iter().copied().fold(0, |acc, x| acc ^ x);
        if bytes[4 + len] != computed_checksum {
            return Err(anyhow!("Invalid checksum"));
        }

        Ok(NiimbotPacket { packet_type, data })
    }

    // pub fn to_bytes(&mut self) -> Vec<u8> {
    //     dbg!(self.data.len());
    //     let mut bytes = vec![0x55, 0x55, self.packet_type, self.data.len() as u8];
    //     bytes.extend(&mut self.data);

    //     let checksum: u8 =
    //         self.packet_type ^ bytes[3] ^ self.data.iter().copied().fold(0, |acc, x| acc ^ x);
    //     bytes.push(checksum);
    //     bytes.extend(&[0xaa, 0xaa]);

    //     bytes
    // }

    fn to_bytes(&self) -> Vec<u8> {
        let mut result = vec![0x55, 0x55, self.packet_type, self.data.len() as u8];
        result.extend_from_slice(&self.data);

        let mut checksum = self.packet_type ^ (self.data.len() as u8);
        for &byte in &self.data {
            checksum ^= byte;
        }

        result.push(checksum);
        result.extend_from_slice(&[0xAA, 0xAA]);
        result
    }
}

pub struct PrinterClient {
    handle: DeviceHandle<GlobalContext>,
}

impl PrinterClient {
    pub fn new(handle: DeviceHandle<GlobalContext>) -> Result<Self> {
        Ok(Self { handle })
    }

    fn recv(&mut self) -> Result<Vec<NiimbotPacket>> {
        let mut packets = Vec::new();
        let mut buffer = [0u8; 1024];
        let start_bytes = [0x55, 0x55];
        let end_bytes = [0xaa, 0xaa];
        let mut packet_buffer = Vec::new();

        let bytes_read = self
            .handle
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

    fn send_command(&mut self, request_code: RequestCodeEnum, data: &[u8]) -> Result<()> {
        let mut packet = NiimbotPacket::new(request_code as u8, data.to_vec());
        let bytes = packet.to_bytes();

        self.handle.write_bulk(1, &bytes, Duration::from_secs(5))?;

        Ok(())
    }

    fn transceive(&mut self, request_code: RequestCodeEnum, data: &[u8]) -> Result<NiimbotPacket> {
        self.send_command(request_code, data)?;
        dbg!("Command Send!");

        while let Some(packet) = self.recv()?.into_iter().next() {
            if packet.packet_type == request_code as u8 + 1 {
                return Ok(packet);
            }
        }
        // if let Some(packet) = self.recv()?.into_iter().next() {
        //     return Ok(packet);
        // }
        Err(anyhow!("No Packets"))
    }

    fn get_info(&mut self, key: InfoEnum) -> Result<u32> {
        let response = self.transceive(RequestCodeEnum::GetInfo, &[key as u8])?;
        Ok(u32::from_be_bytes(response.data.try_into().unwrap()))
    }

    fn set_label_type(&mut self, n: u8) -> Result<bool> {
        assert!(1 <= n && n <= 3);
        let packet = self.transceive(RequestCodeEnum::SetLabelType, &[n])?;
        Ok(packet.data[0] != 0)
    }

    fn set_label_density(&mut self, n: u8) -> Result<bool> {
        assert!(1 <= n && n <= 5);
        let packet = self.transceive(RequestCodeEnum::SetLabelDensity, &[n])?;
        Ok(packet.data[0] != 0)
    }

    fn start_print(&mut self) -> Result<bool> {
        let packet = self.transceive(RequestCodeEnum::StartPrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn end_print(&mut self) -> Result<bool> {
        let packet = self.transceive(RequestCodeEnum::EndPrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn start_page_print(&mut self) -> Result<bool> {
        let packet = self.transceive(RequestCodeEnum::StartPagePrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn end_page_print(&mut self) -> Result<bool> {
        let packet = self.transceive(RequestCodeEnum::EndPagePrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn set_dimension(&mut self, w: u16, h: u16) -> Result<bool> {
        let data = [(w >> 8) as u8, w as u8, (h >> 8) as u8, h as u8];
        let packet = self.transceive(RequestCodeEnum::SetDimension, &data)?;
        Ok(packet.data[0] != 0)
    }

    fn set_quantity(&mut self, n: u16) -> Result<bool> {
        let data = [(n >> 8) as u8, n as u8];
        let packet = self.transceive(RequestCodeEnum::SetQuantity, &data)?;
        Ok(packet.data[0] != 0)
    }

    fn get_print_status(&mut self) -> Result<(u16, u8, u8)> {
        let packet = self.transceive(RequestCodeEnum::GetPrintStatus, &[0x01])?;
        let page = u16::from_be_bytes([packet.data[0], packet.data[1]]);
        let progress1 = packet.data[2];
        let progress2 = packet.data[3];
        Ok((page, progress1, progress2))
    }

    pub fn print_image(
        &mut self,
        image_data: &[u32],
        width: u16,
        height: u16,
        density: u8,
        quantity: u16,
    ) -> Result<()> {
        dbg!("Setting Density");
        self.set_label_density(density)?;
        dbg!("Setting Type");
        self.set_label_type(1)?;
        self.start_print()?;
        self.start_page_print()?;
        self.set_dimension(width, height)?;
        self.set_quantity(quantity)?;

        for y in 0..height {
            let mut line_data = Vec::new();
            for x in 0..width {
                let pixel = image_data[(y as usize * width as usize + x as usize)];
                let alpha = (pixel >> 24) & 0xFF;
                line_data.push(if alpha > 128 { 1 } else { 0 });
            }

            let mut packed_data = Vec::new();
            for chunk in line_data.chunks(8) {
                let mut byte = 0u8;
                for (i, &bit) in chunk.iter().enumerate() {
                    byte |= bit << (7 - i);
                }
                packed_data.push(byte);
            }

            let mut header = vec![(y >> 8) as u8, y as u8, 0, 0, 0, 1];
            header.extend_from_slice(&packed_data);

            self.send_command(RequestCodeEnum::GetInfo, &header)?;
            std::thread::sleep(Duration::from_millis(10));
        }

        while !self.end_page_print()? {
            std::thread::sleep(Duration::from_millis(50));
        }

        loop {
            let (page, _, _) = self.get_print_status()?;
            if page == quantity {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        self.end_print()?;

        Ok(())
    }
}
