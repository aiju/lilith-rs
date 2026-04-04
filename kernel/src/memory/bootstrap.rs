use crate::memory::boot_alloc::{BootAlloc, NodeSlot, SpanType};
use crate::memory::multiboot::MultibootInfoRaw;
#[allow(unused_imports)]
use crate::prelude::*;
use crate::sync::BootInit;

use core::ffi::CStr;
use core::mem::MaybeUninit;

use alloc::vec::Vec;
use x86_64::PhysAddr;

use core::alloc::Layout;
use core::fmt::Debug;

use x86_64::{
    VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTableFlags, PhysFrame,
        Size1GiB, Size4KiB, Translate,
    },
};

use crate::memory::buddy::{buddy_add_range, buddy_init};
use crate::memory::slub::slub_init;
use crate::memory::{FRAME_LAYOUT, MULTIBOOT_MODULES, MultibootModule};
use crate::memory::{
    KERNEL_STACK_SIZE, KERNEL_STACK_TOP, PHYSICAL_MEMORY_MAX_SIZE, PHYSICAL_MEMORY_OFFSET,
    frame_info::{FrameInfo, frame_info_addr},
    phys_to_mut, phys_to_virt,
};

fn boot_oom() -> ! {
    panic!("out of memory on boot -- shouldn't happen");
}

unsafe impl FrameAllocator<Size4KiB> for BootAlloc {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        unsafe {
            Some(PhysFrame::from_start_address_unchecked(
                self.alloc(FRAME_LAYOUT).unwrap_or_else(|| boot_oom()),
            ))
        }
    }
}

unsafe extern "C" {
    static __text_start: u8;
    static __data_start: u8;
    static __bss_end: u8;
    static __text_start_phys: u8;
    static __data_start_phys: u8;
    static __bss_end_phys: u8;
    static __reclaimable_start_phys: u8;
    static __reclaimable_end_phys: u8;
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
                        .alloc(Layout::from_size_align_unchecked(4096, 4096))
                        .unwrap_or_else(|| boot_oom());
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

            // allocate a new kernel stack
            for addr in (KERNEL_STACK_TOP - KERNEL_STACK_SIZE..KERNEL_STACK_TOP).step_by(4096) {
                self.ensure_allocated(addr);
            }

            // allocate and map pages for FrameInfo structs
            let mut addr = 0;
            while let Some(node) = self.boot_alloc.tree().lower_bound(|s, _| s.start >= addr) {
                let span = *(*node).value();
                if span.span_type.needs_frameinfo() {
                    let fi_start =
                        frame_info_addr(PhysAddr::new_unsafe(span.start).align_down(4096u64))
                            .align_down(4096u64);
                    let fi_end = frame_info_addr(PhysAddr::new_unsafe(span.end).align_up(4096u64))
                        .align_up(4096u64);
                    // might want to be more efficient here somehow
                    for fi_addr in (fi_start..fi_end).step_by(4096) {
                        let phys = self.ensure_allocated(fi_addr);
                        // sometimes does redundant work if things aren't page-aligned but should be OK
                        FrameInfo::new_page(phys_to_virt(phys).as_mut_ptr());
                    }
                }
                addr = span.end;
            }
        }
    }
}

fn get_string<'a>(addr: u32) -> &'a CStr {
    unsafe { CStr::from_ptr(phys_to_virt(PhysAddr::new_unsafe(addr as u64)).as_ptr()) }
}

fn mark_string(boot_alloc: &mut BootAlloc, addr: u32) {
    boot_alloc.mark_reclaimable(addr as u64, get_string(addr).count_bytes() as u64 + 1);
}

fn mark_modules(boot_alloc: &mut BootAlloc, multiboot_info: &MultibootInfoRaw) {
    if multiboot_info.flags & 1 << 3 == 0 {
        return;
    }
    boot_alloc.mark_reclaimable(
        multiboot_info.mods_addr as u64,
        multiboot_info.mods_count as u64 * 16,
    );
    for m in multiboot_info.modules() {
        boot_alloc.mark_used(m.mod_start as u64, (m.mod_end - m.mod_start) as u64);
        mark_string(boot_alloc, m.name);
    }
}

fn grab_modules(multiboot_info: &MultibootInfoRaw) -> Vec<MultibootModule> {
    unsafe {
        multiboot_info
            .modules()
            .iter()
            .map(|m| MultibootModule {
                name: get_string(m.name)
                    .to_str()
                    .expect("invalid utf-8 in module name")
                    .into(),
                contents: core::slice::from_raw_parts(
                    phys_to_virt(PhysAddr::new_unsafe(m.mod_start as u64)).as_ptr(),
                    (m.mod_end - m.mod_start) as usize,
                ),
            })
            .collect()
    }
}

#[unsafe(link_section = ".boot_reclaimable")]
static mut SPANS: [NodeSlot; 128] = [const { MaybeUninit::uninit() }; 128];

pub unsafe fn init(multiboot_info_addr: PhysAddr) -> Reclaimer {
    unsafe {
        let multiboot_info: &MultibootInfoRaw = &*phys_to_virt(multiboot_info_addr).as_ptr();
        #[allow(static_mut_refs)]
        let mut boot_alloc = BootAlloc::new(&mut SPANS);

        // have to be careful not to allocate from boot_alloc until it's populated properly
        for span in multiboot_info.mmaps() {
            boot_alloc.update(span.addr, span.len, |s| {
                if span.is_usable() {
                    s.unwrap_or(SpanType::Free)
                } else {
                    SpanType::Reserved
                }
            });
        }

        boot_alloc.mark_reserved(0, 4096);
        boot_alloc.mark_used(
            &raw const __text_start_phys as u64,
            &raw const __bss_end_phys as u64 - &raw const __text_start_phys as u64,
        );
        boot_alloc.mark_reclaimable(
            multiboot_info_addr.as_u64(),
            core::mem::size_of::<MultibootInfoRaw>() as u64,
        );
        boot_alloc.mark_reclaimable(
            multiboot_info.mmap_addr as u64,
            multiboot_info.mmap_length as u64,
        );
        boot_alloc.mark_reclaimable(
            &raw const __reclaimable_start_phys as u64,
            &raw const __reclaimable_end_phys as u64 - &raw const __reclaimable_start_phys as u64,
        );

        mark_modules(&mut boot_alloc, multiboot_info);

        // now we can allocate using boot_alloc

        let page_table = boot_alloc
            .alloc(Layout::from_size_align_unchecked(4096, 4096))
            .unwrap_or_else(|| boot_oom());
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

        let mut boot_alloc = bootstrap.boot_alloc;

        buddy_init();
        boot_alloc.claim_free_ranges();
        slub_init();

        // we can use the normal allocator now

        BootInit::set(&MULTIBOOT_MODULES, grab_modules(multiboot_info));

        Reclaimer { boot_alloc }
    }
}

pub struct Reclaimer {
    boot_alloc: BootAlloc,
}

impl Drop for Reclaimer {
    fn drop(&mut self) {
        unsafe {
            let mut bytes = 0;
            for range in self.boot_alloc.reclaimable_range_iter() {
                buddy_add_range(range.start, range.end);
                bytes += range.end - range.start;
            }
            println!("reclaimed {} KB", bytes / 1024);
        }
    }
}
