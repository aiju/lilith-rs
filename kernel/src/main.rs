#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(abi_x86_interrupt)]

mod mach;
mod interrupts;
mod memory;
mod serial;
#[cfg(test)]
mod test;
mod vga_buffer;
mod debug_info;
mod ramfs;
mod user;

extern crate alloc;

#[cfg(not(test))]
use core::panic::PanicInfo;

use alloc::boxed::Box;
use bootloader::{BootInfo, entry_point};

use crate::{memory::memory_manager, ramfs::ram_fs, user::{Proc, go_to_userspace}};

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{info}");
    loop {}
}

#[allow(dead_code)]
fn os_main() {
    println!("╔═══════════╗\n║ LILITH OS ║\n╚═══════════╝\nBooting...");
    println!("{} MB free", memory_manager().lock().free_bytes() / 1048576);

    let root_proc = Box::leak(Box::new(Proc::new().unwrap()));
    let data = ram_fs().get("cat").unwrap();
    root_proc.load_elf(data);
    unsafe { go_to_userspace(root_proc) };
}

entry_point!(main);
fn main(boot_info: &'static BootInfo) -> ! {
    mach::init();
    interrupts::init();
    memory::init(boot_info);
    ramfs::init();

    #[cfg(test)]
    test_main();
    #[cfg(not(test))]
    os_main();
    loop {}
}
