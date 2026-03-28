#[allow(unused_imports)]
use crate::prelude::*;

/*
use core::{
    future::Future,
    pin::Pin,
    task::{Poll, RawWaker},
};

use alloc::{boxed::Box, collections::VecDeque, vec::Vec};
use spin::mutex::Mutex;

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct TaskId(usize);

enum TaskState {
    Executing,
    ExecutingAndAwoken,
    Ready(Pin<Box<dyn Future<Output = ()> + Send>>),
    Paused(Pin<Box<dyn Future<Output = ()> + Send>>),
}

pub struct Task {
    state: TaskState,
}

static WAKER_VTABLE: core::task::RawWakerVTable = core::task::RawWakerVTable::new(
    |ptr| RawWaker::new(ptr, &WAKER_VTABLE),
    |ptr| {
        SCHEDULER
            .lock()
            .wake(unsafe { core::mem::transmute::<*const (), TaskId>(ptr) })
    },
    |ptr| {
        SCHEDULER
            .lock()
            .wake(unsafe { core::mem::transmute::<*const (), TaskId>(ptr) })
    },
    |_| {},
);

pub struct Scheduler {
    tasks: Vec<Option<Task>>,
    ready: VecDeque<TaskId>,
    free: VecDeque<TaskId>,
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

fn get_task(tasks: &mut Vec<Option<Task>>, id: TaskId) -> &mut Task {
    tasks[id.0].as_mut().expect("invalid task id")
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            tasks: Vec::new(),
            ready: VecDeque::new(),
            free: VecDeque::new(),
        }
    }
    pub fn spawn(&mut self, f: impl Future<Output = ()> + 'static + Send) {
        let state = TaskState::Ready(Box::pin(f));
        let id = self.alloc(Task { state });
        self.ready.push_back(id);
    }
    fn wake(&mut self, id: TaskId) {
        let task = get_task(&mut self.tasks, id);
        match core::mem::replace(&mut task.state, TaskState::ExecutingAndAwoken) {
            TaskState::Executing | TaskState::ExecutingAndAwoken => {}
            TaskState::Paused(future) => {
                task.state = TaskState::Ready(future);
                self.ready.push_back(id);
            }
            TaskState::Ready(future) => {
                task.state = TaskState::Ready(future);
            }
        }
    }
    fn alloc(&mut self, t: Task) -> TaskId {
        if let Some(id) = self.free.pop_front() {
            let result = self.tasks[id.0].replace(t);
            assert!(result.is_none());
            id
        } else {
            let id = TaskId(self.tasks.len());
            self.tasks.push(Some(t));
            id
        }
    }
    fn delete(&mut self, id: TaskId) {
        let task: &mut Option<Task> = &mut self.tasks[id.0];
        match task.take() {
            None => panic!("task double-delete ?"),
            Some(_) => {
                self.free.push_back(id);
            }
        }
    }
    pub fn sched() {
        loop {
            let mut guard = SCHEDULER.lock();
            let Some(id) = guard.ready.pop_front() else {
                break;
            };
            let task = get_task(&mut guard.tasks, id);
            let TaskState::Ready(mut future) =
                core::mem::replace(&mut task.state, TaskState::Executing)
            else {
                panic!("task in ready list not actually ready");
            };
            drop(guard);
            let waker = unsafe {
                core::task::Waker::from_raw(core::task::RawWaker::new(
                    core::mem::transmute::<TaskId, *const ()>(id),
                    &WAKER_VTABLE,
                ))
            };
            let mut context = core::task::Context::from_waker(&waker);
            let result = future.as_mut().poll(&mut context);
            let mut guard = SCHEDULER.lock();
            match result {
                Poll::Ready(()) => {
                    guard.delete(id);
                }
                Poll::Pending => {
                    let task = get_task(&mut guard.tasks, id);
                    match task.state {
                        TaskState::Executing => task.state = TaskState::Paused(future),
                        TaskState::ExecutingAndAwoken => {
                            task.state = TaskState::Ready(future);
                            guard.ready.push_back(id)
                        }
                        TaskState::Paused(_) | TaskState::Ready(_) => panic!("double-pause ?"),
                    }
                }
            }
        }
    }
}

enum YieldFuture {
    NotYielded,
    Yielded,
}

impl Future for YieldFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
        match core::mem::replace(&mut *self, YieldFuture::Yielded) {
            YieldFuture::NotYielded => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            YieldFuture::Yielded => Poll::Ready(()),
        }
    }
}

pub async fn yield_now() {
    YieldFuture::NotYielded.await
}
*/

use core::{alloc::Layout, arch::naked_asm, cell::UnsafeCell, sync::atomic::Ordering};

use alloc::{collections::VecDeque, vec::Vec};
use x86_64::VirtAddr;

use crate::{
    define_id,
    id_vec::IdSparseVec,
    interrupts::{PICS, TrapFrame},
    mach::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, mach},
    memory::{KERNEL_STACK_SIZE, KERNEL_STACK_TOP, kernel_alloc, kernel_free},
    sync::{IrqLock, interrupt_guard},
};

define_id!(ThreadId);

pub const IDLE_THREAD_ID: ThreadId = ThreadId(0);

#[derive(PartialEq, Eq, Debug)]
pub enum ThreadState {
    Running,
    Ready,
    Exiting,
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

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            threads: IdSparseVec::new(),
            ready: VecDeque::new(),
        }
    }
    pub fn spawn(&mut self, f: fn()) {
        let stack_size = 16384;
        let stack = kernel_alloc(Layout::from_size_align(stack_size, 4096).unwrap()).unwrap();
        let rsp = stack + stack_size;
        let regs = TrapFrame {
            rip: thread_entry_stub as *const () as u64,
            rbx: f as *const () as u64,
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
    fn clean_up(&mut self, id: ThreadId) {
        println!("cleaning up {id:?}");
        let thread = self.threads.get_mut(id).unwrap();
        assert_eq!(thread.state, ThreadState::Exiting);
        unsafe { kernel_free(thread.stack) };
        self.threads.remove(id);
    }
}

#[unsafe(naked)]
extern "C" fn thread_entry_stub() {
    naked_asm!(
        "mov rdi, rbx",
        "jmp {thread_entry}",
        thread_entry = sym thread_entry,
    )
}

#[allow(improper_ctypes_definitions)]
extern "C" fn thread_entry(f: fn()) -> ! {
    unsafe { SCHEDULER.force_unlock() };
    x86_64::instructions::interrupts::enable();
    f();
    thread_exit();
}

/* pub struct TrapFrame {
    pub rax: u64,   0
    pub rbx: u64,   1
    pub rcx: u64,   2
    pub rdx: u64,  3
    pub rsi: u64, 4
    pub rdi: u64, 5
    pub rbp: u64, 6
    pub r8: u64, 7
    pub r9: u64, 8
    pub r10: u64, 9
    pub r11: u64, 10
    pub r12: u64, 11
    pub r13: u64, 12
    pub r14: u64, 13
    pub r15: u64, 14
    pub int_num: u64, 15
    pub error_code: u64, 16
    pub rip: u64, 17
    pub cs: u64, 18
    pub rflags: u64, 19
    pub rsp: u64, 20
    pub ss: u64, 21
}
    */
/*
#[unsafe(naked)]
extern "C" fn switch(current: *const TrapFrame) {
    naked_asm!(
        "mov rax, [rdi+8*0]",
        "mov rbx, [rdi+8*1]",
        "mov rcx, [rdi+8*2]",
        "mov rdx, [rdi+8*3]",
        "mov rsi, [rdi+8*4]",
        "mov rbp, [rdi+8*6]",
        "mov r8, [rdi+8*7]",
        "mov r9, [rdi+8*8]",
        "mov r10, [rdi+8*9]",
        "mov r11, [rdi+8*10]",
        "mov r12, [rdi+8*11]",
        "mov r13, [rdi+8*12]",
        "mov r14, [rdi+8*13]",
        "mov r15, [rdi+8*14]",
        "push qword [rdi+8*21]",
        "push qword [rdi+8*20]",
        "push qword [rdi+8*19]",
        "push qword [rdi+8*18]",
        "push qword [rdi+8*17]",
        "mov rdi, [rdi+8*5]",
        "iretq"
    )
}*/

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
}

#[derive(Clone, Copy, Debug)]
enum DeferredAction {
    None,
    Cleanup(ThreadId),
}

fn sched(reason: SchedReason) -> bool {
    print!("sched({reason:?}");
    let mut guard = SCHEDULER.lock();
    println!(")");
    let Scheduler { threads, ready, .. } = &mut *guard;
    let current_id = ThreadId(mach().current_thread_id.load(Ordering::Relaxed));

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
            guard.clean_up(thread_id);
            true
        }
    }
}

pub unsafe fn idle_thread() -> ! {
    thread_yield();
    loop {
            println!("enter hlt");
            x86_64::instructions::interrupts::enable_and_hlt();
            println!("exit hlt");
    }
}

pub fn thread_exit() -> ! {
    sched(SchedReason::Exiting);
    unreachable!();
}

pub fn thread_yield() {
    while sched(SchedReason::Yielding) {
    }
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
    println!("timer");
    unsafe { PICS.lock().notify_end_of_interrupt(32) };
    thread_yield();
}
