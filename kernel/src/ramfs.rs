use core::slice;

use crate::{memory::MULTIBOOT_MODULES, sync::BootInit};

#[repr(C)]
#[derive(Debug)]
struct FsHeader {
    magic: u64,
    n_files: u64,
}

#[repr(C)]
#[derive(Debug)]
struct FsEntry {
    name: [u8; 32],
    offset: u64,
    size: u64,
    reserved: [u8; 16],
}

pub struct RamFs {
    data: &'static [u8],
}

impl RamFs {
    fn header(&self) -> &FsHeader {
        unsafe { &*(self.data.as_ptr() as *const FsHeader) }
    }
    fn entries(&self) -> &[FsEntry] {
        unsafe {
            slice::from_raw_parts(
                &*(self.data.as_ptr().byte_add(16) as *const FsEntry),
                self.header().n_files as usize,
            )
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.entries().iter().map(|entry| {
            let len = entry.name.iter().position(|c| *c == 0).unwrap_or(32);
            str::from_utf8(&entry.name[0..len]).unwrap()
        })
    }
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let index = self.iter().position(|n| n == name)?;
        let entry = &self.entries()[index];
        Some(&self.data[entry.offset as usize..(entry.offset + entry.size) as usize])
    }
}

static RAM_FS: BootInit<RamFs> = unsafe { BootInit::uninit() };

pub fn ram_fs() -> &'static RamFs {
    &RAM_FS
}

pub fn init() {
    assert_eq!(MULTIBOOT_MODULES.len(), 1);
    unsafe {
        BootInit::set(
            &RAM_FS,
            RamFs {
                data: MULTIBOOT_MODULES[0].contents,
            },
        )
    };
}
