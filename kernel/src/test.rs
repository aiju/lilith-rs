use alloc::vec::Vec;

use crate::{memory::MULTIBOOT_CMDLINE, prelude::*};

use core::panic::PanicInfo;

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
    println!("[failed]\n{}", info);
    exit_qemu(QemuExitCode::Failed);
}

pub trait Testable {
    fn run(&self) -> ();
    fn name(&self) -> &'static str;
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
    fn run(&self) {
        print!("{}...\t", self.name());
        self();
        println!("[ok]\n");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Lilith Tests");
    let cmdline = *MULTIBOOT_CMDLINE;
    let arguments: Vec<&'static str> = cmdline.split(' ').skip(1).collect();
    let selected_tests: Vec<&dyn Testable> = if arguments.is_empty() {
        tests.iter().copied().collect()
    } else {
        tests
            .iter()
            .copied()
            .filter(|t| arguments.iter().any(|n| t.name().contains(n)))
            .collect()
    };
    println!("Running {} tests...\n", selected_tests.len());
    for test in selected_tests.iter() {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}
