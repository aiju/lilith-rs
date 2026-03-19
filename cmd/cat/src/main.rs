#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
extern "C" fn _start() {
    let s = "hello, world";
    for c in s.as_bytes() {
        unsafe { asm!("mov rdi, {c}", "syscall", c = in(reg) *c as u64) }
    }
    loop {}
}
