#[allow(unused_imports)]
use crate::prelude::*;

use core::{alloc::Layout, arch::naked_asm, sync::atomic::Ordering};

use alloc::collections::VecDeque;
use x86_64::VirtAddr;

use crate::{
    define_id,
    id_vec::IdSparseVec,
    interrupts::{PICS, TrapFrame},
    mach::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, mach},
    memory::{FRAME_SIZE, KERNEL_STACK_SIZE, KERNEL_STACK_TOP, kernel_alloc, kernel_free},
    sync::{IrqLock, IrqLockGuard},
};

define_id!(ThreadId);

pub const IDLE_THREAD_ID: ThreadId = ThreadId(0);

#[derive(PartialEq, Eq, Debug)]
pub enum ThreadState {
    Running,
    Ready,
    Exiting,
    Waiting,
}

pub struct SchedThread {
    stack: VirtAddr,
    stack_size: usize,
    regs: TrapFrame,
    state: ThreadState,
}

pub struct Scheduler {
    threads: IdSparseVec<ThreadId, SchedThread>,
    ready: VecDeque<ThreadId>,
}

pub static SCHEDULER: IrqLock<Scheduler> = IrqLock::new(Scheduler::new());

pub const MIN_STACK: usize = 8192;

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            threads: IdSparseVec::new(),
            ready: VecDeque::new(),
        }
    }
    fn spawn_inner(&mut self, fun: fn(*const ()), data: *const (), size: usize, align: usize) {
        let stack_size = 16384;
        let stack = kernel_alloc(Layout::from_size_align(stack_size, FRAME_SIZE).unwrap()).unwrap();
        let mut rsp = (stack + stack_size - size).align_down(align as u64);
        rsp = rsp.align_down(16u64); // SysV ABI requires 16-byte stack alignment
        assert!(rsp >= stack + MIN_STACK);
        unsafe { core::ptr::copy_nonoverlapping(data as *const u8, rsp.as_mut_ptr(), size) };
        let regs = TrapFrame {
            rip: thread_entry_stub as *const () as u64,
            rbx: fun as *const () as u64,
            rsp: rsp.as_u64(),
            rflags: 0x2,
            cs: KERNEL_CODE_SELECTOR.0 as u64,
            ss: KERNEL_DATA_SELECTOR.0 as u64,
            ..TrapFrame::default()
        };
        let id = self.threads.insert(SchedThread {
            stack,
            stack_size,
            regs,
            state: ThreadState::Ready,
        });
        self.ready.push_back(id);
    }
    pub fn spawn<F>(&mut self, body: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let call_body: fn(*const ()) = |ptr| {
            let f = unsafe { core::ptr::read(ptr as *const F) };
            f();
        };
        self.spawn_inner(
            call_body,
            (&raw const body) as *const (),
            core::mem::size_of::<F>(),
            core::mem::align_of::<F>(),
        );
        core::mem::forget(body);
    }
    fn clean_up(&mut self, id: ThreadId) {
        let thread = self.threads.get_mut(id).unwrap();
        assert_eq!(thread.state, ThreadState::Exiting);
        unsafe { kernel_free(thread.stack) };
        self.threads.remove(id);
    }
    fn wake(&mut self, id: ThreadId) {
        let thread = self.threads.get_mut(id).unwrap();
        assert_eq!(thread.state, ThreadState::Waiting);
        thread.state = ThreadState::Ready;
        self.ready.push_back(id);
    }
}

#[unsafe(naked)]
extern "C" fn thread_entry_stub() {
    naked_asm!(
        "mov rdi, rbx",
        "mov rsi, rsp",
        "call {thread_entry}",
        // never returns
        thread_entry = sym thread_entry,
    )
}

#[allow(improper_ctypes_definitions)]
extern "C" fn thread_entry(f: fn(*const ()), aux: *const ()) -> ! {
    unsafe { SCHEDULER.force_unlock() };
    x86_64::instructions::interrupts::enable();
    f(aux);
    thread_exit();
}

#[unsafe(naked)]
unsafe extern "C" fn switch(
    current: *mut TrapFrame,
    next: *const TrapFrame,
    value: *const DeferredAction,
) -> *const DeferredAction {
    naked_asm!(
        "mov rax, rdx",
        "mov [rdi+8*1], rbx",
        "mov [rdi+8*6], rbp",
        "mov [rdi+8*11], r12",
        "mov [rdi+8*12], r13",
        "mov [rdi+8*13], r14",
        "mov [rdi+8*14], r15",
        "pop [rdi+8*17]",
        "mov [rdi+8*20], rsp",
        "pushf",
        "pop [rdi+8*19]",
        "mov rbx, [rsi+8*1]",
        "mov rbp, [rsi+8*6]",
        "mov r12, [rsi+8*11]",
        "mov r13, [rsi+8*12]",
        "mov r14, [rsi+8*13]",
        "mov r15, [rsi+8*14]",
        "push [rsi+8*21]",
        "push [rsi+8*20]",
        "push [rsi+8*19]",
        "push [rsi+8*18]",
        "push [rsi+8*17]",
        "iretq"
    )
}

#[derive(PartialEq, Eq, Debug)]
enum SchedReason {
    Yielding,
    Exiting,
    Waiting,
}

#[derive(Clone, Copy, Debug)]
enum DeferredAction {
    None,
    Cleanup(ThreadId),
}

fn sched(mut scheduler_guard: IrqLockGuard<Scheduler>, reason: SchedReason) -> bool {
    let Scheduler { threads, ready, .. } = &mut *scheduler_guard;
    let current_id = mach().current_thread_id();

    if reason == SchedReason::Yielding && ready.is_empty() {
        return false;
    }
    let next_id = if reason == SchedReason::Exiting {
        IDLE_THREAD_ID
    } else {
        ready.pop_front().unwrap_or(IDLE_THREAD_ID)
    };
    println!("switching from {:?} to {:?}", current_id, next_id);
    assert_ne!(current_id, next_id);

    let (current_thread, next_thread) = threads.get_mut_2(current_id, next_id).unwrap();
    assert_eq!(current_thread.state, ThreadState::Running);

    let deferred_action = match reason {
        SchedReason::Yielding => {
            current_thread.state = ThreadState::Ready;
            if current_id != IDLE_THREAD_ID {
                ready.push_back(current_id);
            }
            DeferredAction::None
        }
        SchedReason::Exiting => {
            current_thread.state = ThreadState::Exiting;
            assert_ne!(current_id, IDLE_THREAD_ID);
            DeferredAction::Cleanup(current_id)
        }
        SchedReason::Waiting => {
            current_thread.state = ThreadState::Waiting;
            DeferredAction::None
        }
    };

    next_thread.state = ThreadState::Running;
    mach().current_thread_id.store(next_id.0, Ordering::Relaxed);

    let deferred_action = unsafe {
        *switch(
            &mut current_thread.regs,
            &next_thread.regs,
            &raw const deferred_action,
        )
    };

    match deferred_action {
        DeferredAction::None => false,
        DeferredAction::Cleanup(thread_id) => {
            assert_eq!(current_id, IDLE_THREAD_ID);
            scheduler_guard.clean_up(thread_id);
            true
        }
    }
}

pub unsafe fn idle_thread() -> ! {
    thread_yield();
    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

pub fn thread_spawn(body: impl FnOnce() + Send + 'static) {
    SCHEDULER.lock().spawn(body)
}

pub fn thread_exit() -> ! {
    sched(SCHEDULER.lock(), SchedReason::Exiting);
    unreachable!();
}

pub fn thread_yield() {
    while sched(SCHEDULER.lock(), SchedReason::Yielding) {}
}

pub unsafe fn init() {
    let id = SCHEDULER.lock().threads.insert(SchedThread {
        stack: KERNEL_STACK_TOP - KERNEL_STACK_SIZE,
        stack_size: KERNEL_STACK_SIZE,
        regs: TrapFrame {
            cs: KERNEL_CODE_SELECTOR.0 as u64,
            ss: KERNEL_DATA_SELECTOR.0 as u64,
            ..TrapFrame::default()
        },
        state: ThreadState::Running,
    });
    assert_eq!(id, IDLE_THREAD_ID);
}

pub unsafe fn timer_interrupt() {
    unsafe { PICS.lock().notify_end_of_interrupt(32) };
    thread_yield();
}

pub struct WaitQueue {
    waiters: IrqLock<VecDeque<ThreadId>>,
}

impl WaitQueue {
    pub const fn new() -> WaitQueue {
        WaitQueue {
            waiters: IrqLock::new(VecDeque::new()),
        }
    }
    pub fn sleep_until<'a, T>(
        &self,
        lock: &'a IrqLock<T>,
        condition: impl Fn(&T) -> bool,
    ) -> IrqLockGuard<'a, T> {
        loop {
            let obj = lock.lock();
            if condition(&obj) {
                return obj;
            }
            let scheduler = SCHEDULER.lock();
            self.waiters.lock().push_back(mach().current_thread_id());
            drop(obj);
            sched(scheduler, SchedReason::Waiting);
        }
    }
    pub fn wake_one(&self) {
        let mut scheduler = SCHEDULER.lock();
        self.waiters.lock().pop_front().map(|id| scheduler.wake(id));
    }
    pub fn wake_all(&self) {
        let mut scheduler = SCHEDULER.lock();
        self.waiters
            .lock()
            .drain(..)
            .for_each(|id| scheduler.wake(id));
    }
}
