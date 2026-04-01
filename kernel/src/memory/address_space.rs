use alloc::vec::Vec;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator as _, Mapper as _, OffsetPageTable, Page, PageTable, PageTableFlags,
        PhysFrame, Size4KiB, page_table::PageTableEntry,
    },
};

use crate::{
    interrupts::IrqContext, mach::mach, memory::{
        MemoryError, PHYSICAL_MEMORY_OFFSET, alloc_frame, frame_info::FRAME_SIZE, is_user_address, kernel_free, phys_to_virt, virt_to_phys, zero_frame
    }, println, sync::{BootInit, IrqLock}
};

struct FrameAllocator;

unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for FrameAllocator {
    fn allocate_frame(&mut self) -> Option<x86_64::structures::paging::PhysFrame<Size4KiB>> {
        let frame = alloc_frame().ok()?;
        unsafe { Some(PhysFrame::from_start_address_unchecked(frame)) }
    }
}

pub struct KernelAddressSpace {
    page_table: VirtAddr,
}

// TODO: this should be an RwLock
pub static KERNEL_ADDRESS_SPACE: BootInit<IrqLock<KernelAddressSpace>> = unsafe { BootInit::uninit() };

pub unsafe fn init(global_page_table: PhysAddr) {
    unsafe {
        BootInit::set(
            &KERNEL_ADDRESS_SPACE,
            IrqLock::new(KernelAddressSpace {
                page_table: phys_to_virt(global_page_table),
            }),
        );
    }
}

impl KernelAddressSpace {
    fn offset_page_table(&mut self) -> OffsetPageTable<'_> {
        unsafe { OffsetPageTable::new(&mut *self.page_table.as_mut_ptr(), PHYSICAL_MEMORY_OFFSET) }
    }
    pub unsafe fn map_new_page(&mut self, addr: VirtAddr) -> Result<(), MemoryError> {
        unsafe {
            let phys = alloc_frame()?;
            self.offset_page_table()
                .map_to(
                    Page::<Size4KiB>::from_start_address_unchecked(addr),
                    PhysFrame::from_start_address_unchecked(phys),
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::GLOBAL,
                    &mut FrameAllocator,
                )
                .unwrap()
                .flush();
            Ok(())
        }
    }
    pub unsafe fn unmap_page(&mut self, addr: VirtAddr) {
        unsafe {
            let (frame, flush) = self
                .offset_page_table()
                .unmap(Page::<Size4KiB>::from_start_address_unchecked(addr))
                .unwrap();
            flush.flush();
            kernel_free(phys_to_virt(frame.start_address()));
        }
    }
}

pub struct Mapping {
    addr: VirtAddr,
    size: usize,
}

pub struct UserAddressSpace {
    page_table: VirtAddr,
    mappings: Vec<Mapping>,
}

impl UserAddressSpace {
    pub fn new() -> Result<Self, MemoryError> {
        let guard = KERNEL_ADDRESS_SPACE.lock();
        let global: &PageTable = unsafe { &*(*guard).page_table.as_ptr() };
        let addr = phys_to_virt(alloc_frame()?);
        let new: &mut PageTable = unsafe { &mut *addr.as_mut_ptr() };
        for i in 0..256 {
            new[i] = PageTableEntry::new();
        }
        for i in 256..512 {
            new[i] = global[i].clone();
        }
        Ok(UserAddressSpace {
            page_table: addr,
            mappings: Vec::new(),
        })
    }

    pub fn page_table_address(&self) -> PhysAddr {
        virt_to_phys(self.page_table).unwrap()
    }

    pub fn add_mapping(&mut self, addr: VirtAddr, size: usize) {
        self.mappings.push(Mapping { addr, size });
    }

    fn offset_page_table(&mut self) -> OffsetPageTable<'_> {
        unsafe { OffsetPageTable::new(&mut *self.page_table.as_mut_ptr(), PHYSICAL_MEMORY_OFFSET) }
    }
}

impl Drop for UserAddressSpace {
    fn drop(&mut self) {
        // TODO: free page table
    }
}

fn unhandled_fault(ctx: &mut IrqContext, addr: VirtAddr) -> ! {
    println!(
        "Page fault at {:#x}, error code: {:#x}",
        addr,
        ctx.trap_frame().error_code
    );
    println!(
        "RIP {:#x}, RSP {:#x}",
        ctx.trap_frame().rip,
        ctx.trap_frame().rsp
    );
    loop {}
}

pub fn page_fault_handler(ctx: &mut IrqContext, addr: VirtAddr) {
    if is_user_address(addr) {
        if let Some(proc) = mach().current_proc() {
            let mut memory = proc.memory.lock();
            let found = 'found: {
                for mapping in &memory.address_space.mappings {
                    if addr >= mapping.addr && addr - mapping.addr < mapping.size as u64 {
                        break 'found true;
                    }
                }
                false
            };
            if found {
                let frame = FrameAllocator.allocate_frame().unwrap();
                let mut pt = memory.address_space.offset_page_table();
                unsafe {
                    zero_frame(frame.start_address());
                    pt.map_to(
                        Page::from_start_address_unchecked(addr.align_down(FRAME_SIZE as u64)),
                        frame,
                        PageTableFlags::PRESENT
                            | PageTableFlags::WRITABLE
                            | PageTableFlags::USER_ACCESSIBLE,
                        &mut FrameAllocator,
                    )
                    .unwrap()
                    .flush()
                };
                return;
            }
        }
    }
    unhandled_fault(ctx, addr);
}
