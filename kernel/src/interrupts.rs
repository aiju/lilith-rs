use core::arch::naked_asm;

use crate::{
    mach::{
        KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, Mach, USER_CODE_SELECTOR, USER_DATA_SELECTOR, mach
    }, print, println
};
use pic8259::ChainedPics;
use x86_64::{
    VirtAddr,
    registers::{
        control::{Cr2, Efer, EferFlags},
        model_specific::{LStar, SFMask, Star}, rflags::RFlags,
    },
    structures::{
        idt::{InterruptStackFrame, PageFaultErrorCode},
        tss::TaskStateSegment,
    },
};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[repr(align(16))]
#[allow(dead_code)]
struct AlignedStack([u8; 4096]);

static mut DOUBLE_FAULT_STACK: AlignedStack = AlignedStack([0; 4096]);
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, _zero: u64) -> ! {
    println!("DOUBLE FAULT\n{:#?}", stack_frame);
    loop {}
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("BREAKPOINT\n{:#?}", stack_frame);
    loop {}
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error: PageFaultErrorCode,
) {
    let addr = Cr2::read();
    println!("Page Fault, address {:?}, error code {:?}", addr, error);
    println!("{:#?}", stack_frame);
    loop {}
}

fn set_ist_stack<T>(tss: &mut TaskStateSegment, index: u16, stack: *mut T) {
    tss.interrupt_stack_table[index as usize] =
        VirtAddr::from_ptr(stack) + core::mem::size_of::<T>();
}

extern "C" fn syscall_handler(arg: u64) {
    print!("{}", arg as u8 as char);
}

#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        "swapgs",
        "mov gs:8, rsp",
        "mov rsp, gs:0",
        "mov gs:16, rcx",
        "mov gs:24, r11",
        "call {syscall_handler}",
        "mov rsp, gs:8",
        "mov rcx, gs:16",
        "mov r11, gs:24",
        "swapgs",
        "sysretq",
        syscall_handler = sym syscall_handler
    )
}

pub fn init() {
    let mut guard = mach().lock();
    let Mach {
        ref mut idt,
        ref mut tss,
        ..
    } = *guard;

    set_ist_stack(tss, DOUBLE_FAULT_IST_INDEX, &raw mut DOUBLE_FAULT_STACK);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(DOUBLE_FAULT_IST_INDEX);
    }
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.breakpoint
        .set_handler_fn(breakpoint_handler)
        .set_privilege_level(x86_64::PrivilegeLevel::Ring3);

    unsafe { PICS.lock().initialize() };
    unsafe { PICS.lock().disable() };

    Star::write(
        USER_CODE_SELECTOR,
        USER_DATA_SELECTOR,
        KERNEL_CODE_SELECTOR,
        KERNEL_DATA_SELECTOR,
    )
    .unwrap();
    LStar::write(VirtAddr::from_ptr(syscall_entry as *const u8));
    SFMask::write(RFlags::INTERRUPT_FLAG);
    unsafe { Efer::update(|f| *f |= EferFlags::SYSTEM_CALL_EXTENSIONS) };
}
