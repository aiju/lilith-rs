use crate::{gdt, println};
use lazy_static::lazy_static;
use x86_64::{registers::control::Cr2, structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode}};

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, exc_no: u64) -> ! {
    println!("DOUBLE FAULT, EXCEPTION #{}\n{:#?}", exc_no, stack_frame);
    loop {}
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error: PageFaultErrorCode) {
    let addr = Cr2::read();
    println!("Page Fault, address {:?}, error code {:?}", addr, error);
    println!("{:#?}", stack_frame);
    loop {}
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt
    };
}

pub fn init() {
    IDT.load();
}
