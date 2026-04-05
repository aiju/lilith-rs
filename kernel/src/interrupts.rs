#[allow(unused_imports)]
use crate::prelude::*;
use core::{arch::naked_asm, sync::atomic::Ordering};

use crate::{
    interrupts::tables::Interrupt,
    mach::{
        KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, Mach, USER_CODE_SELECTOR,
        USER_DATA_SELECTOR, mach,
    },
    memory::page_fault_handler,
    sched::{SCHEDULER, SchedReason, sched, thread_sleep, timer_interrupt},
    sync::{InterruptGuard, IrqLock, IrqLockGuard, interrupt_guard},
};
use pic8259::ChainedPics;
use x86_64::{
    VirtAddr,
    instructions::port::Port,
    registers::{
        control::{Cr2, Efer, EferFlags},
        model_specific::{LStar, SFMask, Star},
        rflags::RFlags,
    },
    structures::{idt::InterruptDescriptorTable, tss::TaskStateSegment},
};

mod tables;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: IrqLock<ChainedPics> =
    IrqLock::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[repr(align(16))]
#[allow(dead_code)]
struct AlignedStack([u8; 4096]);

static mut DOUBLE_FAULT_STACK: AlignedStack = AlignedStack([0; 4096]);
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

#[derive(Debug, Default)]
#[allow(dead_code)]
#[repr(C)]
pub struct TrapFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub int_num: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

const SYSCALL_SENTINEL_INT_NUM: u64 = 256;

#[unsafe(naked)]
extern "C" fn int_common_entry() {
    naked_asm!(
        "test dword ptr [rsp + 24], 3", // check if we came from kernel mode
        "jz 2f",
        "swapgs",
        "2:",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        "mov rdi, rsp",
        "call {int_handler}",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "test dword ptr [rsp + 24], 3", // check if we're going to kernel mode
        "jz 2f",
        "swapgs",
        "2:",
        "add rsp, 16", // skip error code and interrupt number
        "iretq",
        int_handler = sym int_handler
    )
}

pub struct IrqContext<'a> {
    trap_frame: &'a mut TrapFrame,
    interrupt_guard: InterruptGuard,
}

impl IrqContext<'_> {
    pub fn trap_frame(&self) -> &TrapFrame {
        self.trap_frame
    }
    pub unsafe fn trap_frame_mut(&mut self) -> &mut TrapFrame {
        self.trap_frame
    }
    pub fn need_resched(&self) {
        mach().irq_need_resched.store(true, Ordering::Relaxed);
    }
}

extern "C" fn int_handler(trap_frame: &mut TrapFrame) {
    // when we enter here interrupts are always disabled but irq_lock_count may be 0
    // create an interrupt_guard to maintain the irq_lock_count invariant
    let interrupt_guard = interrupt_guard();
    let mut irq_context = IrqContext {
        trap_frame,
        interrupt_guard,
    };
    match Interrupt::try_from(irq_context.trap_frame.int_num as u8).unwrap() {
        Interrupt::PageFault => page_fault_handler(&mut irq_context, Cr2::read()),
        Interrupt::Irq(0) => timer_interrupt(&mut irq_context),
        trap => {
            panic!("Unhandled interrupt: {:#?}\nFrame {:?}", trap, irq_context.trap_frame);
        }
    }
    let mut interrupt_guard = irq_context.interrupt_guard;
    // if we're about to re-enable interrupts, check if we should reschedule
    if interrupt_guard.drop_would_reenable() {
        if mach().irq_need_resched.swap(false, Ordering::Relaxed) {
            // sched() requires irq_lock_count == 1, hence we need to re-use the existing interrupt_guard
            let scheduler_guard = SCHEDULER.lock_with_interrupt_guard(interrupt_guard);
            let scheduler_guard = sched(scheduler_guard, SchedReason::Yielding);
            interrupt_guard = IrqLockGuard::into_interrupt_guard(scheduler_guard);
        }
    }
    // subtract 1 from irq_lock_count, but don't actually re-enable interrupts, let IRETQ do it
    unsafe { interrupt_guard.drop_without_disabling() };
    debug_assert_eq!(
        mach().irq_lock_count.load(Ordering::Relaxed) == 0,
        trap_frame.rflags & 0x200 != 0,
        "mismatch between irq_lock_count and interrupt flag, irq_lock_count={}, IF={}",
        mach().irq_lock_count.load(Ordering::Relaxed),
        trap_frame.rflags >> 9 & 1
    );
}

fn set_ist_stack<T>(tss: &mut TaskStateSegment, index: u16, stack: *mut T) {
    tss.interrupt_stack_table[index as usize] =
        VirtAddr::from_ptr(stack) + core::mem::size_of::<T>();
}

extern "C" fn syscall_handler(trap: &mut TrapFrame) {
    // when we're here irq_lock_count should be 0 and interrupts are disabled
    // restore the irq_lock_count invariant by re-enabling interrupts
    debug_assert_eq!(mach().irq_lock_count.load(Ordering::Relaxed), 0);
    x86_64::instructions::interrupts::enable();

    match trap.rdi {
        0 => print!("{}", trap.rsi as u8 as char),
        1 => thread_sleep(100_000_000),
        2 => trap.rax = usize::from(mach().current_thread_id()) as u64,
        _ => {}
    }
}

#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        "swapgs",
        "mov gs:{user_rsp_offset}, rsp",
        "mov rsp, gs:{kernel_rsp_offset}",
        "push {ss}",
        "push gs:{user_rsp_offset}",
        "push r11",
        "push {cs}",
        "push rcx",
        "push 0",
        "push {SYSCALL_SENTINEL}",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        "mov rdi, rsp",
        "call {syscall_handler}",
        "cli",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "mov rsp, [rsp+8*5]",
        "swapgs",
        "sysretq",
        syscall_handler = sym syscall_handler,
        kernel_rsp_offset = const core::mem::offset_of!(Mach, descriptors.tss) + core::mem::offset_of!(TaskStateSegment, privilege_stack_table),
        user_rsp_offset = const core::mem::offset_of!(Mach, syscall_saved_user_rsp),
        cs = const USER_CODE_SELECTOR.0 as u64,
        ss = const USER_DATA_SELECTOR.0 as u64,
        SYSCALL_SENTINEL = const SYSCALL_SENTINEL_INT_NUM,
    )
}

const PIT_FREQUENCY: u32 = 1_193_182;
const DESIRED_HZ: u32 = 100;
const DIVISOR: u16 = (PIT_FREQUENCY / DESIRED_HZ) as u16;
pub const TICK_NS: u64 = 1_000_000_000 * DIVISOR as u64 / PIT_FREQUENCY as u64;

unsafe fn init_pit() {
    unsafe {
        let mut cmd: Port<u8> = Port::new(0x43);
        let mut data: Port<u8> = Port::new(0x40);

        // channel 0, lobyte/hibyte, rate generator mode
        cmd.write(0x36);
        data.write((DIVISOR & 0xFF) as u8); // low byte
        data.write(((DIVISOR >> 8) & 0xFF) as u8); // high byte
    }
}

pub unsafe fn fill_idt_tss(idt: &mut InterruptDescriptorTable, tss: &mut TaskStateSegment) {
    set_ist_stack(tss, DOUBLE_FAULT_IST_INDEX, &raw mut DOUBLE_FAULT_STACK);
    tables::fill_idt(idt);
}

pub unsafe fn init() {
    unsafe {
        let mut pics = PICS.lock();
        pics.initialize();
        pics.write_masks(0xfe, 0xff);
        init_pit();
    }

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
