use color_eyre::{eyre::anyhow, Result};
use serialport::SerialPort;
use std::error::Error;
use std::thread;
use std::time::Duration;

//ISSUE: packets eem to send fine except that it doesnt start printing

const DEVICE_PATH: &str = "/dev/ttyDUMMY";
const BAUD_RATE: u32 = 115200; // Adjust this based on your printer's specifications
                               // const BAUD_RATE: u32 = 9600;

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
    SendLineData = 0x85,
}

struct NiimbotPacket {
    packet_type: u8,
    data: Vec<u8>,
}

impl NiimbotPacket {
    fn new(packet_type: u8, data: Vec<u8>) -> Self {
        Self { packet_type, data }
    }

    fn from_bytes(pkt: &[u8]) -> Result<Self> {
        if pkt == [0x55, 0x55, 0x00] {
            return Ok(NiimbotPacket {
                packet_type: 0,
                data: vec![],
            });
        }

        if &pkt[..2] != &[0x55, 0x55] || &pkt[pkt.len() - 2..] != &[0xaa, 0xaa] {
            return Err(anyhow!("Invalid packet boundaries"));
        }

        let packet_type = pkt[2];
        let len = pkt[3] as usize;
        let data = pkt[4..4 + len].to_vec();

        let mut checksum = packet_type ^ (len as u8);
        for &byte in &data {
            checksum ^= byte;
        }

        if checksum != pkt[pkt.len() - 3] {
            dbg!(&pkt);
            // return Err(anyhow!("Checksum mismatch"));
        }

        Ok(Self { packet_type, data })
    }

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
    port: Box<dyn SerialPort>,
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
        header[2] = (mid_point as i32 - left as i32) as u8;
        header[3] = (mid_point as i32 - right as i32) as u8;
        dbg!(mid_point as i32 - left as i32);
        dbg!(mid_point as i32 - right as i32);
        header[4..6].copy_from_slice(&(1 as u16).to_be_bytes());

        image_data.push([header.to_vec(), line_data].concat());
    }

    image_data
}

impl PrinterClient {
    pub fn new() -> Result<Self> {
        let port = serialport::new(DEVICE_PATH, BAUD_RATE)
            .timeout(Duration::from_millis(10))
            .open()?;

        Ok(Self { port })
    }

    fn send_command(
        &mut self,
        request_code: RequestCodeEnum,
        data: &[u8],
    ) -> Result<NiimbotPacket> {
        self.send_no_read(request_code, data)?;
        thread::sleep(Duration::from_millis(10));
        let mut buf = [0u8; 1024];
        let len = self.port.read(&mut buf)?;

        NiimbotPacket::from_bytes(&buf[..len])
    }

    fn send_no_read(&mut self, request_code: RequestCodeEnum, data: &[u8]) -> Result<()> {
        let packet = NiimbotPacket::new(request_code as u8, data.to_vec());
        let bytes = packet.to_bytes();

        self.port.write_all(&bytes)?;

        Ok(())
    }

    fn get_info(&mut self, key: InfoEnum) -> Result<u32> {
        let response = self.send_command(RequestCodeEnum::GetInfo, &[key as u8])?;
        Ok(u32::from_be_bytes(response.data.try_into().unwrap()))
    }

    fn set_label_type(&mut self, n: u8) -> Result<bool> {
        assert!(1 <= n && n <= 3);
        let packet = self.send_command(RequestCodeEnum::SetLabelType, &[n])?;
        Ok(packet.data[0] != 0)
    }

    fn set_label_density(&mut self, n: u8) -> Result<bool> {
        assert!(1 <= n && n <= 5);
        let packet = self.send_command(RequestCodeEnum::SetLabelDensity, &[n])?;
        Ok(packet.data[0] != 0)
    }

    fn start_print(&mut self) -> Result<bool> {
        let packet = self.send_command(RequestCodeEnum::StartPrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn end_print(&mut self) -> Result<bool> {
        let packet = self.send_command(RequestCodeEnum::EndPrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn start_page_print(&mut self) -> Result<bool> {
        let packet = self.send_command(RequestCodeEnum::StartPagePrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn end_page_print(&mut self) -> Result<bool> {
        let packet = self.send_command(RequestCodeEnum::EndPagePrint, &[0x01])?;
        Ok(packet.data[0] != 0)
    }

    fn set_dimension(&mut self, w: u16, h: u16) -> Result<bool> {
        let data = [(w >> 8) as u8, w as u8, (h >> 8) as u8, h as u8];
        let packet = self.send_command(RequestCodeEnum::SetDimension, &data)?;
        Ok(packet.data[0] != 0)
    }

    fn set_quantity(&mut self, n: u16) -> Result<bool> {
        let data = [(n >> 8) as u8, n as u8];
        let packet = self.send_command(RequestCodeEnum::SetQuantity, &data)?;
        Ok(packet.data[0] != 0)
    }

    fn get_print_status(&mut self) -> Result<(u16, u8, u8)> {
        let packet = self.send_command(RequestCodeEnum::GetPrintStatus, &[0x01])?;
        dbg!(&packet.data);
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
        dbg!("Setting Density!");
        self.set_label_density(density)?;
        dbg!("Setting Label Type");
        self.set_label_type(1)?;
        self.start_print()?;
        self.start_page_print()?;
        self.set_dimension(width, height)?;
        self.set_quantity(quantity)?;

        for data in prepare_image(image_data, width as usize, height as usize).into_iter() {
            dbg!("Sending Line");
            std::thread::sleep(Duration::from_millis(2));
            self.send_no_read(RequestCodeEnum::SendLineData, &data)?;
        }

        std::thread::sleep(Duration::from_millis(5));

        println!("Setting Something");

        self.end_page_print()?;

        println!("End Page Print");
        loop {
            let (page, _, _) = self.get_print_status()?;
            dbg!(&page);
            if page == quantity {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        println!("End Print");
        self.end_print()?;

        Ok(())
    }
}
