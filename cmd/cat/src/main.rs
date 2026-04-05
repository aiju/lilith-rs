#![no_std]
#![no_main]

use core::{arch::asm, fmt::Write, panic::PanicInfo};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

struct Writer;

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.bytes() {
            unsafe { asm!("mov rdi, 0", "mov rsi, {c}", "syscall", c = in(reg) c as u64) }
        }
        Ok(())
    }
}

#[unsafe(no_mangle)]
extern "C" fn _start() {
    let id: u64;
    unsafe { asm!("mov rdi, 2", "syscall", out("eax") id) };
    loop {
        let _ = write!(Writer, "hello {id}\n");
        unsafe { asm!("mov rdi, 1", "syscall") }
    }
}
