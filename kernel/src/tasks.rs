use core::{
    future::Future,
    pin::Pin,
    task::{Poll, RawWaker},
};

use alloc::{boxed::Box, collections::VecDeque};

use crate::{
    define_id, id_vec::IdSparseVec, mach::mach, sched::{WaitQueue, run_later, thread_spawn}, sync::IrqLock
};

define_id!(TaskId);

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
        EXECUTOR
            .lock()
            .wake(unsafe { core::mem::transmute::<*const (), TaskId>(ptr) })
    },
    |ptr| {
        EXECUTOR
            .lock()
            .wake(unsafe { core::mem::transmute::<*const (), TaskId>(ptr) })
    },
    |_| {},
);

pub struct Executor {
    tasks: IdSparseVec<TaskId, Task>,
    ready: VecDeque<TaskId>,
}

pub static EXECUTOR: IrqLock<Executor> = IrqLock::new(Executor::new());
static EXECUTOR_WAIT: WaitQueue = WaitQueue::new();

impl Executor {
    pub const fn new() -> Self {
        Executor {
            tasks: IdSparseVec::new(),
            ready: VecDeque::new(),
        }
    }
    pub fn spawn(&mut self, f: impl Future<Output = ()> + 'static + Send) {
        let state = TaskState::Ready(Box::pin(f));
        let id = self.tasks.insert(Task { state });
        self.ready.push_back(id);
        EXECUTOR_WAIT.wake_one();
    }
    fn wake(&mut self, id: TaskId) {
        let Some(task) = self.tasks.get_mut(id) else {
            // spurious wake -- ignore
            return;
        };
        match core::mem::replace(&mut task.state, TaskState::ExecutingAndAwoken) {
            TaskState::Executing | TaskState::ExecutingAndAwoken => {}
            TaskState::Paused(future) => {
                task.state = TaskState::Ready(future);
                self.ready.push_back(id);
                EXECUTOR_WAIT.wake_one();
            }
            TaskState::Ready(future) => {
                task.state = TaskState::Ready(future);
            }
        }
    }
}

fn mk_waker(id: TaskId) -> core::task::Waker {
    unsafe {
        core::task::Waker::from_raw(core::task::RawWaker::new(
            core::mem::transmute::<TaskId, *const ()>(id),
            &WAKER_VTABLE,
        ))
    }
}

fn executor_thread() {
    loop {
        let mut executor =
            EXECUTOR_WAIT.sleep_until(&EXECUTOR, |executor| !executor.ready.is_empty());
        let id = executor.ready.pop_front().unwrap();
        let task = executor.tasks.get_mut(id).unwrap();
        let TaskState::Ready(mut future) =
            core::mem::replace(&mut task.state, TaskState::Executing)
        else {
            panic!("task in ready list not actually ready");
        };
        drop(executor);
        let result = future
            .as_mut()
            .poll(&mut core::task::Context::from_waker(&mk_waker(id)));
        let mut executor = EXECUTOR.lock();
        match result {
            Poll::Ready(()) => {
                executor.tasks.remove(id);
            }
            Poll::Pending => {
                let task = executor.tasks.get_mut(id).unwrap();
                match task.state {
                    TaskState::Executing => task.state = TaskState::Paused(future),
                    TaskState::ExecutingAndAwoken => {
                        task.state = TaskState::Ready(future);
                        executor.ready.push_back(id)
                    }
                    TaskState::Paused(_) | TaskState::Ready(_) => panic!("double-pause ?"),
                }
            }
        }
    }
}

pub unsafe fn init() {
    thread_spawn(executor_thread);
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

pub fn task_spawn(body: impl Future<Output = ()> + Send + 'static) {
    EXECUTOR.lock().spawn(body)
}

pub async fn task_yield() {
    YieldFuture::NotYielded.await
}

// TODO: need to be able to cancel sleeps
enum SleepFuture {
    NotYet(u64),
    Yet(u64),
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
        match *self {
            SleepFuture::NotYet(delay_ns) => {
                let waker = cx.waker().clone();
                let target_ticks = run_later(delay_ns, move || waker.wake());
                *self = SleepFuture::Yet(target_ticks);
                Poll::Pending
            }
            SleepFuture::Yet(target_ticks) => {
                if mach().ticks() >= target_ticks {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

pub async fn task_sleep(delay_ns: u64) {
    SleepFuture::NotYet(delay_ns).await
}

