use core::{cell::UnsafeCell, mem::MaybeUninit, ops::Deref, sync::atomic::Ordering};

use spin::{Mutex, MutexGuard};

use crate::mach::mach;

#[must_use = "interrupt guard re-enables interrupts when dropped"]
pub struct InterruptGuard(());

impl InterruptGuard {
    pub fn drop_would_reenable(&self) -> bool {
        mach().irq_lock_count.load(Ordering::Relaxed) == 1
    }
    pub unsafe fn drop_without_disabling(self) {
        mach().irq_lock_count.fetch_sub(1, Ordering::Relaxed);
        core::mem::forget(self);
    }
    unsafe fn enter() {
        x86_64::instructions::interrupts::disable();
        mach().irq_lock_count.fetch_add(1, Ordering::Relaxed);
    }
    unsafe fn leave() {
        if mach().irq_lock_count.fetch_sub(1, Ordering::Relaxed) == 1 {
            x86_64::instructions::interrupts::enable();
        }
    }
}

pub fn interrupt_guard() -> InterruptGuard {
    unsafe { InterruptGuard::enter() };
    InterruptGuard(())
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        unsafe { InterruptGuard::leave() };
    }
}

/// this type signifies that something will be initialised on boot and afterwards only modified through interior mutability
///
/// you MUST initialise it with set() or by writing to the pointer returned by as_mut_ptr() during boot.
/// do not use get() before initialising.
pub struct BootInit<T> {
    inner: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T> Sync for BootInit<T> where T: Sync {}

impl<T> BootInit<T> {
    pub const unsafe fn uninit() -> Self {
        BootInit {
            inner: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
    pub unsafe fn set(cell: &Self, value: T) -> &T {
        unsafe { core::ptr::write(cell.inner.get(), MaybeUninit::new(value)) };
        cell
    }
    pub unsafe fn as_mut(cell: &Self) -> &mut T {
        unsafe { (*cell.inner.get()).assume_init_mut() }
    }
}

impl<T> Deref for BootInit<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { (*self.inner.get()).assume_init_ref() }
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
    pub fn lock_with_interrupt_guard(&self, interrupt_guard: InterruptGuard) -> IrqLockGuard<'_, T> {
        let guard = self.inner.lock();
        IrqLockGuard {
            guard,
            interrupt_guard,
        }
    }
    pub unsafe fn force_unlock(&self) {
        unsafe {
            self.inner.force_unlock();
            InterruptGuard::leave();
        };
    }
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
}

impl<T> IrqLockGuard<'_, T> {
    pub fn into_interrupt_guard(guard: Self) -> InterruptGuard {
        guard.interrupt_guard
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
