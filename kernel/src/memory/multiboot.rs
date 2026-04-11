use core::marker::PhantomData;

use alloc::{string::String, vec::Vec};
use x86_64::PhysAddr;

use crate::{memory::phys_to_virt, sync::BootInit};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct MultibootAoutSyms {
    tabsize: u32,
    strsize: u32,
    addr: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct MultibootElfSections {
    num: u32,
    size: u32,
    addr: u32,
    shndx: u32,
}

#[repr(C)]
pub union MultibootSyms {
    raw: [u32; 4],
    aout: MultibootAoutSyms,
    elf: MultibootElfSections,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct MultibootColorIndexed {
    palette_addr: u32,
    palette_num_colors: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct MultibootColorRgb {
    red_field_position: u8,
    red_mask_size: u8,
    green_field_position: u8,
    green_mask_size: u8,
    blue_field_position: u8,
    blue_mask_size: u8,
}

#[repr(C)]
pub union MultibootColorInfo {
    raw: [u32; 6],
    indexed: MultibootColorIndexed,
    rgb: MultibootColorRgb,
}

#[repr(C)]
pub struct MultibootInfoRaw {
    pub flags: u32,                     // 0
    pub mem_lower: u32,                 // 4
    pub mem_upper: u32,                 // 8
    pub boot_device: u32,               // 12
    pub cmdline: u32,                   // 16
    pub mods_count: u32,                // 20
    pub mods_addr: u32,                 // 24
    pub syms: MultibootSyms,            // 28-40
    pub mmap_length: u32,               // 44
    pub mmap_addr: u32,                 // 48
    pub drives_length: u32,             // 52
    pub drives_addr: u32,               // 56
    pub config_table: u32,              // 60
    pub boot_loader_name: u32,          // 64
    pub apm_table: u32,                 // 68
    pub vbe_control_info: u32,          // 72
    pub vbe_mode_info: u32,             // 76
    pub vbe_mode: u16,                  // 80
    pub vbe_interface_seg: u16,         // 82
    pub vbe_interface_off: u16,         // 84
    pub vbe_interface_len: u16,         // 86
    pub framebuffer_addr: u64,          // 88
    pub framebuffer_pitch: u32,         // 96
    pub framebuffer_width: u32,         // 100
    pub framebuffer_height: u32,        // 104
    pub framebuffer_bpp: u8,            // 108
    pub framebuffer_type: u8,           // 109
    pub color_info: MultibootColorInfo, // 110-115
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MultibootModuleRaw {
    pub mod_start: u32,
    pub mod_end: u32,
    pub name: u32,
    reserved: u32,
}

pub struct MultibootModule {
    pub name: String,
    pub contents: &'static [u8],
}

pub static MULTIBOOT_MODULES: BootInit<Vec<MultibootModule>> = unsafe { BootInit::uninit() };
pub static MULTIBOOT_CMDLINE: BootInit<&'static str> = unsafe { BootInit::uninit() };

impl core::fmt::Debug for MultibootInfoRaw {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MultibootInfoRaw")
            .field("flags", &self.flags)
            .field("mem_lower", &self.mem_lower)
            .field("mem_upper", &self.mem_upper)
            .field("boot_device", &self.boot_device)
            .field("cmdline", &self.cmdline)
            .field("mods_count", &self.mods_count)
            .field("mods_addr", &self.mods_addr)
            .field("syms", unsafe { &self.syms.raw })
            .field("mmap_length", &self.mmap_length)
            .field("mmap_addr", &self.mmap_addr)
            .field("drives_length", &self.drives_length)
            .field("drives_addr", &self.drives_addr)
            .field("config_table", &self.config_table)
            .field("boot_loader_name", &self.boot_loader_name)
            .field("apm_table", &self.apm_table)
            .field("vbe_control_info", &self.vbe_control_info)
            .field("vbe_mode_info", &self.vbe_mode_info)
            .field("vbe_mode", &self.vbe_mode)
            .field("vbe_interface_seg", &self.vbe_interface_seg)
            .field("vbe_interface_off", &self.vbe_interface_off)
            .field("vbe_interface_len", &self.vbe_interface_len)
            .field("framebuffer_addr", &self.framebuffer_addr)
            .field("framebuffer_pitch", &self.framebuffer_pitch)
            .field("framebuffer_width", &self.framebuffer_width)
            .field("framebuffer_height", &self.framebuffer_height)
            .field("framebuffer_bpp", &self.framebuffer_bpp)
            .field("framebuffer_type", &self.framebuffer_type)
            .field("color_info", unsafe { &self.color_info.raw })
            .finish()
    }
}

pub enum MultibootMmapType {
    Reserved,
    Available,
    Acpi,
    Hibernation,
    BadRam,
}

#[repr(C, packed(1))]
#[derive(Debug)]
pub struct MultibootMmapEntry {
    entry_size: u32,
    pub addr: u64,
    pub len: u64,
    pub mem_type: u32,
}

impl MultibootMmapEntry {
    pub fn is_usable(&self) -> bool {
        match self.mem_type {
            1 => true,
            _ => false,
        }
    }
}

pub struct MultibootMmapIter<'a> {
    ptr: *const u8,
    end: *const u8,
    _phantom: PhantomData<&'a u8>,
}

impl MultibootInfoRaw {
    pub fn mmaps(&self) -> MultibootMmapIter<'_> {
        assert!(
            self.flags & 1 << 6 != 0,
            "no mmap data passed by bootloader"
        );
        MultibootMmapIter {
            ptr: phys_to_virt(unsafe { PhysAddr::new_unsafe(self.mmap_addr.into()) }).as_ptr(),
            end: phys_to_virt(unsafe {
                PhysAddr::new_unsafe(self.mmap_addr as u64 + self.mmap_length as u64)
            })
            .as_ptr(),
            _phantom: Default::default(),
        }
    }

    pub fn modules(&self) -> &[MultibootModuleRaw] {
        unsafe {
            core::slice::from_raw_parts(
                phys_to_virt(PhysAddr::new_unsafe(self.mods_addr as u64)).as_mut_ptr(),
                self.mods_count as usize,
            )
        }
    }
}

impl<'a> Iterator for MultibootMmapIter<'a> {
    type Item = &'a MultibootMmapEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        } else {
            unsafe {
                assert!(self.ptr.add(core::mem::size_of::<MultibootMmapEntry>()) <= self.end);
                let r = &*(self.ptr as *const MultibootMmapEntry);
                self.ptr = self.ptr.add(4 + r.entry_size as usize);
                Some(r)
            }
        }
    }
}
