#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(atomic_ptr_null)]
#![feature(isolate_most_least_significant_one)]
#![allow(dead_code)]

mod device;
mod id_vec;
mod interrupts;
mod mach;
mod memory;
mod prelude;
mod ramfs;
mod sched;
mod sync;
mod tasks;
#[cfg(test)]
mod test;
mod user;
mod util;

extern crate alloc;

#[cfg(not(test))]
use core::panic::PanicInfo;

use alloc::sync::Arc;

use crate::{
    ramfs::ram_fs,
    sched::thread_spawn,
    tasks::{task_sleep, task_spawn},
    user::Proc,
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

    thread_spawn(|| {
        let root_proc = Arc::new(Proc::new().unwrap());
        let active_proc = root_proc.activate();
        let data = ram_fs().get("cat").unwrap();
        let entry = active_proc.load_elf(data);
        unsafe { active_proc.launch(entry) };
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn early_init(multiboot_info: x86_64::PhysAddr) -> memory::Stack {
    // this runs on the boot stack
    // you can't use memory allocation until memory::init
    // boot memory remains allocated until we drop _reclaimer at the end of this function

    unsafe {
        let interrupt_guard = mach::init();
        device::early_init();
        interrupts::init();
        let (_reclaimer, kernel_stack) = memory::init(multiboot_info);
        interrupt_guard.drop_without_disabling(); // leave interrupts off for now
        kernel_stack
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(kernel_stack: memory::Stack) -> ! {
    // this runs on kernel_stack, ownership of which we pass to the scheduler

    let interrupt_guard = sync::interrupt_guard();

    unsafe {
        sched::init(kernel_stack);
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
