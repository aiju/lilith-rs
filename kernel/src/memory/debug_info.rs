use crate::prelude::*;
use arrayvec::ArrayVec;
use x86_64::{
    VirtAddr,
    registers::control::Cr3,
    structures::paging::{PageTable, PageTableFlags},
};

use crate::memory::{PHYSICAL_MEMORY_OFFSET, phys_to_mut};

struct MapRegion {
    virt_start: u64,
    phys_start: u64,
    size: u64,
    flags: PageTableFlags,
}

fn canonicalize(addr: u64) -> u64 {
    if addr & (1 << 47) != 0 {
        addr | 0xFFFF_0000_0000_0000
    } else {
        addr
    }
}

fn mask_flags(flags: PageTableFlags) -> PageTableFlags {
    flags.difference(PageTableFlags::ACCESSED | PageTableFlags::DIRTY)
}

fn collect_mappings(
    table: &PageTable,
    level: i32,
    offset: u64,
    physical_memory_offset: VirtAddr,
    regions: &mut ArrayVec<MapRegion, 512>,
) {
    for (i, entry) in table.iter().enumerate() {
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            continue;
        }
        let va = canonicalize(offset + ((i as u64) << (12 + 9 * (level - 1))));
        if entry.flags().contains(PageTableFlags::HUGE_PAGE) || level == 1 {
            let page_size = 1u64 << (12 + 9 * (level - 1));
            let phys = entry.addr().as_u64();
            let flags = mask_flags(entry.flags());

            // Try to extend the previous region
            if let Some(last) = regions.last_mut() {
                if last.virt_start + last.size == va
                    && last.phys_start + last.size == phys
                    && last.flags == flags
                {
                    last.size += page_size;
                    continue;
                }
            }
            let _ = regions.push(MapRegion {
                virt_start: va,
                phys_start: phys,
                size: page_size,
                flags,
            });
        } else {
            let t: &PageTable =
                unsafe { &*(physical_memory_offset + entry.addr().as_u64()).as_ptr() };
            collect_mappings(t, level - 1, va, physical_memory_offset, regions);
        }
    }
}

fn print_map(table: &PageTable, physical_memory_offset: VirtAddr) {
    let mut regions = ArrayVec::<MapRegion, 512>::new();
    collect_mappings(table, 4, 0, physical_memory_offset, &mut regions);

    for r in &regions {
        let size_label = if r.size >= 1 << 30 {
            (r.size >> 30, "GiB")
        } else if r.size >= 1 << 20 {
            (r.size >> 20, "MiB")
        } else {
            (r.size >> 10, "KiB")
        };

        println!(
            "{:016x}-{:016x} -> {:012x}-{:012x} ({} {}) {:?}",
            r.virt_start,
            r.virt_start + r.size,
            r.phys_start,
            r.phys_start + r.size,
            size_label.0,
            size_label.1,
            r.flags,
        );
    }
}

// Utility function to print the entire page table in a convenient format
#[allow(dead_code)]
pub fn print_memory_map() {
    let (current_table_frame, _) = Cr3::read();
    let page_table: &mut PageTable = unsafe { phys_to_mut(current_table_frame.start_address()) };
    print_map(page_table, PHYSICAL_MEMORY_OFFSET);
}
