use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
};

use spin::{Mutex, MutexGuard};

#[must_use = "interrupt guard re-enables interrupts when dropped"]
pub struct InterruptGuard {
    were_enabled: bool,
}

pub fn interrupt_guard() -> InterruptGuard {
    let were_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();
    InterruptGuard { were_enabled }
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        if self.were_enabled {
            x86_64::instructions::interrupts::enable();
        }
    }
}

/// this type signifies that something will be initialised on boot and afterwards only modified through interior mutability
///
/// you MUST initialise it with set() or by writing to the pointer returned by as_mut_ptr() during boot.
/// do not use get() before initialising.
pub struct BootInit<T> {
    inner: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T> Sync for BootInit<T> {}

impl<T> BootInit<T> {
    pub const unsafe fn uninit() -> Self {
        BootInit {
            inner: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
    pub fn get(&self) -> &T {
        unsafe { (*self.inner.get()).assume_init_ref() }
    }
    pub unsafe fn set(&self, value: T) -> &T {
        unsafe { core::ptr::write(self.inner.get(), MaybeUninit::new(value)) };
        self.get()
    }
    pub unsafe fn as_mut_ptr(&self) -> *mut T {
        unsafe { MaybeUninit::as_mut_ptr(self.inner.get().as_mut_unchecked()) }
    }
}

pub struct IrqLock<T> {
    inner: Mutex<T>,
}

pub struct IrqLockGuard<'a, T> {
    // guard needs to be first for drop order reasons!!
    guard: MutexGuard<'a, T>,
    #[allow(dead_code)]
    interrupt_guard: InterruptGuard,
}

impl<T> IrqLock<T> {
    pub const fn new(inner: T) -> Self {
        IrqLock {
            inner: Mutex::new(inner),
        }
    }
    pub fn lock(&self) -> IrqLockGuard<'_, T> {
        let interrupt_guard = interrupt_guard();
        let guard = self.inner.lock();
        IrqLockGuard {
            guard,
            interrupt_guard,
        }
    }
    pub unsafe fn force_unlock(&self) {
        unsafe { self.inner.force_unlock() };
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
