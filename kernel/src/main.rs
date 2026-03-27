#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

mod debug_info;
mod interrupts;
mod mach;
mod memory;
mod ramfs;
mod sched;
mod serial;
mod sync;
#[cfg(test)]
mod test;
mod user;
mod vga_buffer;

extern crate alloc;

#[cfg(not(test))]
use core::panic::PanicInfo;

use alloc::boxed::Box;
use bootloader::{BootInfo, entry_point};

use crate::{
    ramfs::ram_fs,
    user::{Proc, go_to_userspace},
};

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{info}");
    loop {}
}

#[allow(dead_code)]
fn os_main() {
    println!("╔═══════════╗\n║ LILITH OS ║\n╚═══════════╝\nBooting...");

    let root_proc = Box::leak(Box::new(Proc::new().unwrap()));
    let data = ram_fs().get("cat").unwrap();
    root_proc.load_elf(data);
    unsafe { go_to_userspace(root_proc) };
}

entry_point!(main);
fn main(boot_info: &'static BootInfo) -> ! {
    unsafe {
        println!("start");
        mach::init();
        interrupts::init();
        memory::init(boot_info);
        ramfs::init();
    }

    #[cfg(test)]
    test_main();
    #[cfg(not(test))]
    os_main();
    loop {}
}
