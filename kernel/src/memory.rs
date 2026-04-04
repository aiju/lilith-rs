use core::alloc::Layout;
use x86_64::{PhysAddr, VirtAddr};

mod address_space;
mod bootstrap;
mod buddy;
mod debug_info;
mod frame_info;
mod rbtree;
mod slub;
mod virtual_alloc;

pub use address_space::UserAddressSpace;
pub use address_space::page_fault_handler;
pub use bootstrap::init;
pub use frame_info::{FRAME_SHIFT, FRAME_SIZE};

use crate::memory::virtual_alloc::VIRTUAL_ALLOCATOR;
use crate::memory::virtual_alloc::VIRTUAL_ALLOCATOR_END;
use crate::memory::virtual_alloc::VIRTUAL_ALLOCATOR_START;
use crate::memory::{
    buddy::{BUDDY_ALLOCATOR, BUDDY_MAX},
    slub::{SLUB_ALLOCATOR, SLUB_MAX},
};
use crate::util::clog2;

pub const PHYSICAL_MEMORY_OFFSET: VirtAddr = VirtAddr::new_truncate(0xFFFF_8000_0000_0000);
pub const PHYSICAL_MEMORY_MAX_SIZE: usize = 4 * 1024 * 1024 * 1024 * 1024;
pub const KERNEL_STACK_TOP: VirtAddr = VirtAddr::new_truncate(0xFFFF_FFFF_A000_0000); // needs to match boot.s
pub const KERNEL_STACK_SIZE: usize = 255 * 4096;

pub const FRAME_LAYOUT: Layout =
    unsafe { Layout::from_size_align_unchecked(FRAME_SIZE, FRAME_SIZE) };

#[derive(Clone, Debug)]
pub enum MemoryError {
    OutOfMemory,
    OutOfVirtual,
    SizeTooBig,
}

pub fn is_user_address(virt: VirtAddr) -> bool {
    virt.as_u64() >> 48 == 0
}

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

pub fn direct_alloc(layout: Layout) -> Result<VirtAddr, MemoryError> {
    let effective_size = layout.size().max(layout.align());
    if layout.size() == 0 {
        Ok(ZST_SENTINEL)
    } else if effective_size <= SLUB_MAX {
        // equivalent to size <= SLUB_MAX && align <= SLUB_MAX
        SLUB_ALLOCATOR.lock().alloc(layout)
    } else if effective_size <= BUDDY_MAX {
        let order = clog2(effective_size).saturating_sub(FRAME_SHIFT);
        BUDDY_ALLOCATOR.lock().alloc(order).map(phys_to_virt)
    } else {
        Err(MemoryError::SizeTooBig)
    }
}

pub fn direct_alloc_ptr<T>() -> Result<*mut T, MemoryError> {
    direct_alloc(Layout::new::<T>()).map(|a| a.as_mut_ptr())
}

pub fn kernel_alloc(layout: Layout) -> Result<VirtAddr, MemoryError> {
    direct_alloc(layout).or_else(|_| VIRTUAL_ALLOCATOR.lock().alloc(layout))
}

pub fn alloc_frame() -> Result<PhysAddr, MemoryError> {
    BUDDY_ALLOCATOR.lock().alloc(0)
}

pub unsafe fn direct_free(addr: VirtAddr) {
    if addr == ZST_SENTINEL {
        return;
    }
    let phys = virt_to_phys(addr).expect("non-direct mapped address passed to direct_free");
    let fi = frame_info::frame_info(phys);
    match fi.ty() {
        frame_info::FrameType::BuddyAllocated => unsafe {
            let order = (*fi.u.get()).buddy_list.order;
            BUDDY_ALLOCATOR.lock().free(phys, order as usize);
        },
        frame_info::FrameType::Slab => unsafe { SLUB_ALLOCATOR.lock().free(addr) },
        _ => panic!("invalid address passed to direct_free"),
    }
}

pub unsafe fn kernel_free(addr: VirtAddr) {
    if addr >= PHYSICAL_MEMORY_OFFSET && addr < PHYSICAL_MEMORY_OFFSET + PHYSICAL_MEMORY_MAX_SIZE {
        unsafe { direct_free(addr) };
    } else if addr >= VIRTUAL_ALLOCATOR_START && addr < VIRTUAL_ALLOCATOR_END {
        unsafe { VIRTUAL_ALLOCATOR.lock().free(addr) }
    } else {
        panic!("invalid address passed to kernel_free");
    }
}

pub unsafe fn zero_frame(addr: PhysAddr) {
    let virt = phys_to_virt(addr);
    unsafe { core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, FRAME_SIZE) };
}

struct GlobalAlloc;

#[global_allocator]
static GLOBAL_ALLOC: GlobalAlloc = GlobalAlloc;

unsafe impl core::alloc::GlobalAlloc for GlobalAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        if let Ok(addr) = kernel_alloc(layout) {
            addr.as_mut_ptr()
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        unsafe { kernel_free(VirtAddr::from_ptr(ptr)) }
    }
}
