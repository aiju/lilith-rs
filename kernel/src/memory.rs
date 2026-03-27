use core::alloc::Layout;
use x86_64::{PhysAddr, VirtAddr};

mod address_space;
mod bootstrap;
mod buddy;
mod frame_info;
mod slub;

pub use address_space::AddressSpace;
pub use bootstrap::init;

use crate::memory::{
    buddy::{BUDDY_ALLOCATOR, BUDDY_MAX},
    frame_info::{FRAME_SHIFT, FRAME_SIZE},
    slub::{SLUB_ALLOCATOR, SLUB_MAX},
};

pub const PHYSICAL_MEMORY_OFFSET: VirtAddr = VirtAddr::new_truncate(0xFFFF_8000_0000_0000);
pub const PHYSICAL_MEMORY_MAX_SIZE: usize = 4 * 1024 * 1024 * 1024 * 1024;
pub const KERNEL_STACK_TOP: VirtAddr = VirtAddr::new_truncate(0xFFFF_FFFF_A000_0000);
pub const KERNEL_STACK_SIZE: usize = 255 * 4096;
pub const BOOT_INFO: VirtAddr = VirtAddr::new_truncate(0xFFFF_FFFF_9000_0000);

pub const FRAME_LAYOUT: Layout =
    unsafe { Layout::from_size_align_unchecked(FRAME_SIZE, FRAME_SIZE) };

pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    assert!(phys.as_u64() < PHYSICAL_MEMORY_MAX_SIZE as u64);
    PHYSICAL_MEMORY_OFFSET + phys.as_u64()
}

pub fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    (virt >= PHYSICAL_MEMORY_OFFSET && virt < PHYSICAL_MEMORY_OFFSET + PHYSICAL_MEMORY_MAX_SIZE)
        .then(|| unsafe { PhysAddr::new_unsafe(virt - PHYSICAL_MEMORY_OFFSET) })
}

pub unsafe fn phys_to_mut<T>(phys: PhysAddr) -> &'static mut T {
    unsafe { &mut *phys_to_virt(phys).as_mut_ptr() }
}

/// sentinel value used for zero-sized allocations
///
/// should satisfy any reasonable alignment
pub const ZST_SENTINEL: VirtAddr = PHYSICAL_MEMORY_OFFSET;

pub fn kernel_alloc(layout: Layout) -> Option<VirtAddr> {
    // TODO: this only works bc SLUB allocator only uses power-of-two sizes!!
    let effective_size = layout.size().max(layout.align());
    if layout.size() == 0 {
        Some(ZST_SENTINEL)
    } else if effective_size <= SLUB_MAX {
        SLUB_ALLOCATOR.lock().alloc(effective_size)
    } else if effective_size <= BUDDY_MAX {
        let n = usize::BITS - (effective_size - 1).leading_zeros() - FRAME_SHIFT as u32;
        BUDDY_ALLOCATOR.lock().alloc(n as usize).map(phys_to_virt)
    } else {
        None
    }
}

pub unsafe fn kernel_free(addr: VirtAddr) {
    if addr == ZST_SENTINEL {
        return;
    }
    let phys = virt_to_phys(addr).expect("non-direct mapped address passed to kernel_free");
    let fi = frame_info::frame_info(phys);
    match fi.ty() {
        frame_info::FrameType::BuddyAllocated => unsafe {
            let order = (*fi.u.get()).buddy_list.order;
            BUDDY_ALLOCATOR.lock().free(phys, order as usize);
        },
        frame_info::FrameType::Slab => unsafe { SLUB_ALLOCATOR.lock().free(addr) },
        _ => panic!("invalid address passed to kernel_free"),
    }
}

struct GlobalAlloc;

#[global_allocator]
static GLOBAL_ALLOC: GlobalAlloc = GlobalAlloc;

unsafe impl core::alloc::GlobalAlloc for GlobalAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        if let Some(addr) = kernel_alloc(layout) {
            addr.as_mut_ptr()
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        unsafe { kernel_free(VirtAddr::from_ptr(ptr)) }
    }
}
