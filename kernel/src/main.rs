#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(abi_x86_interrupt)]

mod gdt;
mod interrupts;
mod memory;
mod serial;
#[cfg(test)]
mod test;
mod vga_buffer;
mod debug_info;

extern crate alloc;

#[cfg(not(test))]
use core::panic::PanicInfo;

use bootloader::{BootInfo, entry_point};

use crate::memory::memory_manager;

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
}

entry_point!(main);
fn main(boot_info: &'static BootInfo) -> ! {
    gdt::init();
    interrupts::init();
    memory::init(boot_info);

    #[cfg(test)]
    test_main();
    #[cfg(not(test))]
    os_main();
    loop {}
}
