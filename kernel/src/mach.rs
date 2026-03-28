use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

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
    sync::{BootInit, IrqLock},
    user::Proc,
};

#[repr(C)]
// in kernel mode this is accessible via GS: prefix
// syscall_entry knows the layout of this struct!!
pub struct MachGsSpace {
    pub kernel_rsp: u64,
    pub user_rsp: u64,
}

pub struct MachDescriptors {
    pub gdt: GlobalDescriptorTable,
    pub tss: TaskStateSegment,
    pub idt: InterruptDescriptorTable,
}

pub struct Mach {
    pub descriptors: IrqLock<MachDescriptors>,
    pub current_thread_id: AtomicUsize,
    pub current_proc: AtomicPtr<Proc>,
    gs_space: *mut MachGsSpace, // because we allow magic access through GS cannot be a reference
}

static MACH0: BootInit<Mach> = unsafe { BootInit::uninit() };

// instead of a spin::Mutex this should probably be something that disables interrupts and panics if locked or smth
static mut MACH_GS_SPACE: MachGsSpace = MachGsSpace {
    kernel_rsp: 0,
    user_rsp: 0,
};

pub const KERNEL_CODE_SELECTOR: SegmentSelector = SegmentSelector(8);
pub const KERNEL_DATA_SELECTOR: SegmentSelector = SegmentSelector(16);
pub const USER_DATA_SELECTOR: SegmentSelector = SegmentSelector(27);
pub const USER_CODE_SELECTOR: SegmentSelector = SegmentSelector(35);
pub const TSS_SELECTOR: SegmentSelector = SegmentSelector(40);

pub unsafe fn init() {
    let mach = unsafe {
        MACH0.set(Mach {
            descriptors: IrqLock::new(MachDescriptors {
                gdt: GlobalDescriptorTable::new(),
                tss: TaskStateSegment::new(),
                idt: InterruptDescriptorTable::new(),
            }),
            current_proc: AtomicPtr::null(),
            current_thread_id: AtomicUsize::new(0),
            gs_space: &raw mut MACH_GS_SPACE,
        })
    };

    let mut descriptor_guard = mach.descriptors.lock();
    let MachDescriptors {
        ref mut gdt,
        ref mut tss,
        ref mut idt,
    } = *descriptor_guard;

    GsBase::write(VirtAddr::from_ptr(mach.gs_space));

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
}

pub fn mach() -> &'static Mach {
    MACH0.get()
}

impl Mach {
    pub fn gs_space(&self) -> &MachGsSpace {
        unsafe { self.gs_space.as_ref_unchecked() }
    }
    pub fn gs_space_mut(&self) -> &mut MachGsSpace {
        unsafe { self.gs_space.as_mut_unchecked() }
    }
    pub fn current_proc(&self) -> Option<Arc<Proc>> {
        let ptr = self.current_proc.load(Ordering::Relaxed);
        (!ptr.is_null()).then(|| unsafe {
            Arc::increment_strong_count(ptr);
            Arc::from_raw(ptr)
        })
    }
}
