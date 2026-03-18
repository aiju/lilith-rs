use core::panic::PanicInfo;
use crate::{serial_print, serial_println};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

fn exit_qemu(exit_code: QemuExitCode) -> ! {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n{}", info);
    exit_qemu(QemuExitCode::Failed);
}

pub trait Testable {
    fn run(&self) -> ();
}

impl<T> Testable for T
where T: Fn()
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]\n");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Lilith Tests\nRunning {} tests...\n", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}
