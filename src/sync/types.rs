use std::{rc::Rc, sync::Arc};

use parking_lot::{
    Mutex, MutexGuard as PLMutexGuard, RwLock, RwLockReadGuard as PLRwLockReadGuard,
    RwLockWriteGuard as PLRwLockWriteGuard,
};

pub type RcMutex<T> = Rc<Mutex<T>>;
pub type RcRwLock<T> = Rc<RwLock<T>>;

pub type ArcMutex<T> = Arc<Mutex<T>>;
pub type ArcRwLock<T> = Arc<RwLock<T>>;

pub type MutexGuard<'a, T> = PLMutexGuard<'a, T>;
pub type RwLockReadGuard<'a, T> = PLRwLockReadGuard<'a, T>;
pub type RwLockWriteGuard<'a, T> = PLRwLockWriteGuard<'a, T>;

pub fn rc_mutex_new<T>(object: T) -> RcMutex<T> {
    Rc::new(Mutex::new(object))
}

pub fn rc_rw_lock_new<T>(object: T) -> RcRwLock<T> {
    Rc::new(RwLock::new(object))
}

pub fn arc_mutex_new<T>(object: T) -> ArcMutex<T> {
    Arc::new(Mutex::new(object))
}

pub fn arc_rw_lock_new<T>(object: T) -> ArcRwLock<T> {
    Arc::new(RwLock::new(object))
}
