use core::fmt::Debug;
use core::{alloc::Layout, ops::Range};

use bootloader::{
    BootInfo,
    bootinfo::{MemoryMap, MemoryRegion, MemoryRegionType},
};
use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTableFlags, PhysFrame,
        Size1GiB, Size4KiB, Translate,
    },
};

use crate::memory::FRAME_LAYOUT;
use crate::memory::buddy::buddy_init;
use crate::memory::slub::slub_init;
use crate::memory::{
    BOOT_INFO, KERNEL_STACK_SIZE, KERNEL_STACK_TOP, PHYSICAL_MEMORY_MAX_SIZE,
    PHYSICAL_MEMORY_OFFSET,
    frame_info::{FrameInfo, frame_info_addr},
    phys_to_mut, phys_to_virt,
};

struct BootAlloc {
    map: &'static MemoryMap,
    region_index: usize,
    alloc_address: u64,
}

fn boot_usable(region: &MemoryRegion) -> bool {
    match region.region_type {
        MemoryRegionType::Usable => true,
        _ => false,
    }
}

fn needs_frame_info(region: &MemoryRegion) -> bool {
    match region.region_type {
        MemoryRegionType::Usable
        | MemoryRegionType::BootInfo
        | MemoryRegionType::Kernel
        | MemoryRegionType::PageTable
        | MemoryRegionType::KernelStack => true,
        _ => false,
    }
}

fn boot_reclaimable(region: &MemoryRegion) -> bool {
    match region.region_type {
        MemoryRegionType::Usable | MemoryRegionType::PageTable => true,
        _ => false,
    }
}

fn boot_oom() -> ! {
    panic!("out of memory on boot -- shouldn't happen");
}

impl BootAlloc {
    pub fn new(map: &'static MemoryMap) -> BootAlloc {
        BootAlloc {
            map,
            region_index: 0,
            alloc_address: 0,
        }
    }
    pub fn alloc(&mut self, layout: Layout) -> PhysAddr {
        loop {
            if self.region_index == self.map.len() {
                boot_oom();
            }
            if boot_usable(&self.map[self.region_index]) {
                let range = &self.map[self.region_index].range;
                if let Some(alloc_addr) = self
                    .alloc_address
                    .max(range.start_addr())
                    .checked_next_multiple_of(layout.align() as u64)
                {
                    if range.end_addr().saturating_sub(alloc_addr) as usize >= layout.size() {
                        self.alloc_address = alloc_addr + layout.size() as u64;
                        return PhysAddr::new(alloc_addr);
                    }
                }
            }
            self.region_index += 1;
        }
    }
    pub fn reclaimable_range_iter(&self) -> impl Iterator<Item = Range<PhysAddr>> {
        self.map.iter().filter_map(|r| {
            if boot_reclaimable(r) {
                let mut bot = r.range.start_addr();
                let top = r.range.end_addr();
                if self.alloc_address > bot && boot_usable(r) {
                    bot = self.alloc_address;
                }
                if top > bot {
                    unsafe { Some(PhysAddr::new_unsafe(bot)..PhysAddr::new_unsafe(top)) }
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootAlloc {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        unsafe {
            Some(PhysFrame::from_start_address_unchecked(
                self.alloc(FRAME_LAYOUT),
            ))
        }
    }
}

unsafe extern "C" {
    static __text_start: u8;
    static __data_start: u8;
    static __bss_end: u8;
}

#[inline(always)]
fn text_start() -> VirtAddr {
    VirtAddr::from_ptr(&raw const __text_start)
}
#[inline(always)]
fn data_start() -> VirtAddr {
    VirtAddr::from_ptr(&raw const __data_start)
}
#[inline(always)]
fn bss_end() -> VirtAddr {
    VirtAddr::from_ptr(&raw const __bss_end)
}

struct Bootstrap<'a> {
    boot_alloc: BootAlloc,
    offset_page_table: OffsetPageTable<'a>,
    previous_table: OffsetPageTable<'a>,
}

impl Bootstrap<'_> {
    unsafe fn map<S>(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags)
    where
        S: PageSize + Debug,
        for<'a> OffsetPageTable<'a>: Mapper<S>,
    {
        unsafe {
            self.offset_page_table
                .map_to(
                    Page::<S>::from_start_address_unchecked(virt),
                    PhysFrame::from_start_address_unchecked(phys),
                    flags,
                    &mut self.boot_alloc,
                )
                .expect("map error -- shouldn't happen")
                .ignore();
        }
    }
    unsafe fn ensure_allocated(&mut self, virt: VirtAddr) -> PhysAddr {
        unsafe {
            let flags_rwdata = PageTableFlags::PRESENT
                | PageTableFlags::GLOBAL
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_EXECUTE;
            match self.offset_page_table.translate_addr(virt) {
                None => {
                    let phys = self
                        .boot_alloc
                        .alloc(Layout::from_size_align_unchecked(4096, 4096));
                    self.map::<Size4KiB>(virt.align_down(4096u64), phys, flags_rwdata);
                    phys + (virt.as_u64() & 4095)
                }
                Some(phys) => phys,
            }
        }
    }
    unsafe fn copy_mapping_4kib(&mut self, start: VirtAddr, end: VirtAddr, flags: PageTableFlags) {
        assert!(start.is_aligned(4096u64));
        for virt in (start..end).step_by(4096) {
            let phys = self
                .previous_table
                .translate_addr(virt)
                .expect("page not mapped");
            unsafe { self.map::<Size4KiB>(virt, phys, flags) };
        }
    }
    unsafe fn setup_page_table(&mut self) {
        unsafe {
            let flags_rwdata = PageTableFlags::PRESENT
                | PageTableFlags::GLOBAL
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_EXECUTE;
            let flags_code = PageTableFlags::PRESENT | PageTableFlags::GLOBAL;

            self.offset_page_table.level_4_table().zero();

            // set up direct map at PHYSICAL_MEMORY_OFFSET
            for i in (0..PHYSICAL_MEMORY_MAX_SIZE).step_by(1024 * 1024 * 1024) {
                self.map::<Size1GiB>(
                    PHYSICAL_MEMORY_OFFSET + i,
                    PhysAddr::new_unsafe(i as u64),
                    flags_rwdata,
                );
            }

            // map kernel text / data / bss
            self.copy_mapping_4kib(text_start(), data_start(), flags_code);
            self.copy_mapping_4kib(data_start(), bss_end(), flags_rwdata);

            // map bootinfo page
            self.copy_mapping_4kib(BOOT_INFO, BOOT_INFO + 4096u64, flags_rwdata);

            // map kernel stack
            self.copy_mapping_4kib(
                KERNEL_STACK_TOP - KERNEL_STACK_SIZE as u64,
                KERNEL_STACK_TOP,
                flags_rwdata,
            );

            // allocate and map pages for FrameInfo structs
            for region in self.boot_alloc.map.iter() {
                if needs_frame_info(region) {
                    for frame_addr in
                        (region.range.start_addr()..region.range.end_addr()).step_by(4096)
                    {
                        // TODO: this can be more intelligent instead of iterating over every page
                        let phys = self
                            .ensure_allocated(frame_info_addr(PhysAddr::new_unsafe(frame_addr)));
                        FrameInfo::new_at(phys_to_virt(phys).as_mut_ptr());
                    }
                }
            }
        }
    }
}

pub unsafe fn init(boot_info: &'static BootInfo) {
    unsafe {
        assert_eq!(
            PHYSICAL_MEMORY_OFFSET.as_u64(),
            boot_info.physical_memory_offset
        );

        let mut boot_alloc = BootAlloc::new(&boot_info.memory_map);
        let page_table = boot_alloc.alloc(Layout::from_size_align_unchecked(4096, 4096));
        super::address_space::init(page_table);
        let offset_page_table =
            OffsetPageTable::new(phys_to_mut(page_table), PHYSICAL_MEMORY_OFFSET);
        let previous_table_ref = phys_to_mut(Cr3::read().0.start_address());
        let previous_table = OffsetPageTable::new(previous_table_ref, PHYSICAL_MEMORY_OFFSET);

        let mut bootstrap = Bootstrap {
            boot_alloc,
            offset_page_table,
            previous_table,
        };

        bootstrap.setup_page_table();

        let flags = Cr3::read().1;
        Cr3::write(PhysFrame::from_start_address_unchecked(page_table), flags);

        buddy_init(bootstrap.boot_alloc.reclaimable_range_iter());
        slub_init();
    }
}
