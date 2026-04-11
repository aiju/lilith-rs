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
mod draw;

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

    let screen_rect = draw::FRAME_BUFFER.lock().rect();
    let mut screen = draw::Screen::new(screen_rect);
    let window_a = screen.new_window(draw::Rect::new(100, 100, 200, 200), 0);
    let window_b = screen.new_window(draw::Rect::new(150, 150, 250, 250), 0);
    screen.window_mut(window_a).surface_mut().fill(screen_rect, draw::Color::RED);
    screen.window_mut(window_b).surface_mut().fill(screen_rect, draw::Color::BLUE);
    screen.update(&mut *draw::FRAME_BUFFER.lock());
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
