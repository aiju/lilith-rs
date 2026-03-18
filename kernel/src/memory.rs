use core::ops::Range;

use arrayvec::ArrayVec;
use bootloader::{
    BootInfo,
    bootinfo::{MemoryMap, MemoryRegionType},
};
use paging::FrameAllocator as _;
use spin::{Mutex, Once};
use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        self, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
        mapper::MapToError,
    },
};

// UNSAFE: physical address space must be mapped at physical_memory_offset. this function must only be called once.
pub unsafe fn offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr = virt.as_u64() as *mut PageTable;
    unsafe { OffsetPageTable::new(&mut *page_table_ptr, physical_memory_offset) }
}

const MAX_SPANS: usize = 32;

pub struct FrameAllocator {
    spans: ArrayVec<Range<u64>, MAX_SPANS>,
}

impl From<&MemoryMap> for FrameAllocator {
    fn from(value: &MemoryMap) -> Self {
        let mut spans = ArrayVec::new();
        for region in value.iter() {
            match region.region_type {
                MemoryRegionType::Usable => {
                    spans.push(region.range.start_addr()..region.range.end_addr());
                }
                _ => {}
            }
        }
        FrameAllocator { spans }
    }
}

impl FrameAllocator {
    pub fn free_bytes(&self) -> u64 {
        let mut free = 0;
        for span in &self.spans {
            free += span.end - span.start;
        }
        free
    }
}

unsafe impl paging::FrameAllocator<Size4KiB> for FrameAllocator {
    fn allocate_frame(&mut self) -> Option<paging::PhysFrame<Size4KiB>> {
        let span = self.spans.first_mut()?;
        let addr = span.start;
        span.start += 4096;
        if span.start >= span.end {
            assert_eq!(span.start, span.end);
            self.spans.remove(0);
        }
        Some(PhysFrame::from_start_address(PhysAddr::new(addr)).unwrap())
    }
}

pub struct MemoryManager {
    pub frame_allocator: FrameAllocator,
    pub physical_memory_offset: VirtAddr,
    pub page_table: OffsetPageTable<'static>,
    pub heap_avail: VirtAddr,
}

static MEMORY_MANAGER: Once<Mutex<MemoryManager>> = Once::new();

pub fn init(boot_info: &'static BootInfo) {
    let frame_allocator = FrameAllocator::from(&boot_info.memory_map);
    let physical_memory_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let page_table = unsafe { offset_page_table(physical_memory_offset) };

    MEMORY_MANAGER.call_once(|| {
        Mutex::new(MemoryManager {
            frame_allocator,
            page_table,
            physical_memory_offset,
            heap_avail: HEAP_START,
        })
    });
}

pub fn memory_manager() -> &'static Mutex<MemoryManager> {
    MEMORY_MANAGER.get().expect("memory manager uninitializer")
}

impl MemoryManager {
    pub fn free_bytes(&self) -> u64 {
        self.frame_allocator.free_bytes()
    }
}

struct GlobalAlloc;

#[global_allocator]
static GLOBAL_ALLOC: GlobalAlloc = GlobalAlloc;

const HEAP_START: VirtAddr = VirtAddr::new_truncate(0xFFFF_FFFF_A000_0000);
const HEAP_END: VirtAddr = VirtAddr::new_truncate(0xFFFF_FFFF_B000_0000);

unsafe impl core::alloc::GlobalAlloc for GlobalAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut man = memory_manager().lock();
        let addr = man
            .heap_avail
            .as_u64()
            .next_multiple_of(layout.align() as u64);
        let new_end = addr + layout.size() as u64;
        if new_end > HEAP_END.as_u64() {
            return core::ptr::null_mut();
        }
        let mut alloc_addr = man.heap_avail.as_u64().next_multiple_of(4096);
        let alloc_end = new_end.next_multiple_of(4096);
        let MemoryManager {
            ref mut page_table,
            ref mut frame_allocator,
            ..
        } = *man;
        while alloc_addr < alloc_end {
            let Some(frame) = frame_allocator.allocate_frame() else {
                return core::ptr::null_mut();
            };
            unsafe {
                match page_table.map_to(
                    Page::from_start_address_unchecked(VirtAddr::new_unsafe(alloc_addr)),
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    frame_allocator,
                ) {
                    Ok(flush) => flush.flush(),
                    Err(MapToError::FrameAllocationFailed) => return core::ptr::null_mut(),
                    Err(err) => panic!("error in alloc: {err:?}"),
                }
            }
            alloc_addr += 4096;
        }
        man.heap_avail = VirtAddr::new(new_end);
        addr as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}
