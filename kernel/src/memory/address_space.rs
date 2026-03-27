use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator as _, Mapper as _, OffsetPageTable, Page, PageTable, PageTableFlags,
        PhysFrame, Size4KiB, Translate as _, page_table::PageTableEntry,
    },
};

use crate::{
    memory::{
        FRAME_LAYOUT, PHYSICAL_MEMORY_OFFSET, kernel_alloc, phys_to_virt, virt_to_phys,
    },
    sync::IrqLock,
};

static GLOBAL_PAGE_TABLE: IrqLock<VirtAddr> = IrqLock::new(VirtAddr::zero());

pub(super) unsafe fn set_global_page_table_address(addr: PhysAddr) {
    *GLOBAL_PAGE_TABLE.lock() = phys_to_virt(addr);
}

struct FrameAllocator;

unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for FrameAllocator {
    fn allocate_frame(&mut self) -> Option<x86_64::structures::paging::PhysFrame<Size4KiB>> {
        unsafe {
            kernel_alloc(FRAME_LAYOUT)
                .map(|x| PhysFrame::from_start_address_unchecked(virt_to_phys(x).unwrap()))
        }
    }
}

pub struct AddressSpace {
    page_table: VirtAddr,
}

impl AddressSpace {
    pub fn new() -> Option<Self> {
        let guard = GLOBAL_PAGE_TABLE.lock();
        let global: &PageTable = unsafe { &*(*guard).as_ptr() };
        let addr = kernel_alloc(FRAME_LAYOUT)?;
        let new: &mut PageTable = unsafe { &mut *addr.as_mut_ptr() };
        for i in 0..256 {
            new[i] = PageTableEntry::new();
        }
        for i in 256..512 {
            new[i] = global[i].clone();
        }
        Some(AddressSpace { page_table: addr })
    }

    fn offset_page_table(&mut self) -> OffsetPageTable<'_> {
        unsafe { OffsetPageTable::new(&mut *self.page_table.as_mut_ptr(), PHYSICAL_MEMORY_OFFSET) }
    }

    pub unsafe fn ensure_allocated(&mut self, start: VirtAddr, len: usize) {
        assert!(start.as_u64() >> 48 == 0);
        let mut pt = self.offset_page_table();
        let mut page = start.as_u64() & !4095;
        while page < start.as_u64() + (len as u64) {
            let va = unsafe { VirtAddr::new_unsafe(page) };
            if pt.translate_addr(va).is_none() {
                unsafe {
                    pt.map_to(
                        Page::from_start_address_unchecked(va),
                        FrameAllocator.allocate_frame().unwrap(),
                        PageTableFlags::PRESENT
                            | PageTableFlags::WRITABLE
                            | PageTableFlags::USER_ACCESSIBLE,
                        &mut FrameAllocator,
                    )
                    .unwrap()
                    .flush()
                };
            }
            page += 4096;
        }
    }

    pub fn access_via_direct_map(&mut self, addr: VirtAddr) -> Option<&'static mut [u8]> {
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

    pub fn activate(&mut self) {
        unsafe {
            let flags = Cr3::read().1;
            Cr3::write(
                PhysFrame::from_start_address_unchecked(virt_to_phys(self.page_table).unwrap()),
                flags,
            );
        }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        // TODO: free page table
    }
}
