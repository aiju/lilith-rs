use core::ops::Range;

use x86_64::PhysAddr;

use crate::{
    memory::frame_info::{
        FRAME_INFO_SHIFT, FRAME_SHIFT, FRAME_SIZE, FrameInfo, FrameType, frame_info,
    },
    sync::IrqLock,
};

#[derive(Clone, Copy)]
pub(super) struct BuddyList {
    next: *mut BuddyList,
    prev: *mut BuddyList,
    pub(super) order: u8,
}

unsafe impl Send for BuddyList {}

impl BuddyList {
    fn insert(&mut self, item: *mut BuddyList) {
        unsafe {
            *item = BuddyList {
                prev: self.prev,
                next: self,
                order: self.order,
            };
            (*self.prev).next = item;
            self.prev = item;
        }
    }
    fn pop_front(&mut self) -> Option<*mut BuddyList> {
        unsafe {
            let item = self.next;
            if item != self {
                self.next = (*item).next;
                (*self.next).prev = self;
                (*item).prev = core::ptr::null_mut();
                (*item).next = core::ptr::null_mut();
                Some(item)
            } else {
                None
            }
        }
    }
    fn unlink(&mut self) {
        unsafe {
            (*self.next).prev = self.prev;
            (*self.prev).next = self.next;
            self.prev = core::ptr::null_mut();
            self.next = core::ptr::null_mut();
        }
    }
    unsafe fn frame_info(bl: *mut BuddyList) -> &'static FrameInfo {
        unsafe {
            &*(((bl as u64) - core::mem::offset_of!(FrameInfo, u) as u64) as *const FrameInfo)
        }
    }
}

const MAX_ORDER: usize = 10;
pub const BUDDY_MAX: usize = FRAME_SIZE << MAX_ORDER;

pub struct BuddyAllocator {
    heads: [BuddyList; MAX_ORDER + 1],
}

pub static BUDDY_ALLOCATOR: IrqLock<BuddyAllocator> = IrqLock::new(BuddyAllocator::dummy());

impl BuddyAllocator {
    const fn dummy() -> BuddyAllocator {
        BuddyAllocator {
            heads: [BuddyList {
                next: core::ptr::null_mut(),
                prev: core::ptr::null_mut(),
                order: 0,
            }; MAX_ORDER + 1],
        }
    }
    fn init(&mut self) {
        for i in 0..=MAX_ORDER {
            self.heads[i] = BuddyList {
                next: &raw mut self.heads[i],
                prev: &raw mut self.heads[i],
                order: i as u8,
            };
        }
    }
}

/// initialize the buddy allocator with the given ranges as free memory
///
/// we assume the FrameInfo structs are already mapped in the correct locations and initialised to show all memory as reserved
pub(super) unsafe fn buddy_init(free_ranges: impl Iterator<Item = Range<PhysAddr>>) {
    assert!(core::mem::size_of::<FrameInfo>().is_power_of_two());

    let mut buddy_allocator = BUDDY_ALLOCATOR.lock();
    buddy_allocator.init();
    for range in free_ranges {
        let mut addr = range.start;
        let end = range.end;
        assert!(addr.is_aligned(FRAME_SIZE as u64) && end.is_aligned(FRAME_SIZE as u64));
        while addr < end {
            let order = ((addr.as_u64() >> FRAME_SHIFT) | (1 << MAX_ORDER))
                .trailing_zeros()
                .min((end - addr).ilog2() - FRAME_SHIFT as u32) as usize;
            let fi = frame_info(addr);
            unsafe {
                buddy_allocator.heads[order].insert(&raw mut (*fi.u.get()).buddy_list);
                fi.set_ty(FrameType::Buddy);
                for a in (addr.as_u64() + FRAME_SIZE as u64..end.as_u64()).step_by(FRAME_SIZE) {
                    frame_info(PhysAddr::new_unsafe(a)).set_ty(FrameType::BuddyTail);
                }
            }
            addr += 1u64 << (order + FRAME_SHIFT);
        }
    }
}

unsafe fn get_buddy(fi: &FrameInfo, order: usize) -> &'static FrameInfo {
    unsafe {
        &*(((fi as *const FrameInfo as u64) ^ (1 << (order + FRAME_INFO_SHIFT)))
            as *const FrameInfo)
    }
}

unsafe fn sort_buddies(fi: &FrameInfo, order: usize) -> (&'static FrameInfo, &'static FrameInfo) {
    unsafe {
        (
            &*(((fi as *const FrameInfo as u64) & !(1 << (order + FRAME_INFO_SHIFT)))
                as *const FrameInfo),
            &*(((fi as *const FrameInfo as u64) | (1 << (order + FRAME_INFO_SHIFT)))
                as *const FrameInfo),
        )
    }
}

impl BuddyAllocator {
    fn alloc_min(&mut self, order: usize) -> Option<(&'static FrameInfo, usize)> {
        for o in order..=MAX_ORDER {
            if let Some(bl) = self.heads[o].pop_front() {
                return Some((unsafe { BuddyList::frame_info(bl) }, o));
            }
        }
        None
    }
    fn split(&mut self, fi: &FrameInfo, mut order: usize, target_order: usize) {
        while order > target_order {
            unsafe {
                let other_fi = &*(fi as *const FrameInfo).add(1 << (order - 1));
                self.heads[order - 1].insert(&mut (*other_fi.u.get()).buddy_list);
                other_fi.set_ty(FrameType::Buddy);
            }
            order -= 1;
        }
        unsafe { (*fi.u.get()).buddy_list.order = order as u8 };
    }
    pub fn alloc(&mut self, order: usize) -> Option<PhysAddr> {
        assert!(order <= MAX_ORDER);

        let (fi, o) = self.alloc_min(order)?;
        self.split(fi, o, order);
        unsafe { fi.set_ty(FrameType::BuddyAllocated) };

        Some(unsafe { fi.addr() })
    }
    fn merge(
        &mut self,
        mut fi: &'static FrameInfo,
        mut order: usize,
    ) -> (&'static FrameInfo, usize) {
        while order < MAX_ORDER {
            let buddy = unsafe { get_buddy(fi, order) };
            match buddy.ty() {
                FrameType::Reserved | FrameType::BuddyAllocated | FrameType::Slab => break,
                FrameType::Buddy => unsafe {
                    let bl = &mut (*buddy.u.get()).buddy_list;
                    if bl.order as usize != order {
                        break;
                    }
                    bl.unlink();
                    let (lower, higher) = sort_buddies(fi, order);
                    higher.set_ty(FrameType::BuddyTail);
                    fi = lower;
                    order += 1;
                },
                FrameType::BuddyTail => panic!("buddy tail found in merge -- shouldn't happen"),
            }
        }
        (fi, order)
    }
    pub unsafe fn free(&mut self, addr: PhysAddr, order: usize) {
        let fi = frame_info(addr);

        let (merged_fi, merged_order) = self.merge(fi, order);
        unsafe {
            self.heads[merged_order].insert(&raw mut (*merged_fi.u.get()).buddy_list);
            merged_fi.set_ty(FrameType::Buddy);
        }
    }
}
