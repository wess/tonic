//! Native state owned by reference terms, copied with their heap wrappers.

use crate::term::*;
use parking_lot::Mutex;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

pub(crate) struct Resource {
    id: u64,
    value: Arc<dyn Any + Send + Sync>,
}

pub fn is_managed(t: Term) -> bool {
    is_ref(t) && size_of_hdr(hdr(t)) == 2
}

fn registry() -> &'static Mutex<HashMap<u64, Vec<Weak<Resource>>>> {
    static REGISTRY: std::sync::OnceLock<Mutex<HashMap<u64, Vec<Weak<Resource>>>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Drop for Resource {
    fn drop(&mut self) {
        let mut registry = registry().lock();
        if let Some(refs) = registry.get_mut(&self.id) {
            refs.retain(|r| r.strong_count() != 0);
            if refs.is_empty() {
                registry.remove(&self.id);
            }
        }
    }
}

pub(crate) fn find(id: u64) -> Option<Arc<Resource>> {
    registry().lock().get(&id)?.iter().find_map(Weak::upgrade)
}

pub(crate) unsafe fn write_existing(p: *mut u64, state: Arc<Resource>) {
    *p = header(T_REF, 2);
    *p.add(1) = state.id;
    *p.add(2) = Arc::into_raw(state) as u64;
}

pub unsafe fn write<T: Any + Send + Sync>(p: *mut u64, id: u64, value: T) {
    let state = Arc::new(Resource {
        id,
        value: Arc::new(value),
    });
    registry()
        .lock()
        .entry(id)
        .or_default()
        .push(Arc::downgrade(&state));
    write_existing(p, state);
}

pub fn get<T: Any + Send + Sync>(t: Term) -> Option<Arc<T>> {
    if !is_managed(t) {
        return None;
    }
    let state = unsafe { &*(word(t, 2) as *const Resource) };
    state.value.clone().downcast().ok()
}

pub fn downgrade(t: Term) -> Option<Weak<dyn Any + Send + Sync>> {
    if !is_managed(t) {
        return None;
    }
    let state = unsafe { &*(word(t, 2) as *const Resource) };
    Some(Arc::downgrade(&state.value))
}

pub unsafe fn retain(t: Term) {
    if is_managed(t) {
        Arc::increment_strong_count(word(t, 2) as *const Resource);
    }
}

pub unsafe fn release(t: Term) {
    Arc::decrement_strong_count(word(t, 2) as *const Resource);
}

pub unsafe fn collect(mut start: *mut u64, end: *mut u64) -> Vec<Term> {
    let mut refs = Vec::new();
    while start < end {
        if is_managed(start as Term) {
            refs.push(start as Term);
        }
        start = start.add(1 + (size_of_hdr(*start) as usize).max(1));
    }
    refs
}
