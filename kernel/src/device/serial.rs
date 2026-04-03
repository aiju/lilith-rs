use crate::{
    device::Writer,
    sync::{BootInit, IrqLock},
};
use uart_16550::SerialPort;

pub static SERIAL1: BootInit<IrqLock<SerialPort>> = unsafe { BootInit::uninit() };

pub unsafe fn init() {
    unsafe {
        BootInit::set(&SERIAL1, {
            let mut serial_port = SerialPort::new(0x3F8);
            serial_port.init();
            IrqLock::new(serial_port)
        });
    }
}

impl Writer for SerialPort {
    fn write(&mut self, data: &[u8]) {
        for &byte in data {
            self.send(byte);
        }
    }
}
