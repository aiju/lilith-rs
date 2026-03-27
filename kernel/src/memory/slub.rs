use x86_64::{PhysAddr, VirtAddr};

use crate::{
    memory::{
        buddy::BUDDY_ALLOCATOR,
        frame_info::{FRAME_SIZE, FrameInfo, FrameType, frame_info},
        phys_to_virt, virt_to_phys,
    },
    sync::IrqLock,
};

#[derive(Copy, Clone, Debug)]
struct SlabCache {
    list: SlabList,
    object_size: usize,
    slab_order: u8,
    max_free_count: u16,
}

#[derive(Copy, Clone, Debug)]
pub(super) struct SlabList {
    next: *mut SlabList,
    prev: *mut SlabList,
    free_list: *mut (),
    free_count: u16,
    cache_index: u8,
}

impl SlabList {
    fn insert(&mut self, item: *mut SlabList) {
        unsafe {
            (*item).next = self;
            (*item).prev = self.prev;
            (*self.prev).next = item;
            self.prev = item;
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
    unsafe fn frame_info(bl: *mut SlabList) -> &'static FrameInfo {
        unsafe {
            &*(((bl as u64) - core::mem::offset_of!(FrameInfo, u) as u64) as *const FrameInfo)
        }
    }
}

unsafe impl Send for SlabList {}

const NUM_SIZE_CLASSES: usize = 8;
const SIZE_CLASSES: [usize; NUM_SIZE_CLASSES] = [8, 16, 32, 64, 128, 256, 512, 1024];
pub const SLUB_MAX: usize = SIZE_CLASSES[NUM_SIZE_CLASSES - 1];

pub struct SlubAllocator {
    slabs: [SlabCache; NUM_SIZE_CLASSES],
}

pub static SLUB_ALLOCATOR: IrqLock<SlubAllocator> = IrqLock::new(SlubAllocator::dummy());

impl SlubAllocator {
    const fn dummy() -> Self {
        SlubAllocator {
            slabs: [SlabCache {
                list: SlabList {
                    next: core::ptr::null_mut(),
                    prev: core::ptr::null_mut(),
                    free_list: core::ptr::null_mut(),
                    free_count: 0,
                    cache_index: 0,
                },
                object_size: 0,
                slab_order: 0,
                max_free_count: 0,
            }; NUM_SIZE_CLASSES],
        }
    }
}

fn round_up_size(size: usize) -> Option<(usize, usize)> {
    // TODO: i'm sure we can do better than this
    let mut index = 0;
    while index < NUM_SIZE_CLASSES {
        if size <= SIZE_CLASSES[index] {
            return Some((index, SIZE_CLASSES[index]));
        }
        index += 1;
    }
    None
}

pub(super) unsafe fn slub_init() {
    let mut slub_allocator = SLUB_ALLOCATOR.lock();
    for (index, &size) in SIZE_CLASSES.iter().enumerate() {
        let slab = &mut slub_allocator.slabs[index];
        slab.object_size = size;
        slab.slab_order = 0;
        slab.max_free_count = ((FRAME_SIZE << slab.slab_order) / size).try_into().unwrap();
        slab.list.cache_index = index.try_into().unwrap();
        slab.list.next = &raw mut slab.list;
        slab.list.prev = &raw mut slab.list;
    }
}

impl SlabCache {
    fn init_slab(&self, addr: PhysAddr) {
        let ptr: *mut () = phys_to_virt(addr).as_mut_ptr();
        let mut list_start = core::ptr::null_mut();
        let mut pp = &raw mut list_start;
        for i in 0..self.max_free_count {
            unsafe {
                let p = ptr.byte_add(self.object_size * i as usize) as *mut ();
                *pp = p;
                pp = p.cast();
            }
        }
        unsafe {
            *pp = core::ptr::null_mut();
        }

        let fi = frame_info(addr);
        let sl = unsafe { &raw mut (*fi.u.get()).slab_list };
        unsafe {
            core::ptr::write(
                sl,
                SlabList {
                    next: core::ptr::null_mut(),
                    prev: core::ptr::null_mut(),
                    free_list: list_start,
                    free_count: self.max_free_count,
                    cache_index: self.list.cache_index,
                },
            );
        }
    }

    fn grab_slab(&mut self) -> Option<&'static FrameInfo> {
        if self.list.next != &raw mut self.list {
            unsafe { Some(SlabList::frame_info(self.list.next)) }
        } else {
            let addr = BUDDY_ALLOCATOR.lock().alloc(self.slab_order as usize)?;
            let fi = frame_info(addr);
            unsafe {
                self.init_slab(addr);
                self.list.insert(&raw mut (*fi.u.get()).slab_list);
                fi.set_ty(FrameType::Slab);
            }
            Some(fi)
        }
    }

    fn put_slab(&mut self, fi: &'static FrameInfo) {
        let sl = unsafe { &mut (*fi.u.get()).slab_list };
        if sl.free_count == 1 {
            self.list.insert(sl);
        } else if sl.free_count == self.max_free_count {
            sl.unlink();
            unsafe {
                BUDDY_ALLOCATOR
                    .lock()
                    .free(fi.addr(), self.slab_order as usize);
            }
        }
    }
}

impl SlubAllocator {
    pub fn alloc(&mut self, desired_size: usize) -> Option<VirtAddr> {
        let (index, _) = round_up_size(desired_size)?;
        let fi = self.slabs[index].grab_slab()?;
        let sl = unsafe { &mut (*fi.u.get()).slab_list };
        let p = sl.free_list;
        sl.free_list = unsafe { *(p as *mut *mut ()) };
        sl.free_count -= 1;
        if sl.free_count == 0 {
            sl.unlink();
        }
        Some(VirtAddr::from_ptr(p))
    }
    pub unsafe fn free(&mut self, object: VirtAddr) {
        let phys = virt_to_phys(object).expect("address passed to free not in direct map");
        let fi = frame_info(phys);
        let sl = unsafe { &mut (*fi.u.get()).slab_list };
        assert_eq!(fi.ty(), FrameType::Slab);
        unsafe {
            *object.as_mut_ptr() = sl.free_list;
            sl.free_list = object.as_mut_ptr();
        }
        sl.free_count += 1;
        self.slabs[sl.cache_index as usize].put_slab(fi);
    }
}
