use core::fmt;
use lazy_static::lazy_static;
use volatile::Volatile;
use x86_64::PhysAddr;

use crate::{memory::phys_to_mut, sync::IrqLock};

/// Unicode codepoints in ascending order (for binary search lookup).
const UNICODE_TO_CP437_KEYS: [u16; 128] = [
    0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A5, 0x00AA, 0x00AB, 0x00AC, 0x00B0, 0x00B1, 0x00B2, 0x00B5,
    0x00B7, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BF, 0x00C4, 0x00C5, 0x00C6, 0x00C7, 0x00C9, 0x00D1,
    0x00D6, 0x00DC, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x00E4, 0x00E5, 0x00E6, 0x00E7, 0x00E8, 0x00E9,
    0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F6, 0x00F7,
    0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x00FF, 0x0192, 0x0393, 0x0398, 0x03A3, 0x03A6, 0x03A9, 0x03B1,
    0x03B4, 0x03B5, 0x03C0, 0x03C3, 0x03C4, 0x03C6, 0x207F, 0x20A7, 0x2219, 0x221A, 0x221E, 0x2229,
    0x2248, 0x2261, 0x2264, 0x2265, 0x2310, 0x2320, 0x2321, 0x2500, 0x2502, 0x250C, 0x2510, 0x2514,
    0x2518, 0x251C, 0x2524, 0x252C, 0x2534, 0x253C, 0x2550, 0x2551, 0x2552, 0x2553, 0x2554, 0x2555,
    0x2556, 0x2557, 0x2558, 0x2559, 0x255A, 0x255B, 0x255C, 0x255D, 0x255E, 0x255F, 0x2560, 0x2561,
    0x2562, 0x2563, 0x2564, 0x2565, 0x2566, 0x2567, 0x2568, 0x2569, 0x256A, 0x256B, 0x256C, 0x2580,
    0x2584, 0x2588, 0x258C, 0x2590, 0x2591, 0x2592, 0x2593, 0x25A0,
];

/// CP437 values corresponding to each entry in UNICODE_TO_CP437_KEYS.
const UNICODE_TO_CP437_VALS: [u8; 128] = [
    0xFF, 0xAD, 0x9B, 0x9C, 0x9D, 0xA6, 0xAE, 0xAA, 0xF8, 0xF1, 0xFD, 0xE6, 0xFA, 0xA7, 0xAF, 0xAC,
    0xAB, 0xA8, 0x8E, 0x8F, 0x92, 0x80, 0x90, 0xA5, 0x99, 0x9A, 0xE1, 0x85, 0xA0, 0x83, 0x84, 0x86,
    0x91, 0x87, 0x8A, 0x82, 0x88, 0x89, 0x8D, 0xA1, 0x8C, 0x8B, 0xA4, 0x95, 0xA2, 0x93, 0x94, 0xF6,
    0x97, 0xA3, 0x96, 0x81, 0x98, 0x9F, 0xE2, 0xE9, 0xE4, 0xE8, 0xEA, 0xE0, 0xEB, 0xEE, 0xE3, 0xE5,
    0xE7, 0xED, 0xFC, 0x9E, 0xF9, 0xFB, 0xEC, 0xEF, 0xF7, 0xF0, 0xF3, 0xF2, 0xA9, 0xF4, 0xF5, 0xC4,
    0xB3, 0xDA, 0xBF, 0xC0, 0xD9, 0xC3, 0xB4, 0xC2, 0xC1, 0xC5, 0xCD, 0xBA, 0xD5, 0xD6, 0xC9, 0xB8,
    0xB7, 0xBB, 0xD4, 0xD3, 0xC8, 0xBE, 0xBD, 0xBC, 0xC6, 0xC7, 0xCC, 0xB5, 0xB6, 0xB9, 0xD1, 0xD2,
    0xCB, 0xCF, 0xD0, 0xCA, 0xD8, 0xD7, 0xCE, 0xDF, 0xDC, 0xDB, 0xDD, 0xDE, 0xB0, 0xB1, 0xB2, 0xFE,
];

/// Look up Unicode codepoint -> CP437 byte. Returns None if not in CP437.
fn unicode_to_cp437(c: char) -> Option<u8> {
    let cp = c as u32;
    if cp < 0x80 {
        return Some(cp as u8);
    }
    let cp = u16::try_from(cp).ok()?;
    UNICODE_TO_CP437_KEYS
        .binary_search(&cp)
        .ok()
        .map(|i| UNICODE_TO_CP437_VALS[i])
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for ch in s.chars() {
            self.write_byte(unicode_to_cp437(ch).unwrap_or(0xfe));
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: IrqLock<Writer> = IrqLock::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { phys_to_mut(PhysAddr::new(0xb8000)) },
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
}
