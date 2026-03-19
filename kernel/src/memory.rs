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
        Translate, mapper::MapToError, page_table::PageTableEntry,
    },
};

use crate::println;

pub const PHYSICAL_MEMORY_OFFSET: VirtAddr = VirtAddr::new_truncate(0xFFFF_8000_0000_0000);

pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    PHYSICAL_MEMORY_OFFSET + phys.as_u64()
}

pub unsafe fn phys_to_mut<T>(phys: PhysAddr) -> &'static mut T {
    unsafe { &mut *phys_to_virt(phys).as_mut_ptr() }
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

pub struct OwnedPageTable(PhysAddr);

impl OwnedPageTable {
    pub fn new() -> Option<Self> {
        let mut memory_manager = memory_manager().lock();
        let addr = memory_manager
            .frame_allocator
            .allocate_frame()?
            .start_address();
        let table: &mut PageTable = unsafe { phys_to_mut(addr) };
        let global = unsafe { memory_manager.global_page_table.as_mut() };
        for i in 0..256 {
            table[i] = PageTableEntry::new();
        }
        for i in 256..512 {
            table[i] = global[i].clone();
        }
        Some(OwnedPageTable(addr))
    }
    pub fn is_active(&self) -> bool {
        Cr3::read().0.start_address() == self.0
    }
    pub unsafe fn activate(&mut self) {
        let (_, flags) = Cr3::read();
        unsafe { Cr3::write(PhysFrame::from_start_address_unchecked(self.0), flags) };
    }
    unsafe fn from_current() -> Self {
        OwnedPageTable(Cr3::read().0.start_address())
    }
    unsafe fn as_mut(&mut self) -> &mut PageTable {
        unsafe { phys_to_mut(self.0) }
    }
}

pub struct MemoryManager {
    pub frame_allocator: FrameAllocator,
    pub heap_avail: VirtAddr,
    pub global_page_table: OwnedPageTable,
}

static MEMORY_MANAGER: Once<Mutex<MemoryManager>> = Once::new();

pub fn init(boot_info: &'static BootInfo) {
    assert_eq!(
        PHYSICAL_MEMORY_OFFSET.as_u64(),
        boot_info.physical_memory_offset
    );
    let frame_allocator = FrameAllocator::from(&boot_info.memory_map);
    let global_page_table = unsafe { OwnedPageTable::from_current() };

    MEMORY_MANAGER.call_once(|| {
        Mutex::new(MemoryManager {
            frame_allocator,
            global_page_table,
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
            ref mut global_page_table,
            ref mut frame_allocator,
            ..
        } = *man;
        let mut page_table =
            unsafe { OffsetPageTable::new(global_page_table.as_mut(), PHYSICAL_MEMORY_OFFSET) };
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

pub struct AddressSpace {
    page_table: OwnedPageTable,
}

impl AddressSpace {
    pub fn new() -> Option<Self> {
        let page_table = OwnedPageTable::new()?;
        Some(AddressSpace { page_table })
    }
    pub unsafe fn offset_page_table(&mut self) -> OffsetPageTable<'_> {
        unsafe { OffsetPageTable::new(self.page_table.as_mut(), PHYSICAL_MEMORY_OFFSET) }
    }
    pub unsafe fn ensure_allocated(&mut self, start: VirtAddr, len: usize) {
        assert!(start.as_u64() >> 48 == 0);
        let mut pt = unsafe { self.offset_page_table() };
        let mut page = start.as_u64() & !4095;
        let frame_allocator = &mut memory_manager().lock().frame_allocator;
        while page < start.as_u64() + (len as u64) {
            let va = unsafe { VirtAddr::new_unsafe(page) };
            if pt.translate_addr(va).is_none() {
                let frame = frame_allocator.allocate_frame().unwrap();
                unsafe {
                    pt.map_to(
                        Page::from_start_address_unchecked(va),
                        frame,
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
                        frame_allocator,
                    )
                    .unwrap()
                    .flush()
                };
            }
            page += 4096;
        }
    }
    pub unsafe fn access_via_direct_map(&mut self, addr: VirtAddr) -> Option<&'static mut [u8]> {
        assert!(addr.as_u64() >> 63 == 0);
        unsafe {
            let pt = self.offset_page_table();
            let phys_addr = pt.translate_addr(addr)?;
            Some(core::slice::from_raw_parts_mut(
                phys_to_virt(phys_addr).as_mut_ptr(),
                (4096 - (phys_addr.as_u64() & 4095)) as usize,
            ))
        }
    }
    pub unsafe fn activate(&mut self) {
        unsafe { self.page_table.activate() };
    }
}
