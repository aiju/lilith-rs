use core::mem::ManuallyDrop;

use spin::{Mutex, MutexGuard};

pub struct IrqLock<T> {
    inner: Mutex<T>,
}

pub struct IrqLockGuard<'a, T> {
    guard: ManuallyDrop<MutexGuard<'a, T>>,
    interrupts_were_enabled: bool,
}

impl<T> IrqLock<T> {
    pub const fn new(inner: T) -> Self {
        IrqLock {
            inner: Mutex::new(inner),
        }
    }
    pub fn lock(&self) -> IrqLockGuard<'_, T> {
        let interrupts_were_enabled = x86_64::instructions::interrupts::are_enabled();
        x86_64::instructions::interrupts::disable();
        let guard = ManuallyDrop::new(self.inner.lock());
        IrqLockGuard {
            guard,
            interrupts_were_enabled,
        }
    }
}

impl<T> Drop for IrqLockGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.guard) };
        if self.interrupts_were_enabled {
            x86_64::instructions::interrupts::enable();
        }
    }
}

impl<T> core::ops::Deref for IrqLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> core::ops::DerefMut for IrqLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}
