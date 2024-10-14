#![allow(dead_code)]

use color_eyre::Result;
use rusb::{DeviceHandle, GlobalContext};
use serialport::SerialPort;
use std::time::Duration;

pub trait NiimbotPrinterAdapter {
    fn send(&mut self, bytes: &[u8]) -> Result<usize>;
    fn recv(&mut self, bytes: &mut [u8]) -> Result<usize>;
}

pub struct SerialPortAdapter {
    pub serial_port: Box<dyn SerialPort>,
}

impl SerialPortAdapter {
    pub fn new(serial_port: &str) -> Result<Self> {
        Ok(Self {
            serial_port: serialport::new(serial_port, 115200).open()?,
        })
    }
}

impl NiimbotPrinterAdapter for SerialPortAdapter {
    fn send(&mut self, bytes: &[u8]) -> Result<usize> {
        self.serial_port.write_all(bytes)?;
        std::thread::sleep(Duration::from_millis(2));
        Ok(0)
    }

    fn recv(&mut self, bytes: &mut [u8]) -> Result<usize> {
        Ok(self.serial_port.read(bytes)?)
    }
}

pub struct UsbAdapter {
    pub device_handle: DeviceHandle<GlobalContext>,
}

impl UsbAdapter {
    pub fn new(device_handle: DeviceHandle<GlobalContext>) -> Result<Self, rusb::Error> {
        Ok(Self { device_handle })
    }
}

impl NiimbotPrinterAdapter for UsbAdapter {
    fn send(&mut self, bytes: &[u8]) -> Result<usize> {
        Ok(self
            .device_handle
            .write_bulk(0x01, bytes, Duration::from_secs(1))?)
    }

    fn recv(&mut self, bytes: &mut [u8]) -> Result<usize> {
        Ok(self
            .device_handle
            .read_bulk(0x81, bytes, Duration::from_secs(1))?)
    }
}
