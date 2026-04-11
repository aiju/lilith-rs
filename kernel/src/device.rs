use crate::device::serial::SERIAL1;

pub trait Writer {
    fn write(&mut self, data: &[u8]);
}

pub mod serial;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::device::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

struct FmtWriter<T>(T);

impl<T: Writer + ?Sized> core::fmt::Write for FmtWriter<&mut T> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;

    let _ = FmtWriter(&mut *crate::draw::WRITER.lock()).write_fmt(args);
    let _ = FmtWriter(&mut *SERIAL1.lock()).write_fmt(args);
}

pub unsafe fn early_init() {
    unsafe {
        serial::init();
        crate::draw::init();
    }
}
