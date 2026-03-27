use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};

use x86_64::{PhysAddr, VirtAddr};

use crate::memory::{buddy::BuddyList, slub::SlabList};

pub(super) union FrameInfoData {
    none: (),
    pub(super) buddy_list: BuddyList,
    pub(super) slab_list: SlabList,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameType {
    Reserved,
    Buddy,
    BuddyTail,
    BuddyAllocated,
    Slab,
}

#[repr(align(64))]
pub struct FrameInfo {
    flags: AtomicU64,
    pub(super) u: UnsafeCell<FrameInfoData>,
}

const FRAME_FLAGS_TYPE_MASK: u64 = 0xFF;

pub const FRAME_SIZE: usize = 4096;
pub const FRAME_SHIFT: usize = 12;
const FRAME_INFO_ADDR: VirtAddr = VirtAddr::new_truncate(0xFFFF_0000_0000_0000);
pub const FRAME_INFO_SHIFT: usize = core::mem::size_of::<FrameInfo>().ilog2() as usize;

pub fn frame_info_addr(addr: PhysAddr) -> VirtAddr {
    FRAME_INFO_ADDR
        + (addr.as_u64() / FRAME_SIZE as u64) * (core::mem::size_of::<FrameInfo>() as u64)
}

pub fn from_frame_info_addr(virt: VirtAddr) -> PhysAddr {
    unsafe {
        PhysAddr::new_unsafe(
            ((virt - FRAME_INFO_ADDR) / core::mem::size_of::<FrameInfo>() as u64)
                * (FRAME_SIZE as u64),
        )
    }
}

pub fn frame_info(addr: PhysAddr) -> &'static FrameInfo {
    unsafe { &*frame_info_addr(addr).as_ptr() }
}

impl FrameInfo {
    pub unsafe fn new_at(ptr: *mut FrameInfo) {
        unsafe {
            let fi = FrameInfo {
                flags: AtomicU64::new(FrameType::Reserved as u64),
                u: UnsafeCell::new(FrameInfoData { none: () }),
            };
            core::ptr::write(ptr, fi);
        }
    }

    pub unsafe fn addr(&self) -> PhysAddr {
        from_frame_info_addr(VirtAddr::from_ptr(self))
    }

    pub fn ty(&self) -> FrameType {
        unsafe {
            core::mem::transmute::<u8, FrameType>(
                (self.flags.load(Ordering::Relaxed) & FRAME_FLAGS_TYPE_MASK) as u8,
            )
        }
    }

    pub unsafe fn set_ty(&self, ty: FrameType) {
        self.flags
            .update(Ordering::Relaxed, Ordering::Relaxed, |f| {
                f & !FRAME_FLAGS_TYPE_MASK | ty as u8 as u64
            });
    }
}
