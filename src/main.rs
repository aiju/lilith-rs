#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

mod vga_buffer;
mod serial;
#[cfg(test)]
mod test;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! { 
    println!("{info}");
    loop {}
}

#[allow(dead_code)]
fn os_main() {
    println!("╔═══════════╗\n║ LILITH OS ║\n╚═══════════╝\nBooting...\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    #[cfg(test)]
    test_main();
    #[cfg(not(test))]
    os_main();
    loop {}
}