#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(atomic_ptr_null)]

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

use alloc::sync::Arc;
use bootloader::{BootInfo, entry_point};

use crate::{ramfs::ram_fs, user::Proc};

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{info}");
    loop {}
}

#[allow(dead_code)]
fn os_main() {
    println!("╔═══════════╗\n║ LILITH OS ║\n╚═══════════╝\nBooting...");

    let root_proc = Arc::new(Proc::new().unwrap());
    let active_proc = root_proc.activate();
    let data = ram_fs().get("cat").unwrap();
    let entry = active_proc.load_elf(data);
    unsafe { active_proc.go_to_userspace(entry) };
}

entry_point!(main);
fn main(boot_info: &'static BootInfo) -> ! {
    unsafe {
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
