use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering},
};

use alloc::sync::Arc;
use x86_64::{
    VirtAddr,
    instructions::tables::load_tss,
    registers::{
        model_specific::GsBase,
        segmentation::{CS, DS, ES, Segment},
    },
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        idt::InterruptDescriptorTable,
        tss::TaskStateSegment,
    },
};

use crate::{
    sched::ThreadId,
    sync::{BootInit, InterruptGuard, interrupt_guard},
    user::Proc,
};

pub struct MachDescriptors {
    pub gdt: GlobalDescriptorTable,
    pub tss: TaskStateSegment,
    pub idt: InterruptDescriptorTable,
}

pub struct Mach {
    pub descriptors: UnsafeCell<MachDescriptors>,
    pub current_thread_id: AtomicUsize,
    pub current_proc: AtomicPtr<Proc>,

    /// this counter counts how many reasons there are for interrupts to be disabled (mostly IrqLocks that are held)
    ///
    /// important invariants:
    /// 1. interrupt flag is set iff irq_lock_count != 0
    /// 2. irq_lock_count == 1 when calling sched() (only the scheduler lock is held)
    ///
    /// corolloraries:
    /// 1. IrqLocks and InterruptGuards increment/decrement the count
    /// 2. the interrupt entry point has to increment/decrement the count
    /// 3. 'blocking' interrupt handlers may call sched but have to accept interrupts being re-enabled
    /// 4. 'non-blocking' interrupt handlers have to rely on irq_need_resched to call sched for them (which fiddles with the count)
    pub irq_lock_count: AtomicUsize,

    /// interrupt handlers can set this flag to mark that rescheduling is needed when falling out of the interrupt handler
    ///
    /// the flag is only acted upon when irq_lock_count == 1 at the end of the handler
    pub irq_need_resched: AtomicBool,
    pub ticks: AtomicU64,

    /// the syscall stub uses this is a scratch space to briefly stash the user rsp while we load the kernel stack
    pub syscall_saved_user_rsp: AtomicU64,
}

static MACH0: BootInit<Mach> = unsafe { BootInit::uninit() };

pub const KERNEL_CODE_SELECTOR: SegmentSelector = SegmentSelector(8);
pub const KERNEL_DATA_SELECTOR: SegmentSelector = SegmentSelector(16);
pub const USER_DATA_SELECTOR: SegmentSelector = SegmentSelector(27);
pub const USER_CODE_SELECTOR: SegmentSelector = SegmentSelector(35);
pub const TSS_SELECTOR: SegmentSelector = SegmentSelector(40);

pub unsafe fn init() -> InterruptGuard {
    let mach = unsafe {
        BootInit::set(
            &MACH0,
            Mach {
                descriptors: UnsafeCell::new(MachDescriptors {
                    gdt: GlobalDescriptorTable::new(),
                    tss: TaskStateSegment::new(),
                    idt: InterruptDescriptorTable::new(),
                }),
                current_proc: AtomicPtr::null(),
                current_thread_id: AtomicUsize::new(0),
                ticks: AtomicU64::new(0),
                irq_lock_count: AtomicUsize::new(0),
                irq_need_resched: AtomicBool::new(false),
                syscall_saved_user_rsp: AtomicU64::default(),
            },
        )
    };
    let interrupt_guard = interrupt_guard();

    let MachDescriptors { gdt, tss, idt } = unsafe { &mut *mach.descriptors.get() };

    GsBase::write(VirtAddr::from_ptr(&*MACH0));

    assert_eq!(
        gdt.add_entry(Descriptor::kernel_code_segment()),
        KERNEL_CODE_SELECTOR
    );
    assert_eq!(
        gdt.add_entry(Descriptor::kernel_data_segment()),
        KERNEL_DATA_SELECTOR
    );
    assert_eq!(
        gdt.add_entry(Descriptor::user_data_segment()),
        USER_DATA_SELECTOR
    );
    assert_eq!(
        gdt.add_entry(Descriptor::user_code_segment()),
        USER_CODE_SELECTOR
    );
    assert_eq!(
        gdt.add_entry(unsafe { Descriptor::tss_segment_unchecked(tss) }),
        TSS_SELECTOR
    );

    // this is needed for SYSRET to work correctly
    assert_eq!(USER_DATA_SELECTOR.0 + 8, USER_CODE_SELECTOR.0);

    unsafe {
        gdt.load_unsafe();
        CS::set_reg(KERNEL_CODE_SELECTOR);
        DS::set_reg(KERNEL_DATA_SELECTOR);
        ES::set_reg(KERNEL_DATA_SELECTOR);
        load_tss(TSS_SELECTOR);
        idt.load_unsafe();
    }
    interrupt_guard
}

pub fn mach() -> &'static Mach {
    &*MACH0
}

impl Mach {
    pub fn current_proc(&self) -> Option<Arc<Proc>> {
        let ptr = self.current_proc.load(Ordering::Relaxed);
        (!ptr.is_null()).then(|| unsafe {
            Arc::increment_strong_count(ptr);
            Arc::from_raw(ptr)
        })
    }
    pub fn current_thread_id(&self) -> ThreadId {
        ThreadId::from(self.current_thread_id.load(Ordering::Relaxed))
    }
    pub fn ticks(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }
}
