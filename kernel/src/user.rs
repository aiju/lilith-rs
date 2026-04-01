use core::{arch::asm, marker::PhantomData, sync::atomic::Ordering};

use alloc::sync::Arc;
use x86_64::{VirtAddr, registers::control::Cr3, structures::paging::PhysFrame};
use xmas_elf::{ElfFile, program::SegmentData};

use crate::{
    mach::{USER_CODE_SELECTOR, mach}, memory::UserAddressSpace, println, sched::thread_stack, sync::{IrqLock, interrupt_guard}
};

pub const USER_STACK_BOTTOM: u64 = 0x0000_7FFF_0000_0000;
pub const USER_STACK_SIZE: usize = 1048576;

pub struct ProcMemory {
    pub address_space: UserAddressSpace,
}

pub struct Proc {
    pub memory: IrqLock<ProcMemory>,
}

impl Proc {
    pub fn new() -> Option<Proc> {
        let address_space = UserAddressSpace::new()?;
        Some(Proc {
            memory: IrqLock::new(ProcMemory { address_space }),
        })
    }
    pub fn activate(self: Arc<Self>) -> ActiveProc {
        let _interrupt_guard = interrupt_guard();
        let mach = mach();
        let old = mach
            .current_proc
            .swap(Arc::into_raw(self.clone()) as *mut Proc, Ordering::Relaxed);
        if !old.is_null() {
            unsafe { Arc::decrement_strong_count(old) };
        }
        unsafe {
            let (old_page_table, flags) = Cr3::read();
            let page_table = self.memory.lock().address_space.page_table_address();
            if old_page_table.start_address() != page_table {
                Cr3::write(PhysFrame::from_start_address_unchecked(page_table), flags);
            }
        }
        ActiveProc {
            proc: self,
            _phantom: PhantomData::default(),
        }
    }
}

pub struct ActiveProc {
    proc: Arc<Proc>,
    _phantom: PhantomData<*const ()>,
}

impl ActiveProc {
    pub fn load_elf(&self, data: &[u8]) -> u64 {
        let proc = &self.proc;
        let elf = ElfFile::new(data).unwrap();
        for h in elf.program_iter() {
            match h.get_type() {
                Ok(xmas_elf::program::Type::Load) => {
                    let va = VirtAddr::new(h.virtual_addr());
                    proc.memory
                        .lock()
                        .address_space
                        .add_mapping(va, h.mem_size() as usize);
                    let Ok(SegmentData::Undefined(data)) = h.get_data(&elf) else {
                        panic!("elf parsing error")
                    };
                    unsafe {
                        let dst = core::slice::from_raw_parts_mut(
                            va.as_mut_ptr(),
                            h.file_size() as usize,
                        );
                        dst.copy_from_slice(data);
                    }
                }
                Ok(_) => {}
                Err(str) => panic!("load_elf: {str}"),
            }
        }
        proc.memory
            .lock()
            .address_space
            .add_mapping(VirtAddr::new(USER_STACK_BOTTOM), USER_STACK_SIZE);
        elf.header.pt2.entry_point()
    }

    /// launch replaces the current kernel thread with a user process
    /// 
    /// in particular the current kernel thread is destroyed and re-used for syscalls and interrupts of that user process
    pub unsafe fn launch(&self, entry: u64) -> ! {
        println!("go to user!!");
        unsafe {
            let interrupt_guard = interrupt_guard();
            let (_, stack_top) = thread_stack();
            mach().set_kernel_rsp(stack_top);
            // let IRETQ re-enable interrupts
            interrupt_guard.drop_without_disabling();
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
                entry = in(reg) entry,
                options(noreturn)
            )
        }
    }
}
