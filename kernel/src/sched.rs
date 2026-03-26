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
