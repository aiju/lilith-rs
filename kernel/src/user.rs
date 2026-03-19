use core::arch::asm;

use x86_64::VirtAddr;
use xmas_elf::{ElfFile, program::SegmentData};

use crate::{mach::{USER_CODE_SELECTOR, mach}, memory::AddressSpace, println};

pub const KERNEL_STACK_TOP: u64 = 0xFFFF_FFFF_A000_0000;
pub const USER_STACK_BOTTOM: u64 = 0x0000_7FFF_0000_0000;
pub const USER_STACK_SIZE: usize = 1048576;

pub struct Proc {
    address_space: AddressSpace,
    rip: u64,
}

impl Proc {
    pub fn new() -> Option<Proc> {
        let address_space = AddressSpace::new()?;
        Some(Proc {
            address_space,
            rip: 0,
        })
    }
    pub fn load_elf(&mut self, data: &[u8]) {
        let elf = ElfFile::new(data).unwrap();
        for h in elf.program_iter() {
            match h.get_type() {
                Ok(xmas_elf::program::Type::Load) => {
                    let va = VirtAddr::new(h.virtual_addr());
                    unsafe {
                        self.address_space
                            .ensure_allocated(va, h.mem_size() as usize)
                    };
                    let Ok(SegmentData::Undefined(data)) = h.get_data(&elf) else {
                        panic!("elf parsing error")
                    };
                    let mut i = 0;
                    while i < h.file_size() {
                        let p =
                            unsafe { self.address_space.access_via_direct_map(va + i).unwrap() };
                        let n = p.len().min((h.file_size() - i) as usize);
                        p[..n].copy_from_slice(&data[..n]);
                        i += n as u64;
                    }
                }
                Ok(_) => {}
                Err(str) => panic!("load_elf: {str}"),
            }
        }
        unsafe {
            self.address_space
                .ensure_allocated(VirtAddr::new_unsafe(USER_STACK_BOTTOM), USER_STACK_SIZE)
        };
        self.rip = elf.header.pt2.entry_point();
    }
}

pub unsafe fn go_to_userspace(proc: &'static mut Proc) -> ! {
    println!("go to user!!");
    unsafe {
        proc.address_space.activate();
        mach().lock().tss.privilege_stack_table[0] = VirtAddr::new_unsafe(KERNEL_STACK_TOP);
        mach().lock().gs_space_mut().kernel_rsp = KERNEL_STACK_TOP;
        asm!(
            "push {ds_sel}",     // SS
            "push {stack}",      // RSP
            "push 0x200",        // RFLAGS (interrupts enabled)
            "push {cs_sel}",     // CS
            "push {entry}",      // RIP
            "swapgs",
            "iretq",
            ds_sel = in(reg) 0x1bu64,
            cs_sel = in(reg) USER_CODE_SELECTOR.0 as u64,
            stack = in(reg) USER_STACK_BOTTOM + USER_STACK_SIZE as u64,
            entry = in(reg) proc.rip,
            options(noreturn)
        )
    }
}
