#[allow(unused_imports)]
use crate::prelude::*;

use core::{arch::naked_asm, sync::atomic::Ordering};

use alloc::{boxed::Box, collections::VecDeque, sync::Arc};
use x86_64::VirtAddr;

use crate::{
    define_id,
    id_vec::IdSparseVec,
    interrupts::{IrqContext, PICS, TICK_NS},
    mach::mach,
    memory::Stack,
    sync::{IrqLock, IrqLockGuard},
    user::Proc,
};

// used in sched() to block access of stale references
macro_rules! yeet_references {
    ($($ref:expr),+ $(,)?) => {
        #[allow(forgetting_references)]
        {
            $(core::mem::forget($ref);)+
        }
    };
}

define_id!(ThreadId);

pub const IDLE_THREAD_ID: ThreadId = ThreadId(0);

#[derive(PartialEq, Eq, Debug)]
pub enum ThreadState {
    Running,
    Ready,
    Exiting,
    Waiting,
    Sleeping,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
#[repr(C)]
struct SwitchFrame {
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rip: u64,
    rsp: u64,
}

pub struct SchedThread {
    stack: Stack,
    regs: SwitchFrame,
    proc: Option<Arc<Proc>>,
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
    pub fn current_thread(&self) -> &SchedThread {
        self.threads.get(mach().current_thread_id()).unwrap()
    }
    pub fn current_thread_mut(&mut self) -> &mut SchedThread {
        self.threads.get_mut(mach().current_thread_id()).unwrap()
    }
    fn spawn_inner(&mut self, fun: fn(*const ()), data: *const (), size: usize, align: usize) {
        let stack = Stack::new().unwrap();
        let mut rsp = (stack.top() - size).align_down(align as u64);
        rsp = rsp.align_down(16u64); // SysV ABI requires 16-byte stack alignment
        assert!(rsp >= stack.bottom() + MIN_STACK);
        unsafe { core::ptr::copy_nonoverlapping(data as *const u8, rsp.as_mut_ptr(), size) };
        let regs = SwitchFrame {
            rip: thread_entry_stub as *const () as u64,
            rbx: fun as *const () as u64,
            rsp: rsp.as_u64(),
            ..SwitchFrame::default()
        };
        let id = self.threads.insert(SchedThread {
            stack,
            regs,
            proc: None,
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
        self.threads.remove(id);
    }
    fn wake(&mut self, id: ThreadId) {
        let thread = self.threads.get_mut(id).unwrap();
        assert!(thread.state == ThreadState::Waiting || thread.state == ThreadState::Sleeping);
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
        // call not jmp to make the stack look normal, esp wrt alignment, never actually returns
        thread_entry = sym thread_entry,
    )
}

#[allow(improper_ctypes_definitions)]
extern "C" fn thread_entry(f: fn(*const ()), aux: *const ()) -> ! {
    // we enter this after the switch() in sched() which means we're holding the scheduler lock
    // but since the actual lock guard isn't transferred across we have to force_unlock
    unsafe { SCHEDULER.force_unlock() };

    f(aux);
    thread_exit();
}

#[unsafe(naked)]
unsafe extern "C" fn switch(
    current: *mut SwitchFrame,
    next: *const SwitchFrame,
    value: *const DeferredAction,
) -> *const DeferredAction {
    naked_asm!(
        // move deferred action pointer into the return value
        "mov rax, rdx",
        // save callee-save registers
        "mov [rdi+8*0], rbx",
        "mov [rdi+8*1], rbp",
        "mov [rdi+8*2], r12",
        "mov [rdi+8*3], r13",
        "mov [rdi+8*4], r14",
        "mov [rdi+8*5], r15",
        // pop return address into rip field
        "pop [rdi+8*6]",
        // save RSP, leave stack alone after this
        "mov [rdi+8*7], rsp",
        // load callee-save registers
        "mov rbx, [rsi+8*0]",
        "mov rbp, [rsi+8*1]",
        "mov r12, [rsi+8*2]",
        "mov r13, [rsi+8*3]",
        "mov r14, [rsi+8*4]",
        "mov r15, [rsi+8*5]",
        // load RSP
        "mov rsp, [rsi+8*7]",
        // push new return address and RET
        "push [rsi+8*6]",
        "ret",
    )
}

#[derive(PartialEq, Eq, Debug)]
pub enum SchedReason {
    Yielding,
    Exiting,
    Waiting,
    Sleeping,
}

#[derive(Clone, Copy, Debug)]
enum DeferredAction {
    None,
    Cleanup(ThreadId),
}

pub fn sched(
    mut scheduler_guard: IrqLockGuard<Scheduler>,
    reason: SchedReason,
) -> IrqLockGuard<Scheduler> {
    // we might need to loop around if we hit a deferred action
    loop {
        assert_eq!(
            mach().irq_lock_count.load(Ordering::Relaxed),
            1,
            "sched() called in invalid context (locks held or non-blocking interrupt context). irq_lock_count == {}",
            mach().irq_lock_count.load(Ordering::Relaxed)
        );

        let Scheduler { threads, ready, .. } = &mut *scheduler_guard;
        let current_id = mach().current_thread_id();

        if reason == SchedReason::Yielding && ready.is_empty() {
            return scheduler_guard;
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

        // important invariant: we only ever pass DeferredActions to the idle thread
        // this is because other threads might be new and thread_entry() doesn't handle DeferredActions

        let deferred_action = match reason {
            SchedReason::Yielding => {
                current_thread.state = ThreadState::Ready;
                if current_id != IDLE_THREAD_ID {
                    ready.push_back(current_id);
                }
                DeferredAction::None
            }
            SchedReason::Exiting => {
                // we can't clean up the current thread while still on its stack
                // so we have to switch to the idle thread, run a deferred action and then loop around
                //
                // TODO: would this be cleaner if we manually swapped the stacks here instead of the DeferredAction mechanism?

                current_thread.state = ThreadState::Exiting;
                assert_ne!(current_id, IDLE_THREAD_ID);
                debug_assert_eq!(next_id, IDLE_THREAD_ID);
                DeferredAction::Cleanup(current_id)
            }
            SchedReason::Waiting => {
                current_thread.state = ThreadState::Waiting;
                DeferredAction::None
            }
            SchedReason::Sleeping => {
                current_thread.state = ThreadState::Sleeping;
                DeferredAction::None
            }
        };

        next_thread.state = ThreadState::Running;
        mach().current_thread_id.store(next_id.0, Ordering::Relaxed);

        current_thread.proc = mach().current_proc();
        next_thread.proc.as_ref().map(|x| x.clone().activate());

        // the saved RSP in kernel TSS has to point at the top of the kernel stack in case the new thread wants to go to usermode
        unsafe { mach().set_kernel_rsp(next_thread.stack.top()) };

        // switch to the new thread
        // we pass the deferred action by reading it from the previous stack
        // (always safe even if exiting since the stack is sure to still exist at this point)
        let deferred_action = unsafe {
            *switch(
                &mut current_thread.regs,
                &next_thread.regs,
                &raw const deferred_action,
            )
        };

        // we're now in the new thread
        // all local variables hold values they had before the new thread called sched
        // so be very careful accessing local variables after this point
        // do not access any references e.g. current_thread, next_thread
        yeet_references!(current_thread, next_thread);

        match deferred_action {
            DeferredAction::None => {}
            DeferredAction::Cleanup(thread_id) => {
                assert_eq!(mach().current_thread_id(), IDLE_THREAD_ID);
                scheduler_guard.clean_up(thread_id);
                continue;
            }
        }

        return scheduler_guard;
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
    sched(SCHEDULER.lock(), SchedReason::Yielding);
}

pub unsafe fn init(kernel_stack: Stack) {
    let id = SCHEDULER.lock().threads.insert(SchedThread {
        stack: kernel_stack,
        proc: None,
        regs: SwitchFrame::default(),
        state: ThreadState::Running,
    });
    assert_eq!(id, IDLE_THREAD_ID);
}

type TimerClosure = Box<dyn FnOnce() + Send + 'static>;

struct TimerEvent {
    delta: u64,
    closure: TimerClosure,
}

pub struct TimerQueue {
    timers: VecDeque<TimerEvent>,
}

static TIMER_QUEUE: IrqLock<TimerQueue> = IrqLock::new(TimerQueue::new());

impl TimerQueue {
    pub const fn new() -> TimerQueue {
        TimerQueue {
            timers: VecDeque::new(),
        }
    }
    pub fn insert(&mut self, mut ticks: u64, closure: TimerClosure) {
        for (i, t) in self.timers.iter_mut().enumerate() {
            if ticks < t.delta {
                t.delta -= ticks;
                self.timers.insert(
                    i,
                    TimerEvent {
                        delta: ticks,
                        closure,
                    },
                );
                return;
            } else {
                ticks -= t.delta;
            }
        }
        self.timers.push_back(TimerEvent {
            delta: ticks,
            closure,
        });
    }
    fn tick(&mut self) {
        if !self.timers.is_empty() {
            if self.timers[0].delta > 0 {
                self.timers[0].delta -= 1;
            }
        }
    }
    fn pop_closure(&mut self) -> Option<TimerClosure> {
        self.timers
            .pop_front_if(|t| t.delta == 0)
            .map(|t| t.closure)
    }
}

pub fn run_later(delay_ns: u64, closure: impl FnOnce() + Send + 'static) -> u64 {
    let ticks = delay_ns.div_ceil(TICK_NS);
    let mut timer_queue = TIMER_QUEUE.lock();
    let target_ticks = mach().ticks() + ticks;
    timer_queue.insert(ticks, Box::new(closure));
    target_ticks
}

pub fn thread_sleep(delay_ns: u64) {
    let scheduler = SCHEDULER.lock();
    let id = mach().current_thread_id();
    run_later(delay_ns, move || SCHEDULER.lock().wake(id));
    sched(scheduler, SchedReason::Sleeping);
}

pub fn timer_interrupt(ctx: &mut IrqContext) {
    mach().ticks.fetch_add(1, Ordering::Relaxed);
    TIMER_QUEUE.lock().tick();
    while let Some(closure) = { TIMER_QUEUE.lock().pop_closure() } {
        closure();
    }
    ctx.need_resched();
    unsafe { PICS.lock().notify_end_of_interrupt(32) };
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

// TODO: feels like we can return some kind of magic reference here ?
// e.g. return StackRef that's not Send or something
pub fn thread_stack() -> (VirtAddr, VirtAddr) {
    let scheduler = SCHEDULER.lock();
    let thread = scheduler.threads.get(mach().current_thread_id()).unwrap();
    (thread.stack.bottom(), thread.stack.top())
}
