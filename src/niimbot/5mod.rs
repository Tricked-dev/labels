use serialport::{SerialPort, SerialPortType};
use std::io::{Read, Write};
use std::time::Duration;

const PACKET_HEAD: [u8; 2] = [0x55, 0x55];
const PACKET_TAIL: [u8; 2] = [0xaa, 0xaa];

#[derive(Debug, Clone, Copy)]
pub enum RequestCommandId {
    Invalid = -1,
    Connect = 0xc1,
    CancelPrint = 0xda,
    Heartbeat = 0xdc,
    PageEnd = 0xe3,
    PageStart = 0x03,
    PrintBitmapRow = 0x85,
    PrintBitmapRowIndexed = 0x83,
    PrintEmptyRow = 0x84,
    PrintEnd = 0xf3,
    PrinterInfo = 0x40,
    PrintStart = 0x01,
    PrintStatus = 0xa3,
    SetDensity = 0x21,
    SetLabelType = 0x23,
    SetPageSize = 0x13,
}

pub fn request_code_to_id(code: u8) -> RequestCommandId {
    match code {
        0x40 => RequestCommandId::PrinterInfo,
        0x01 => RequestCommandId::PrintStart,
        0x21 => RequestCommandId::SetDensity,
        0x23 => RequestCommandId::SetLabelType,
        0x13 => RequestCommandId::SetPageSize,
        0x84 => RequestCommandId::PrintBitmapRow,
        0x01 => RequestCommandId::PageStart,
        0xf3 => RequestCommandId::PrintEnd,
        0x03 => RequestCommandId::PageEnd,
        _ => RequestCommandId::Invalid,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ResponseCommandId {
    Invalid = -1,
    In_Connect = 0xc2,
    In_PageStart = 0x04,
    In_PrintEnd = 0xf4,
    In_PrintStatus = 0xb3,
    In_PrintError = 0xdb,
    In_PrintStart = 0x02,
    In_SetDensity = 0x31,
    In_SetLabelType = 0x33,
    In_SetPageSize = 0x14,
    In_PageEnd = 0xe4,
}

#[derive(Debug)]
pub struct NiimbotPacket {
    command: RequestCommandId,
    data: Vec<u8>,
    valid_response_ids: Vec<ResponseCommandId>,
}

impl NiimbotPacket {
    pub fn new(
        command: RequestCommandId,
        data: Vec<u8>,
        valid_response_ids: Vec<ResponseCommandId>,
    ) -> Self {
        NiimbotPacket {
            command,
            data,
            valid_response_ids,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&PACKET_HEAD);
        packet.push(self.command as u8);
        packet.push(self.data.len() as u8);
        packet.extend_from_slice(&self.data);
        packet.push(self.checksum());
        packet.extend_from_slice(&PACKET_TAIL);
        packet
    }

    fn checksum(&self) -> u8 {
        let mut checksum = 0u8;
        checksum ^= self.command as u8;
        checksum ^= self.data.len() as u8;
        for &byte in &self.data {
            checksum ^= byte;
        }
        checksum
    }
}

pub struct NiimbotPrinter {
    port: Box<dyn SerialPort>,
}

impl NiimbotPrinter {
    pub fn new(port_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let port = serialport::new(port_name, 115_200)
            .timeout(Duration::from_millis(10))
            .open()?;

        Ok(NiimbotPrinter { port })
    }

    pub fn send_packet(
        &mut self,
        packet: &NiimbotPacket,
    ) -> Result<(), Box<dyn std::error::Error>> {
        dbg!(packet);
        let bytes = packet.to_bytes();
        self.port.write_all(&bytes)?;
        Ok(())
    }

    pub fn receive_packet(&mut self) -> Result<NiimbotPacket, Box<dyn std::error::Error>> {
        let mut buffer = [0u8; 1024];
        let bytes_read = self.port.read(&mut buffer)?;

        // Parse the received packet
        // This is a simplified version and might need more robust implementation
        if bytes_read < 6 {
            return Err("Received packet is too short".into());
        }

        let command = buffer[2];
        let data_len = buffer[3] as usize;
        let data = buffer[4..4 + data_len].to_vec();

        Ok(NiimbotPacket::new(
            request_code_to_id(command),
            data,
            vec![],
        ))
    }

    pub fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let connect_packet = NiimbotPacket::new(
            RequestCommandId::Connect,
            vec![1],
            vec![ResponseCommandId::In_Connect],
        );
        self.send_packet(&connect_packet)?;

        std::thread::sleep(Duration::from_millis(10));
        // Wait for response
        let response = self.receive_packet()?;
        dbg!(&response);
        if response.command as u8 != ResponseCommandId::In_Connect as u8 {
            return Err("Unexpected response to connect command".into());
        }

        Ok(())
    }

    pub fn print_image(
        &mut self,
        image: &Vec<u32>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Start print job
        self.send_packet(&NiimbotPacket::new(
            RequestCommandId::PrintStart,
            vec![1],
            vec![ResponseCommandId::In_PrintStart],
        ))?;

        // Set page size
        let page_size_data = [
            (height & 0xFF) as u8,
            ((height >> 8) & 0xFF) as u8,
            (width & 0xFF) as u8,
            ((width >> 8) & 0xFF) as u8,
        ];
        self.send_packet(&NiimbotPacket::new(
            RequestCommandId::SetPageSize,
            page_size_data.to_vec(),
            vec![ResponseCommandId::In_SetPageSize],
        ))?;

        // Start page
        self.send_packet(&NiimbotPacket::new(
            RequestCommandId::PageStart,
            vec![1],
            vec![ResponseCommandId::In_PageStart],
        ))?;

        // Print image data
        for (row, chunk) in image.chunks(width as usize).enumerate() {
            let row_data = self.encode_row(chunk);
            let mut packet_data = Vec::new();
            packet_data.extend_from_slice(&(row as u16).to_le_bytes());
            packet_data.push(0); // Placeholder for black pixel count
            packet_data.push(0);
            packet_data.push(1); // Repeat count
            packet_data.extend_from_slice(&row_data);

            self.send_packet(&NiimbotPacket::new(
                RequestCommandId::PrintBitmapRow,
                packet_data,
                vec![],
            ))?;
        }

        // End page
        self.send_packet(&NiimbotPacket::new(
            RequestCommandId::PageEnd,
            vec![1],
            vec![ResponseCommandId::In_PageEnd],
        ))?;

        // End print job
        self.send_packet(&NiimbotPacket::new(
            RequestCommandId::PrintEnd,
            vec![1],
            vec![ResponseCommandId::In_PrintEnd],
        ))?;

        Ok(())
    }

    fn encode_row(&self, row: &[u32]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for chunk in row.chunks(8) {
            let mut byte = 0u8;
            for (i, &pixel) in chunk.iter().enumerate() {
                if pixel != 0xFFFFFFFF {
                    // If not white
                    byte |= 1 << (7 - i);
                }
            }
            encoded.push(byte);
        }
        encoded
    }
}
