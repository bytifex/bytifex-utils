use std::sync::{Mutex as StdMutex, RwLock as StdRwLock};
pub use std::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};

use or_die::OrDieWithMsg;

// Mutex
#[derive(Default)]
pub struct Mutex<T: ?Sized>(StdMutex<T>);

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Self(StdMutex::new(value))
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner().or_die_with_msg("mutex poisoned")
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock().or_die_with_msg("mutex poisoned")
    }
}

// RwLock
#[derive(Default)]
pub struct RwLock<T: ?Sized>(StdRwLock<T>);

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        Self(StdRwLock::new(value))
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner().or_die_with_msg("mutex poisoned")
    }
}

impl<T: ?Sized> RwLock<T> {
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        self.0.read().or_die_with_msg("rwlock poisoned")
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.0.write().or_die_with_msg("rwlock poisoned")
    }
}
