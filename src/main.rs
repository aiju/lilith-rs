#![no_std]
#![no_main]

mod vga_buffer;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! { 
    println!("{info}");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    println!("╔═══════════╗\n║ LILITH OS ║\n╚═══════════╝\nBooting...\n");
    loop {}
}