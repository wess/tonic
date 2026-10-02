//! Global atom table: append-only, lock-free reads.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::OnceLock;

const SEG_BITS: usize = 12;
const SEG_SIZE: usize = 1 << SEG_BITS;
const NSEG: usize = 1024;

#[derive(Clone, Copy)]
struct Entry {
    name: &'static str,
    /// Index of this atom in the BEAM's atom table (see compiler/data/beam_atoms.txt);
    /// orders atom keys in small maps and feeds the BEAM map hash.
    beam: u32,
}

struct Table {
    segs: [AtomicPtr<Entry>; NSEG],
    next_beam: AtomicU32,
    count: AtomicUsize,
    index: Mutex<HashMap<&'static str, u64>>,
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table {
        segs: [const { AtomicPtr::new(std::ptr::null_mut()) }; NSEG],
        count: AtomicUsize::new(0),
        next_beam: AtomicU32::new(0),
        index: Mutex::new(HashMap::new()),
    })
}

pub fn intern(name: &str) -> u64 {
    intern_with(name, None)
}

/// Intern with a known BEAM atom index (used for the program's static atoms).
pub fn intern_with(name: &str, beam: Option<u32>) -> u64 {
    let t = table();
    let mut idx = t.index.lock();
    if let Some(&i) = idx.get(name) {
        return i;
    }
    let i = t.count.load(Ordering::Relaxed);
    let seg = i >> SEG_BITS;
    assert!(seg < NSEG, "atom table overflow");
    let mut sp = t.segs[seg].load(Ordering::Acquire);
    if sp.is_null() {
        let v: Vec<Entry> = vec![Entry { name: "", beam: 0 }; SEG_SIZE];
        sp = Box::leak(v.into_boxed_slice()).as_mut_ptr();
        t.segs[seg].store(sp, Ordering::Release);
    }
    let s: &'static str = Box::leak(name.to_string().into_boxed_str());
    let beam = match beam {
        Some(b) => {
            t.next_beam.fetch_max(b + 1, Ordering::Relaxed);
            b
        }
        None => t.next_beam.fetch_add(1, Ordering::Relaxed),
    };
    unsafe { *sp.add(i & (SEG_SIZE - 1)) = Entry { name: s, beam } };
    t.count.store(i + 1, Ordering::Release);
    idx.insert(s, i as u64);
    i as u64
}

pub fn lookup(name: &str) -> Option<u64> {
    table().index.lock().get(name).copied()
}

#[inline]
pub fn name(i: u64) -> &'static str {
    let t = table();
    let i = i as usize;
    debug_assert!(i < t.count.load(Ordering::Acquire));
    let sp = t.segs[i >> SEG_BITS].load(Ordering::Acquire);
    unsafe { (*sp.add(i & (SEG_SIZE - 1))).name }
}

#[inline]
pub fn beam_index(i: u64) -> u32 {
    let t = table();
    let i = i as usize;
    let sp = t.segs[i >> SEG_BITS].load(Ordering::Acquire);
    unsafe { (*sp.add(i & (SEG_SIZE - 1))).beam }
}

pub fn count() -> usize {
    table().count.load(Ordering::Acquire)
}
