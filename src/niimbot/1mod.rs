use anyhow::Result;
use rusb::{DeviceHandle, GlobalContext, UsbContext};
use std::time::Duration;

// ISSUE: i forgor

pub enum InfoEnum {
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

pub enum RequestCodeEnum {
    GetInfo = 64,
    GetRfid = 26,
    Heartbeat = 220,
    SetLabelType = 35,
    SetLabelDensity = 33,
    StartPrint = 1,
    EndPrint = 243,
    StartPagePrint = 3,
    EndPagePrint = 227,
    AllowPrintClear = 32,
    SetDimension = 19,
    SetQuantity = 21,
    GetPrintStatus = 163,
}

pub struct UsbTransport {
    handle: DeviceHandle<GlobalContext>,
}

impl UsbTransport {
    pub fn new(handle: DeviceHandle<GlobalContext>) -> Self {
        UsbTransport { handle }
    }

    pub fn read(&self, buf: &mut [u8], timeout: Duration) -> rusb::Result<usize> {
        self.handle.read_bulk(0x81, buf, timeout)
    }

    pub fn write(&self, data: &[u8], timeout: Duration) -> rusb::Result<usize> {
        self.handle.write_bulk(0x01, data, timeout)
    }
}

#[derive(Debug)]
pub struct NiimbotPacket {
    pub packet_type: u8,
    pub data: Vec<u8>,
}

impl NiimbotPacket {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 7 || &bytes[..2] != b"\x55\x55" || &bytes[bytes.len() - 2..] != b"\xaa\xaa"
        {
            dbg!("Not a Niimbot Packet");
            return None;
        }

        let packet_type = bytes[2];
        let length = bytes[3] as usize;
        if bytes.len() < 4 + length + 3 {
            dbg!("Not a Niimbot Packet 2");
            return None;
        }

        let data = bytes[4..4 + length].to_vec();
        let checksum = bytes
            .iter()
            .skip(2)
            .take(2 + length)
            .fold(packet_type ^ (length as u8), |acc, &x| acc ^ x);

        if checksum != bytes[4 + length] {
            dbg!("Checksum Mismatch");
            return None;
        }

        Some(NiimbotPacket { packet_type, data })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut checksum = self.packet_type ^ (self.data.len() as u8);
        for byte in &self.data {
            checksum ^= byte;
        }
        let mut bytes = vec![0x55, 0x55, self.packet_type, self.data.len() as u8];
        bytes.extend_from_slice(&self.data);
        bytes.push(checksum);
        bytes.extend_from_slice(&[0xAA, 0xAA]);
        bytes
    }
}

pub struct PrinterClient {
    transport: UsbTransport,
}

impl PrinterClient {
    pub fn new(transport: UsbTransport) -> Self {
        PrinterClient { transport }
    }

    // Example method
    pub fn start_print(&self) -> bool {
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::StartPrint as u8,
            data: vec![1],
        };
        let ok = self.send_packet(&packet);
        dbg!("Start Print: {}", ok);
        ok
    }

    pub fn send_packet(&self, packet: &NiimbotPacket) -> bool {
        let bytes = packet.to_bytes();
        self.transport
            .write(&bytes, Duration::from_secs(1))
            .unwrap();
        true
    }

    // def _recv(self):
    //     packets = []
    //     self._packetbuf.extend(self._transport.read(1024))
    //     while len(self._packetbuf) > 4:
    //         pkt_len = self._packetbuf[3] + 7
    //         if len(self._packetbuf) >= pkt_len:
    //             packet = NiimbotPacket.from_bytes(self._packetbuf[:pkt_len])
    //             self._log_buffer("recv", packet.to_bytes())
    //             packets.append(packet)
    //             del self._packetbuf[:pkt_len]
    //     return packets

    pub fn _recv(&self) -> Result<Vec<NiimbotPacket>> {
        let mut packets = Vec::new();
        let mut packetbuf = [0; 1024];
        self.transport
            .read(&mut packetbuf, Duration::from_secs(1))?;
        let mut packetbuf = packetbuf.to_vec();
        dbg!(&packetbuf.iter().rev().collect::<Vec<&u8>>());
        while packetbuf.len() > 4 {
            let pkt_len = (packetbuf[3] as usize) + 7;
            dbg!(packetbuf.len());
            if packetbuf.len() >= pkt_len {
                let packet = NiimbotPacket::from_bytes(&packetbuf[..pkt_len]);
                dbg!(&packet);
                if let Some(packet) = packet {
                    dbg!("recv", packet.to_bytes());
                    packets.push(packet);
                    packetbuf.drain(..pkt_len);
                    dbg!(&packets);
                }
            }
        }
        Ok(packets)
    }

    pub fn print_image(&self, width: usize, height: usize, image: &Vec<u32>, density: u8) {
        dbg!("Setting Density");
        self.set_label_density(density);
        self.set_label_type(1);
        self.start_print();
        self.start_page_print();
        self.set_dimension(width as u16, height as u16);
        dbg!("Encoding Image!(");
        for packet in self.encode_image(height, image) {
            self.send_packet(&packet);
        }

        dbg!("Ending Print");
        self.end_page_print();
        dbg!("Ending Thing!");
        std::thread::sleep(Duration::from_millis(300));
        while !self.end_print() {
            dbg!("Looping!");
            std::thread::sleep(Duration::from_millis(100));
        }
        dbg!("Done!");
    }

    fn encode_image(&self, height: usize, image: &Vec<u32>) -> Vec<NiimbotPacket> {
        let mut packets = Vec::new();

        for (y, row) in image.chunks_exact(height).enumerate() {
            let line_data: Vec<u8> = row
                .iter()
                .map(|pixel| if (pixel & 0xFF) > 128 { 0b1 } else { 0b0 })
                .collect();
            let line_data_bytes = pack_bits_to_bytes(&line_data);
            let counts = (0, 0, 0);
            let header = [(y >> 8) as u8, y as u8, counts.0, counts.1, counts.2, 1];
            let mut packet_data = Vec::new();
            packet_data.extend_from_slice(&header);
            packet_data.extend_from_slice(&line_data_bytes);
            packets.push(NiimbotPacket {
                packet_type: 0x85,
                data: packet_data,
            });
        }
        packets
    }

    fn set_label_type(&self, n: u8) -> bool {
        if n < 1 || n > 3 {
            return false;
        }
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::SetLabelType as u8,
            data: vec![n],
        };
        self.send_packet(&packet)
    }

    fn set_label_density(&self, n: u8) -> bool {
        if n < 1 || n > 5 {
            return false;
        }
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::SetLabelDensity as u8,
            data: vec![n],
        };
        self.send_packet(&packet)
    }

    fn end_print(&self) -> bool {
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::EndPrint as u8,
            data: vec![1],
        };
        self.send_packet(&packet)
    }

    fn start_page_print(&self) -> bool {
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::StartPagePrint as u8,
            data: vec![1],
        };
        self.send_packet(&packet)
    }

    fn end_page_print(&self) -> bool {
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::EndPagePrint as u8,
            data: vec![1],
        };
        self.send_packet(&packet)
    }

    fn allow_print_clear(&self) -> bool {
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::AllowPrintClear as u8,
            data: vec![1],
        };
        self.send_packet(&packet)
    }

    fn set_dimension(&self, width: u16, height: u16) -> bool {
        let data = vec![
            (width >> 8) as u8,
            width as u8,
            (height >> 8) as u8,
            height as u8,
        ];
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::SetDimension as u8,
            data,
        };
        self.send_packet(&packet)
    }

    fn set_quantity(&self, n: u16) -> bool {
        let data = vec![(n >> 8) as u8, n as u8];
        let packet = NiimbotPacket {
            packet_type: RequestCodeEnum::SetQuantity as u8,
            data,
        };
        self.send_packet(&packet)
    }
}
fn pack_bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    let len = (bits.len() + 7) / 8;
    let mut bytes = vec![0u8; len];
    for (i, &bit) in bits.iter().enumerate() {
        bytes[i / 8] |= bit << (7 - (i % 8));
    }
    bytes
}
