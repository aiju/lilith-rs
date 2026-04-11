use x86_64::PhysAddr;

use crate::{
    draw::{
        geometry::{Color, Rect},
        surface::Surface,
    },
    memory::phys_to_virt,
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

pub static FRAME_BUFFER: BootInit<IrqLock<Surface>> = unsafe { BootInit::uninit() };

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
    assert!(
        mode_info_block.bytes_per_scan_line & 3 == 0,
        "scan lines not dword aligned"
    );

    let frame_buffer = unsafe {
        phys_to_virt(PhysAddr::new_unsafe(mode_info_block.phys_base_ptr as u64)).as_mut_ptr()
    };
    // TODO: we might want to set up write-combining for the framebuffer with PAT or MTRR

    unsafe {
        BootInit::set(
            &FRAME_BUFFER,
            IrqLock::new(Surface::from_raw(
                frame_buffer,
                Rect::new(
                    0,
                    0,
                    mode_info_block.x_resolution as i32,
                    mode_info_block.y_resolution as i32,
                ),
                mode_info_block.bytes_per_scan_line as usize / 4,
                Color::BLACK,
            )),
        )
    };
}
