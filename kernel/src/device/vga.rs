use x86_64::PhysAddr;

use crate::{
    device,
    memory::phys_to_virt,
    println,
    sync::{BootInit, IrqLock},
};

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct ModeInfoBlock {
    // Mandatory information for all VBE revisions
    pub mode_attributes: u16,
    pub win_a_attributes: u8,
    pub win_b_attributes: u8,
    pub win_granularity: u16,
    pub win_size: u16,
    pub win_a_segment: u16,
    pub win_b_segment: u16,
    pub win_func_ptr: u32,
    pub bytes_per_scan_line: u16,

    // Mandatory information for VBE 1.2 and above
    pub x_resolution: u16,
    pub y_resolution: u16,
    pub x_char_size: u8,
    pub y_char_size: u8,
    pub number_of_planes: u8,
    pub bits_per_pixel: u8,
    pub number_of_banks: u8,
    pub memory_model: u8,
    pub bank_size: u8,
    pub number_of_image_pages: u8,
    pub reserved0: u8,

    // Direct Color fields
    pub red_mask_size: u8,
    pub red_field_position: u8,
    pub green_mask_size: u8,
    pub green_field_position: u8,
    pub blue_mask_size: u8,
    pub blue_field_position: u8,
    pub rsvd_mask_size: u8,
    pub rsvd_field_position: u8,
    pub direct_color_mode_info: u8,

    // Mandatory information for VBE 2.0 and above
    pub phys_base_ptr: u32,
    pub reserved1: u32,
    pub reserved2: u16,

    // Mandatory information for VBE 3.0 and above
    pub lin_bytes_per_scan_line: u16,
    pub bnk_number_of_image_pages: u8,
    pub lin_number_of_image_pages: u8,
    pub lin_red_mask_size: u8,
    pub lin_red_field_position: u8,
    pub lin_green_mask_size: u8,
    pub lin_green_field_position: u8,
    pub lin_blue_mask_size: u8,
    pub lin_blue_field_position: u8,
    pub lin_rsvd_mask_size: u8,
    pub lin_rsvd_field_position: u8,
    pub max_pixel_clock: u32,

    pub reserved3: [u8; 190],
}

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

pub struct VgaRenderer {
    frame_buffer: *mut u32,
    screen_width: u32,
    screen_height: u32,
    stride: usize,
}

unsafe impl Send for VgaRenderer {}

static FONT: &'static [u8] = include_bytes!("font.bin");

static VGA_RENDERER: BootInit<IrqLock<VgaRenderer>> = unsafe { BootInit::uninit() };

pub unsafe fn init() {
    // original location in low memory -- we can only access this during early_init before memory::init executes!
    unsafe extern "C" {
        static vesa_mode_info_block: ModeInfoBlock;
    }
    let mode_info_block: &ModeInfoBlock = unsafe {
        &*phys_to_virt(PhysAddr::new_unsafe(&raw const vesa_mode_info_block as u64)).as_ptr()
    };

    assert_eq!(mode_info_block.red_field_position, 16);
    assert_eq!(mode_info_block.green_field_position, 8);
    assert_eq!(mode_info_block.blue_field_position, 0);
    assert_eq!(mode_info_block.bits_per_pixel, 32);

    assert!(mode_info_block.red_mask_size == 8, "red mask not 8 bits");
    assert!(
        mode_info_block.green_mask_size == 8,
        "green mask not 8 bits"
    );
    assert!(mode_info_block.blue_mask_size == 8, "blue mask not 8 bits");

    let frame_buffer = unsafe {
        phys_to_virt(PhysAddr::new_unsafe(mode_info_block.phys_base_ptr as u64)).as_mut_ptr()
    };
    // TODO: we might want to set up write-combining for the framebuffer with PAT or MTRR

    unsafe {
        BootInit::set(
            &VGA_RENDERER,
            IrqLock::new(VgaRenderer {
                frame_buffer,
                screen_width: mode_info_block.x_resolution as u32,
                screen_height: mode_info_block.y_resolution as u32,
                stride: mode_info_block.bytes_per_scan_line as usize,
            }),
        )
    };
}

impl VgaRenderer {
    pub fn render_char(&mut self, x_pos: u32, y_pos: u32, ch: u8) {
        let f = &FONT[14 * ch as usize..14 * (ch as usize + 1)];
        unsafe {
            for j in 0..16 {
                for i in 0..8 {
                    if x_pos + i < self.screen_width && y_pos + j < self.screen_height {
                        let p = self
                            .frame_buffer
                            .byte_add(self.stride * (y_pos + j) as usize)
                            .add((i + x_pos) as usize);
                        let box_drawing = ch >= 0xb0 && ch <= 0xdf;
                        let bit = f[j.min(13) as usize] >> 7 - i & 1 != 0;
                        if (j < 14 || box_drawing) && bit {
                            p.write_volatile(0xffffff);
                        } else {
                            p.write_volatile(0);
                        }
                    }
                }
            }
        }
    }
    pub fn scroll(&mut self, lines: u32) {
        unsafe {
            core::ptr::copy(
                self.frame_buffer.byte_add(lines as usize * self.stride),
                self.frame_buffer,
                (self.screen_height as usize - lines as usize) * self.stride,
            );
        }
    }
}

pub struct Writer {
    x_pos: u32,
    y_pos: u32,
}

pub static WRITER: IrqLock<Writer> = IrqLock::new(Writer { x_pos: 0, y_pos: 0 });

impl Writer {
    fn write_byte(&mut self, b: u8) {
        let mut vga_renderer = VGA_RENDERER.lock();
        if b == b'\n' {
            self.y_pos += 16;
            self.x_pos = 0;
        } else {
            vga_renderer.render_char(self.x_pos, self.y_pos, b);
            self.x_pos += 8;
            if self.x_pos >= vga_renderer.screen_width {
                self.x_pos = 0;
                self.y_pos += 16;
            }
        }
        if self.y_pos >= vga_renderer.screen_height {
            vga_renderer.scroll(16);
            self.y_pos -= 16;
        }
    }
    fn write_string(&mut self, str: &str) {
        for ch in str.chars() {
            self.write_byte(unicode_to_cp437(ch).unwrap_or(0xfe));
        }
    }
}

impl device::Writer for Writer {
    fn write(&mut self, data: &[u8]) {
        for chunk in data.utf8_chunks() {
            self.write_string(chunk.valid());
            if !chunk.invalid().is_empty() {
                self.write_byte(0xfe);
            }
        }
    }
}
