#[cfg(not(target_arch = "wasm32"))]
pub mod sync_parking_lot;
pub mod sync_std;

use std::{rc::Rc, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
pub use sync_parking_lot::*;

#[cfg(target_arch = "wasm32")]
pub use sync_std::*;

pub type RcMutex<T> = Rc<Mutex<T>>;
pub type RcRwLock<T> = Rc<RwLock<T>>;

pub type ArcMutex<T> = Arc<Mutex<T>>;
pub type ArcRwLock<T> = Arc<RwLock<T>>;

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
