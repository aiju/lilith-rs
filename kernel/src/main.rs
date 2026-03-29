#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(atomic_ptr_null)]
#![allow(dead_code)]

mod id_vec;
mod interrupts;
mod mach;
mod memory;
mod prelude;
mod ramfs;
mod sched;
mod serial;
mod sync;
mod tasks;
#[cfg(test)]
mod test;
mod user;
mod vga_buffer;

extern crate alloc;

#[cfg(not(test))]
use core::panic::PanicInfo;

use alloc::sync::Arc;
use bootloader::{BootInfo, entry_point};

use crate::{
    ramfs::ram_fs, sched::thread_spawn, tasks::{task_sleep, task_spawn}, user::Proc
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

    task_spawn(async {
        loop {
            println!(":)");
            task_sleep(1_000_000_000).await;
        }
    });

    thread_spawn(|| {
        let root_proc = Arc::new(Proc::new().unwrap());
        let active_proc = root_proc.activate();
        let data = ram_fs().get("cat").unwrap();
        let entry = active_proc.load_elf(data);
        unsafe { active_proc.launch(entry) };
    });
}

entry_point!(main);
fn main(boot_info: &'static BootInfo) -> ! {
    unsafe {
        let interrupt_guard = mach::init();
        interrupts::init();
        memory::init(boot_info);
        sched::init();
        tasks::init();
        ramfs::init();
        drop(interrupt_guard);
    }

    #[cfg(test)]
    test_main();
    #[cfg(not(test))]
    os_main();

    unsafe { sched::idle_thread() };
}
