//! Per-process heaps and the generational copying garbage collector.
//!
//! Every Elixir process owns a private heap in two generations, as on the
//! BEAM. New objects go to a small young heap. A minor collection copies the
//! young heap's survivors: objects that already survived one collection
//! (below the high-water mark) are promoted to the old heap, the rest stay
//! young. A major collection copies everything into a fresh old heap; it runs
//! only when the old heap has no room for the next promotions. Long-lived data
//! is therefore copied once per major collection instead of on every
//! collection.
//!
//! Minor collections never scan the old heap. That is sound because terms are
//! immutable: an object only points to objects created before it, so nothing
//! old points into the young heap. Promotion after two survivals keeps
//! objects the runtime is still filling in young. TONIC_GC_VERIFY=1 checks the
//! invariant after every minor collection.
//!
//! Collection is a Cheney copy driven from precise roots:
//!
//! * the shadow-stack frame chain (every live Elixir value in compiled code
//!   lives in a frame slot, so moving objects is safe),
//! * the runtime root stack (`Ctx::roots`) used by native BIFs,
//! * process-local state: pending exception, mailbox save queue, dictionary.
//!
//! Heaps are never shared between processes (messages are copied), so each
//! process collects independently without stopping anyone else.

use crate::term::*;
use std::collections::VecDeque;

/// Initial heap of a process, in words: the BEAM's default min_heap_size.
pub const MIN_HEAP_WORDS: usize = 233;

#[repr(C)]
pub struct Ctx {
    // ---- fields read/written by compiled code (offsets are ABI) ----
    pub frame: *mut u64, // 0
    pub hp: *mut u64,    // 8
    pub hend: *mut u64,  // 16
    pub reds: i64,       // 24
    /// Allocation limit for compiled code's inline bump allocation: `hend`,
    /// or the heap start under GC stress so every allocation reaches the
    /// runtime (and collects).
    pub hlim: *mut u64, // 32
    /// Lowest stack address compiled code may enter with (see stackseg);
    /// null where the stack cannot grow.
    pub slim: *mut u8, // 40
    // ---- runtime private ----
    /// Stack segments kept for reuse after deep recursion returns.
    pub segs: Vec<corosensei::stack::DefaultStack>,
    pub heap_start: *mut u64,
    pub heap_words: usize,
    pub heap_buf: Vec<u64>,
    pub min_heap: usize,
    /// Shadow-stack words scanned by the last collection.
    pub last_stack_words: usize,
    /// Words that survived the last collection (heap plus half the stack).
    pub last_live: usize,
    /// Old generation: promoted objects, bump-allocated by minor collections.
    pub old_buf: Vec<u64>,
    pub old_start: *mut u64,
    pub old_words: usize,
    pub old_top: *mut u64,
    /// Young objects below this address survived a collection already; the
    /// next minor collection promotes them.
    pub hwm: *mut u64,
    /// Size the young heap returns to after a collection, adapted to how much
    /// survives.
    pub young_target: usize,
    /// Old-heap words live after the last major collection.
    pub last_major_live: usize,
    pub roots: Vec<Term>,
    pub exc_kind: Term,
    pub exc_reason: Term,
    pub exc_stack: Term,
    /// Set when an untrappable exit is being processed; the process unwinds.
    pub kill_reason: Term,
    /// Messages already moved onto this heap but not yet consumed by a receive.
    pub saveq: VecDeque<Term>,
    pub recv_cursor: usize,
    pub recv_deadline: Option<std::time::Instant>,
    pub recv_started: bool,
    pub dict: Term,
    pub gc_count: u64,
    pub spawn_fun: Term,
    pub exit_reason: Term,
    pub shared: *const crate::sched::ProcShared,
    pub yielder: *const std::ffi::c_void,
    pub ttl_scratch: Vec<u8>,
    /// Shadow frame current when the last exception was raised.
    pub raise_frame: *mut u64,
    /// Per-argument badarg descriptions waiting for the BIF's stack frame.
    pub pending_args: Term,
    offheap: Vec<Term>,
}

pub const REDUCTIONS: i64 = 4000;

/// TONIC_GC_STRESS=1 collects on every allocation (debugging aid).
#[inline]
pub fn gc_stress() -> bool {
    use std::sync::atomic::{AtomicU8, Ordering};
    static S: AtomicU8 = AtomicU8::new(2);
    match S.load(Ordering::Relaxed) {
        0 => false,
        1 => true,
        _ => {
            let on = std::env::var("TONIC_GC_STRESS").map(|v| v == "1").unwrap_or(false);
            S.store(on as u8, Ordering::Relaxed);
            on
        }
    }
}

impl Ctx {
    pub fn new(min_heap: usize) -> Box<Ctx> {
        let mut heap_buf: Vec<u64> = Vec::with_capacity(min_heap);
        let start = heap_buf.as_mut_ptr();
        unsafe { heap_buf.set_len(min_heap) };
        Box::new(Ctx {
            frame: std::ptr::null_mut(),
            hp: start,
            hend: unsafe { start.add(min_heap) },
            hlim: if gc_stress() { start } else { unsafe { start.add(min_heap) } },
            slim: std::ptr::null_mut(),
            segs: Vec::new(),
            reds: REDUCTIONS,
            heap_start: start,
            heap_words: min_heap,
            heap_buf,
            min_heap,
            last_stack_words: 0,
            last_live: 0,
            old_buf: Vec::new(),
            old_start: std::ptr::null_mut(),
            old_words: 0,
            old_top: std::ptr::null_mut(),
            hwm: start,
            young_target: min_heap,
            last_major_live: 0,
            roots: Vec::with_capacity(64),
            exc_kind: 0,
            exc_reason: 0,
            exc_stack: 0,
            kill_reason: 0,
            saveq: VecDeque::new(),
            recv_cursor: 0,
            recv_deadline: None,
            recv_started: false,
            dict: 0,
            gc_count: 0,
            spawn_fun: 0,
            exit_reason: 0,
            shared: std::ptr::null(),
            yielder: std::ptr::null(),
            ttl_scratch: Vec::new(),
            raise_frame: std::ptr::null_mut(),
            pending_args: 0,
            offheap: Vec::new(),
        })
    }

    #[inline(always)]
    pub fn avail(&self) -> usize {
        (self.hend as usize - self.hp as usize) / 8
    }

    /// Allocate `words` words; may trigger a collection.
    #[inline]
    pub fn alloc(&mut self, words: usize) -> *mut u64 {
        if self.avail() < words || gc_stress() {
            self.gc(words);
        }
        let p = self.hp;
        self.hp = unsafe { p.add(words) };
        p
    }

    /// Allocate without collecting. Caller must have reserved.
    #[inline(always)]
    pub fn alloc_nogc(&mut self, words: usize) -> *mut u64 {
        debug_assert!(self.avail() >= words, "alloc_nogc without reserve");
        let p = self.hp;
        self.hp = unsafe { p.add(words) };
        p
    }

    /// Make sure `words` can be allocated without a collection.
    #[inline]
    pub fn reserve(&mut self, words: usize) {
        if self.avail() < words || gc_stress() {
            self.gc(words);
        }
    }

    /// Reserve while keeping `ts` alive (and updated if they move).
    #[inline]
    pub fn reserve_with(&mut self, words: usize, ts: &mut [Term]) {
        if self.avail() < words || gc_stress() {
            let base = self.roots.len();
            self.roots.extend_from_slice(ts);
            self.gc(words);
            ts.copy_from_slice(&self.roots[base..base + ts.len()]);
            self.roots.truncate(base);
        }
    }

    #[inline]
    pub fn in_young(&self, a: usize) -> bool {
        a >= self.heap_start as usize && a < self.heap_start as usize + self.heap_words * 8
    }

    #[inline]
    pub fn in_old(&self, a: usize) -> bool {
        a >= self.old_start as usize && a < self.old_start as usize + self.old_words * 8
    }

    #[inline]
    pub fn in_heap(&self, p: *const u64) -> bool {
        self.in_young(p as usize) || self.in_old(p as usize)
    }

    /// Words in use in both generations.
    pub fn used_words(&self) -> usize {
        let young = (self.hp as usize - self.heap_start as usize) / 8;
        let old = if self.old_start.is_null() { 0 } else { (self.old_top as usize - self.old_start as usize) / 8 };
        young + old
    }

    pub fn gc(&mut self, need: usize) {
        self.gc_count += 1;
        // promotion takes the aged objects and anything they point to: at most
        // the whole young heap
        let promote = (self.hp as usize - self.heap_start as usize) / 8;
        let old_free = if self.old_start.is_null() {
            0
        } else {
            (self.old_start as usize + self.old_words * 8 - self.old_top as usize) / 8
        };
        // under stress, exercise major collections too
        let stress_major = gc_stress() && self.gc_count % 4 == 0;
        if promote > old_free || stress_major {
            self.major(need);
        } else {
            self.minor(need);
        }
        let live = self.used_words() + self.last_stack_words / 2;
        self.last_live = live;
    }

    /// Young survivors stay young once, then move to the old heap.
    fn minor(&mut self, need: usize) {
        let survivors_bound = (self.hp as usize - self.hwm as usize) / 8;
        let to_words = (survivors_bound + need + 64).max(self.young_target);
        let mut to: Vec<u64> = Vec::with_capacity(to_words);
        let to_start = to.as_mut_ptr();
        unsafe { to.set_len(to_words) };
        let young_lo = self.heap_start as usize;
        let mut gc = Cheney {
            young_lo,
            young_hi: young_lo + self.heap_words * 8,
            old_lo: 0,
            old_hi: 0,
            hwm: self.hwm as usize,
            major: false,
            young_free: to_start,
            old_free: self.old_top,
        };
        let old_scan = self.old_top;
        unsafe {
            self.evac_roots(&mut gc);
            gc.scan(to_start, old_scan);
        }
        let young_used = (gc.young_free as usize - to_start as usize) / 8;
        self.sweep_resources(false);
        self.old_top = gc.old_free;
        self.set_young(to, to_start, to_words, young_used);
        // next young size: room for four times what survived
        self.young_target = (young_used * 4 + need).max(self.min_heap);
        if std::env::var_os("TONIC_GC_VERIFY").is_some() {
            self.verify_generations();
        }
    }

    /// Everything live, young and old, into a fresh old heap.
    fn major(&mut self, need: usize) {
        let old_used = if self.old_start.is_null() { 0 } else { (self.old_top as usize - self.old_start as usize) / 8 };
        let young_used = (self.hp as usize - self.heap_start as usize) / 8;
        // room for everything that could survive, and for the old heap to grow
        // to twice its live size so promotions don't force the next major soon
        let old_words = (old_used + young_used).max(self.last_major_live * 2) + self.young_target * 2 + 64;
        let mut old: Vec<u64> = Vec::with_capacity(old_words);
        let old_start = old.as_mut_ptr();
        unsafe { old.set_len(old_words) };
        let young_lo = self.heap_start as usize;
        let mut gc = Cheney {
            young_lo,
            young_hi: young_lo + self.heap_words * 8,
            old_lo: self.old_start as usize,
            old_hi: self.old_start as usize + self.old_words * 8,
            hwm: 0,
            major: true,
            young_free: std::ptr::null_mut(),
            old_free: old_start,
        };
        unsafe {
            self.evac_roots(&mut gc);
            gc.scan(std::ptr::null_mut(), old_start);
        }
        self.sweep_resources(true);
        self.old_buf = old;
        self.old_start = old_start;
        self.old_words = old_words;
        self.old_top = gc.old_free;
        self.last_major_live = (gc.old_free as usize - old_start as usize) / 8;
        // the young heap starts over empty
        let young_words = (self.young_target + need + 64).max(self.min_heap);
        let mut young: Vec<u64> = Vec::with_capacity(young_words);
        let young_start = young.as_mut_ptr();
        unsafe { young.set_len(young_words) };
        self.set_young(young, young_start, young_words, 0);
    }

    fn sweep_resources(&mut self, major: bool) {
        let old_lo = self.old_start as usize;
        let old_hi = self.old_top as usize;
        self.offheap.retain_mut(|t| unsafe {
            if !major && (*t as usize) >= old_lo && (*t as usize) < old_hi {
                return true;
            }
            if boxed_tag(*t) == T_FORWARD {
                *t = word(*t, 1);
                true
            } else {
                crate::resource::release(*t);
                false
            }
        });
    }

    pub fn managed_ref<T: std::any::Any + Send + Sync>(&mut self, value: T) -> Term {
        self.managed_ref_id(crate::sched::next_ref_id(), value)
    }

    fn managed_ref_id<T: std::any::Any + Send + Sync>(&mut self, id: u64, value: T) -> Term {
        let p = self.alloc(3);
        unsafe { crate::resource::write(p, id, value) };
        self.offheap.push(p as Term);
        p as Term
    }

    pub fn ref_from_id(&mut self, id: u64) -> Term {
        if let Some(state) = crate::resource::find(id) {
            let p = self.alloc(3);
            unsafe { crate::resource::write_existing(p, state) };
            self.offheap.push(p as Term);
            return p as Term;
        }
        if let Some(resource) = crate::nif::resource_from_id(id) {
            return self.managed_ref_id(id, resource);
        }
        let p = self.alloc(2);
        unsafe {
            *p = header(T_REF, 1);
            *p.add(1) = id;
        }
        p as Term
    }

    fn set_young(&mut self, buf: Vec<u64>, start: *mut u64, words: usize, used: usize) {
        self.heap_buf = buf;
        self.heap_start = start;
        self.heap_words = words;
        self.hp = unsafe { start.add(used) };
        // everything now young survived a collection
        self.hwm = self.hp;
        self.hend = unsafe { start.add(words) };
        self.hlim = if gc_stress() { start } else { self.hend };
    }

    unsafe fn evac_roots(&mut self, gc: &mut Cheney) {
        // Frame chain.
        let mut f = self.frame;
        let mut stack_words = 0usize;
        while !f.is_null() {
            let n = *f.add(1) as usize;
            stack_words += n + 4;
            for i in 0..n {
                let s = f.add(4 + i);
                *s = gc.evac(*s);
            }
            f = *f as *mut u64;
        }
        self.last_stack_words = stack_words;
        for r in self.roots.iter_mut() {
            *r = gc.evac(*r);
        }
        self.exc_kind = gc.evac(self.exc_kind);
        self.exc_reason = gc.evac(self.exc_reason);
        self.exc_stack = gc.evac(self.exc_stack);
        self.pending_args = gc.evac(self.pending_args);
        self.kill_reason = gc.evac(self.kill_reason);
        self.dict = gc.evac(self.dict);
        for m in self.saveq.iter_mut() {
            *m = gc.evac(*m);
        }
        self.spawn_fun = gc.evac(self.spawn_fun);
        self.exit_reason = gc.evac(self.exit_reason);
    }

    /// TONIC_GC_VERIFY: no old object may point into the young heap.
    fn verify_generations(&self) {
        if self.old_start.is_null() {
            return;
        }
        let mut p = self.old_start;
        while p < self.old_top {
            unsafe {
                let h = *p;
                let n = size_of_hdr(h) as usize;
                let range: Option<(usize, usize)> = match tag_of_hdr(h) {
                    T_TUPLE | T_MAPNODE | T_HAMT => Some((1, n)),
                    T_CONS => Some((1, 2)),
                    T_MAP => Some((2, 2)),
                    T_CLOSURE => Some((4, n)),
                    T_SUBBIN => Some((1, 1)),
                    _ => None,
                };
                if let Some((lo, hi)) = range {
                    for i in lo..=hi {
                        let v = *p.add(i);
                        if v & 7 == 0 && v != 0 && self.in_young(v as usize) {
                            eprintln!("tonic: GC invariant violated: old object {:p} (tag {}) points into the young heap", p, tag_of_hdr(h));
                            std::process::abort();
                        }
                    }
                }
                p = p.add(1 + n.max(1));
            }
        }
    }

    // ---------- root helpers ----------
    #[inline]
    pub fn push(&mut self, t: Term) -> usize {
        self.roots.push(t);
        self.roots.len() - 1
    }
    #[inline]
    pub fn get(&self, i: usize) -> Term {
        self.roots[i]
    }
    #[inline]
    pub fn set(&mut self, i: usize, t: Term) {
        self.roots[i] = t
    }
    #[inline]
    pub fn truncate(&mut self, n: usize) {
        self.roots.truncate(n)
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        for t in self.offheap.drain(..) {
            unsafe { crate::resource::release(t) };
        }
    }
}

/// One collection's copying state: what moves (the young heap, and in a major
/// collection the old heap too) and where it goes.
struct Cheney {
    young_lo: usize,
    young_hi: usize,
    old_lo: usize,
    old_hi: usize,
    /// Young objects below this survived before and are promoted (minor).
    hwm: usize,
    major: bool,
    young_free: *mut u64,
    old_free: *mut u64,
}

impl Cheney {
    #[inline(always)]
    unsafe fn evac(&mut self, t: u64) -> u64 {
        self.evac_to(t, false)
    }

    /// `force_old`: the referring object is in the old heap, so whatever it
    /// points to is promoted with it; the old heap then never points into the
    /// young heap, even for an object the runtime finished filling after it
    /// last survived a collection.
    #[inline(always)]
    unsafe fn evac_to(&mut self, t: u64, force_old: bool) -> u64 {
        if t & 7 != 0 || t == 0 {
            return t;
        }
        let a = t as usize;
        let young = a >= self.young_lo && a < self.young_hi;
        let old = self.major && a >= self.old_lo && a < self.old_hi;
        if !young && !old {
            return t; // old (in a minor collection), static literal or foreign
        }
        let p = t as *mut u64;
        let h = *p;
        if tag_of_hdr(h) == T_FORWARD {
            return *p.add(1);
        }
        let n = 1 + (size_of_hdr(h) as usize).max(1);
        let to_old = self.major || force_old || a < self.hwm;
        let dst = if to_old { self.old_free } else { self.young_free };
        std::ptr::copy_nonoverlapping(p, dst, n);
        if to_old {
            self.old_free = dst.add(n);
        } else {
            self.young_free = dst.add(n);
        }
        *p = T_FORWARD;
        *p.add(1) = dst as u64;
        dst as u64
    }

    /// Scan copied objects in both destinations until no new copies appear.
    unsafe fn scan(&mut self, mut young: *mut u64, mut old: *mut u64) {
        loop {
            let mut moved = false;
            while !young.is_null() && young < self.young_free {
                young = self.scan_object(young, false);
                moved = true;
            }
            while old < self.old_free {
                old = self.scan_object(old, true);
                moved = true;
            }
            if !moved {
                break;
            }
        }
    }

    #[inline(always)]
    unsafe fn scan_object(&mut self, scan: *mut u64, old: bool) -> *mut u64 {
        let h = *scan;
        let n = size_of_hdr(h) as usize;
        match tag_of_hdr(h) {
            T_TUPLE | T_MAPNODE | T_HAMT => {
                for i in 1..=n {
                    *scan.add(i) = self.evac_to(*scan.add(i), old);
                }
            }
            T_CONS => {
                *scan.add(1) = self.evac_to(*scan.add(1), old);
                *scan.add(2) = self.evac_to(*scan.add(2), old);
            }
            T_MAP => {
                *scan.add(2) = self.evac_to(*scan.add(2), old);
            }
            T_CLOSURE => {
                for i in 4..=n {
                    *scan.add(i) = self.evac_to(*scan.add(i), old);
                }
            }
            T_SUBBIN => {
                *scan.add(1) = self.evac_to(*scan.add(1), old);
            }
            _ => {}
        }
        scan.add(1 + n.max(1))
    }
}

/// Appends producing at least this many bytes go to a writable buffer.
const APPEND_MIN: usize = 64;

/// The buffer `x` can be extended in place by `extra` bytes: `x` views all of
/// it and it has room.
fn writable_base(x: Term, extra: usize) -> Option<Term> {
    if boxed_tag(x) != T_SUBBIN || word(x, 2) != 0 {
        return None;
    }
    let base = word(x, 1);
    if boxed_tag(base) != T_WBUF {
        return None;
    }
    let used = word(base, 1) as usize;
    let cap = (size_of_hdr(hdr(base)) as usize - 1) * 8;
    (word(x, 3) as usize == used && cap - used >= extra).then_some(base)
}

/// Size in words of the boxed object `t` (including header and padding).
#[inline]
pub fn obj_words(t: Term) -> usize {
    1 + (size_of_hdr(hdr(t)) as usize).max(1)
}

// ---------------------------------------------------------------------------
// Builders. All builders that take terms assume those terms are either rooted
// by the caller or that the builder reserves first.
// ---------------------------------------------------------------------------

#[repr(C, align(8))]
pub struct StaticEmptyTuple {
    pub h: u64,
    pub pad: u64,
}
pub static EMPTY_TUPLE: StaticEmptyTuple = StaticEmptyTuple {
    h: header(T_TUPLE, 0),
    pad: 0,
};
#[repr(C, align(8))]
pub struct StaticEmptyBin {
    pub h: u64,
    pub len: u64,
}
pub static EMPTY_BIN: StaticEmptyBin = StaticEmptyBin {
    h: header(T_BINARY, 1),
    len: 0,
};

pub fn empty_tuple() -> Term {
    &EMPTY_TUPLE as *const _ as u64
}
pub fn empty_bin() -> Term {
    &EMPTY_BIN as *const _ as u64
}

impl Ctx {
    pub fn tuple(&mut self, elems: &[Term]) -> Term {
        let n = elems.len();
        if n == 0 {
            return empty_tuple();
        }
        let mut tmp: smallvec_like::Buf = smallvec_like::Buf::from(elems);
        self.reserve_with(n + 1, tmp.as_mut());
        let p = self.alloc_nogc(n + 1);
        unsafe {
            *p = header(T_TUPLE, n as u64);
            for (i, e) in tmp.as_ref().iter().enumerate() {
                *p.add(1 + i) = *e;
            }
        }
        p as Term
    }

    /// Tuple allocation where the caller fills the elements; no GC may happen
    /// between this call and the fill.
    pub fn tuple_uninit_nogc(&mut self, n: usize) -> *mut u64 {
        let p = self.alloc_nogc(n + 1);
        unsafe { *p = header(T_TUPLE, n as u64) };
        p
    }

    pub fn cons(&mut self, h: Term, t: Term) -> Term {
        let mut r = [h, t];
        self.reserve_with(3, &mut r);
        self.cons_nogc(r[0], r[1])
    }

    #[inline]
    pub fn cons_nogc(&mut self, h: Term, t: Term) -> Term {
        let p = self.alloc_nogc(3);
        unsafe {
            *p = header(T_CONS, 2);
            *p.add(1) = h;
            *p.add(2) = t;
        }
        p as Term
    }

    pub fn float(&mut self, f: f64) -> Term {
        let p = self.alloc(2);
        unsafe {
            *p = header(T_FLOAT, 1);
            *p.add(1) = f.to_bits();
        }
        p as Term
    }

    pub fn float_nogc(&mut self, f: f64) -> Term {
        let p = self.alloc_nogc(2);
        unsafe {
            *p = header(T_FLOAT, 1);
            *p.add(1) = f.to_bits();
        }
        p as Term
    }

    /// Allocate a binary of `len` bytes and return (term, data pointer).
    pub fn bin_alloc(&mut self, len: usize) -> (Term, *mut u8) {
        let words = 2 + (len + 7) / 8;
        let p = self.alloc(words);
        unsafe {
            *p = header(T_BINARY, (words - 1) as u64);
            *p.add(1) = len as u64;
            if len % 8 != 0 {
                *p.add(words - 1) = 0;
            }
            (p as Term, p.add(2) as *mut u8)
        }
    }

    pub fn bin_alloc_nogc(&mut self, len: usize) -> (Term, *mut u8) {
        let words = 2 + (len + 7) / 8;
        let p = self.alloc_nogc(words);
        unsafe {
            *p = header(T_BINARY, (words - 1) as u64);
            *p.add(1) = len as u64;
            if len % 8 != 0 {
                *p.add(words - 1) = 0;
            }
            (p as Term, p.add(2) as *mut u8)
        }
    }

    pub fn binary(&mut self, bytes: &[u8]) -> Term {
        if bytes.is_empty() {
            return empty_bin();
        }
        let (t, d) = self.bin_alloc(bytes.len());
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), d, bytes.len()) };
        t
    }

    /// A bitstring of `nbits` bits taken from `bytes` (MSB-first); a plain
    /// binary when `nbits` is a multiple of 8.
    pub fn bitstring(&mut self, bytes: &[u8], nbits: usize) -> Term {
        if nbits % 8 == 0 {
            return self.binary(&bytes[..nbits / 8]);
        }
        let nb = (nbits + 7) / 8;
        let words = 2 + (nb + 7) / 8;
        let p = self.alloc(words);
        unsafe {
            *p = header(T_BITS, (words - 1) as u64);
            *p.add(1) = nbits as u64;
            *p.add(words - 1) = 0;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(2) as *mut u8, nb);
            // Clear unused low bits of the last byte so equal bitstrings are bytewise equal.
            let last = (p.add(2) as *mut u8).add(nb - 1);
            *last &= 0xffu8 << (8 - nbits % 8);
        }
        p as Term
    }

    pub fn str(&mut self, s: &str) -> Term {
        self.binary(s.as_bytes())
    }

    /// Sub-binary of `b` (rooted by caller through `r`), never copying.
    pub fn sub_binary(&mut self, b: Term, off: usize, len: usize) -> Term {
        if len == 0 {
            return empty_bin();
        }
        let (base, boff) = if boxed_tag(b) == T_SUBBIN {
            (word(b, 1), word(b, 2) as usize)
        } else {
            (b, 0)
        };
        // a writable buffer is never handed out: it may grow under later views
        if boff == 0 && off == 0 && boxed_tag(base) == T_BINARY && len == bin_len(base) {
            return base;
        }
        // Small slices are cheaper as copies (and don't pin large bases).
        if len <= 16 {
            let mut r = [b];
            self.reserve_with(2 + 2, &mut r);
            let bytes = bin_bytes(r[0]);
            let (t, d) = self.bin_alloc_nogc(len);
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr().add(off), d, len) };
            return t;
        }
        let mut r = [base];
        self.reserve_with(4, &mut r);
        let p = self.alloc_nogc(4);
        unsafe {
            *p = header(T_SUBBIN, 3);
            *p.add(1) = r[0];
            *p.add(2) = (boff + off) as u64;
            *p.add(3) = len as u64;
        }
        p as Term
    }

    /// `x <> y`. Appending to the newest view of a writable buffer writes in
    /// place, so building a binary by repeated appends is linear, as with the
    /// BEAM's append optimization. Views never change: a view covers at most
    /// the bytes written when it was made, and writes only go past them.
    pub fn bin_append(&mut self, x: Term, y: Term) -> Term {
        let (lx, ly) = (bin_len(x), bin_len(y));
        if lx == 0 {
            return y;
        }
        if ly == 0 {
            return x;
        }
        let mut r = [x, y];
        if let Some(base) = writable_base(x, ly) {
            let _ = base;
            self.reserve_with(4, &mut r);
            let base = word(r[0], 1);
            unsafe {
                let used = word(base, 1) as usize;
                let data = (base as *mut u64).add(2) as *mut u8;
                std::ptr::copy(bin_bytes(r[1]).as_ptr(), data.add(used), ly);
                *(base as *mut u64).add(1) = (used + ly) as u64;
                return self.subbin_nogc(base, used + ly);
            }
        }
        let total = lx + ly;
        if total < APPEND_MIN {
            self.reserve_with(2 + (total + 7) / 8, &mut r);
            let (t, d) = self.bin_alloc_nogc(total);
            unsafe {
                std::ptr::copy_nonoverlapping(bin_bytes(r[0]).as_ptr(), d, lx);
                std::ptr::copy_nonoverlapping(bin_bytes(r[1]).as_ptr(), d.add(lx), ly);
            }
            return t;
        }
        let cap = (total * 2).max(256);
        self.reserve_with(2 + cap / 8 + 4, &mut r);
        let (base, d) = self.wbuf_alloc_nogc(cap, total);
        unsafe {
            std::ptr::copy_nonoverlapping(bin_bytes(r[0]).as_ptr(), d, lx);
            std::ptr::copy_nonoverlapping(bin_bytes(r[1]).as_ptr(), d.add(lx), ly);
            self.subbin_nogc(base, total)
        }
    }

    /// A writable buffer with `cap` bytes of room, `used` of them in use.
    fn wbuf_alloc_nogc(&mut self, cap: usize, used: usize) -> (Term, *mut u8) {
        let words = 2 + (cap + 7) / 8;
        let p = self.alloc_nogc(words);
        unsafe {
            *p = header(T_WBUF, (words - 1) as u64);
            *p.add(1) = used as u64;
            (p as Term, p.add(2) as *mut u8)
        }
    }

    unsafe fn subbin_nogc(&mut self, base: Term, len: usize) -> Term {
        let p = self.alloc_nogc(4);
        *p = header(T_SUBBIN, 3);
        *p.add(1) = base;
        *p.add(2) = 0;
        *p.add(3) = len as u64;
        p as Term
    }

    pub fn make_ref(&mut self) -> Term {
        let id = crate::sched::next_ref_id();
        let p = self.alloc(2);
        unsafe {
            *p = header(T_REF, 1);
            *p.add(1) = id;
        }
        p as Term
    }

    /// Build a proper list from terms held in the root stack range [base, base+n).
    pub fn list_from_roots(&mut self, base: usize, n: usize, tail: Term) -> Term {
        let mut t = [tail];
        self.reserve_with(3 * n, &mut t);
        let mut acc = t[0];
        for i in (0..n).rev() {
            let e = self.roots[base + i];
            acc = self.cons_nogc(e, acc);
        }
        acc
    }

    /// Build a list from a Vec of terms that are already rooted elsewhere
    /// (i.e. immediates or static); reserves first.
    pub fn list_from_vec(&mut self, v: &mut Vec<Term>) -> Term {
        let base = self.roots.len();
        self.roots.extend_from_slice(v);
        let r = self.list_from_roots(base, v.len(), NIL_LIST);
        self.roots.truncate(base);
        r
    }
}

/// Tiny inline buffer to avoid heap allocation for small root sets.
pub mod smallvec_like {
    pub enum Buf {
        Inline([u64; 8], usize),
        Heap(Vec<u64>),
    }
    impl Buf {
        pub fn from(s: &[u64]) -> Buf {
            if s.len() <= 8 {
                let mut a = [0u64; 8];
                a[..s.len()].copy_from_slice(s);
                Buf::Inline(a, s.len())
            } else {
                Buf::Heap(s.to_vec())
            }
        }
        pub fn as_mut(&mut self) -> &mut [u64] {
            match self {
                Buf::Inline(a, n) => &mut a[..*n],
                Buf::Heap(v) => v.as_mut_slice(),
            }
        }
        pub fn as_ref(&self) -> &[u64] {
            match self {
                Buf::Inline(a, n) => &a[..*n],
                Buf::Heap(v) => v.as_slice(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Deep copy between heaps (message passing).
// ---------------------------------------------------------------------------

/// A self-contained copy of a term living outside any process heap.
pub struct Fragment {
    pub buf: Vec<u64>,
    pub root: Term,
    offheap: Vec<Term>,
}
unsafe impl Send for Fragment {}

impl Fragment {
    pub fn new(buf: Vec<u64>, root: Term, end: *mut u64) -> Fragment {
        let offheap = unsafe { crate::resource::collect(buf.as_ptr() as *mut u64, end) };
        Fragment { buf, root, offheap }
    }
}

impl Drop for Fragment {
    fn drop(&mut self) {
        for t in self.offheap.drain(..) {
            unsafe { crate::resource::release(t) };
        }
    }
}

/// Words needed to copy `t` out of the region [lo, hi).
pub fn copy_size(t: Term, lo: usize, hi: usize) -> usize {
    copy_size_by(t, &|a| a >= lo && a < hi)
}

/// Words needed to copy the objects of `t` for which `inside` holds.
pub fn copy_size_by<F: Fn(usize) -> bool>(t: Term, inside: &F) -> usize {
    let mut total = 0usize;
    let mut stack: Vec<Term> = vec![t];
    while let Some(mut t) = stack.pop() {
        loop {
            if !is_ptr(t) || !inside(t as usize) {
                break;
            }
            let h = hdr(t);
            let n = size_of_hdr(h) as usize;
            match tag_of_hdr(h) {
                T_TUPLE => {
                    total += 1 + n.max(1);
                    for i in 1..=n {
                        stack.push(word(t, i));
                    }
                    break;
                }
                T_CONS => {
                    total += 3;
                    stack.push(word(t, 1));
                    t = word(t, 2);
                    continue;
                }
                T_MAP => {
                    total += 3;
                    t = word(t, 2);
                    continue;
                }
                T_MAPNODE | T_HAMT => {
                    total += 1 + n;
                    for i in 1..=n {
                        stack.push(word(t, i));
                    }
                    break;
                }
                T_CLOSURE => {
                    total += 1 + n;
                    for i in 4..=n {
                        stack.push(word(t, i));
                    }
                    break;
                }
                T_SUBBIN => {
                    // Copied as a fresh flat binary.
                    let len = word(t, 3) as usize;
                    total += 2 + (len + 7) / 8;
                    break;
                }
                _ => {
                    total += 1 + n.max(1);
                    break;
                }
            }
        }
    }
    total
}

/// Copy `t` (objects in [lo,hi)) into memory starting at `*free`; returns new term.
/// Caller guarantees enough room (see `copy_size`).
pub unsafe fn copy_term(t: Term, lo: usize, hi: usize, free: &mut *mut u64) -> Term {
    copy_term_by(t, &|a| a >= lo && a < hi, free)
}

/// Copy the objects of `t` for which `inside` holds (see `copy_size_by`).
pub unsafe fn copy_term_by<F: Fn(usize) -> bool>(t: Term, inside: &F, free: &mut *mut u64) -> Term {
    if !is_ptr(t) || !inside(t as usize) {
        return t;
    }
    let h = hdr(t);
    let n = size_of_hdr(h) as usize;
    match tag_of_hdr(h) {
        T_CONS => {
            // Iterative over the spine.
            let first = *free;
            let mut cur = t;
            let mut prev: *mut u64 = std::ptr::null_mut();
            loop {
                let cell = *free;
                *free = cell.add(3);
                *cell = header(T_CONS, 2);
                if !prev.is_null() {
                    *prev.add(2) = cell as u64;
                }
                let hd = word(cur, 1);
                *cell.add(1) = copy_term_by(hd, inside, free);
                let tl = word(cur, 2);
                prev = cell;
                if is_ptr(tl) && inside(tl as usize) && boxed_tag(tl) == T_CONS {
                    cur = tl;
                    continue;
                }
                *cell.add(2) = copy_term_by(tl, inside, free);
                break;
            }
            first as u64
        }
        T_TUPLE => {
            let dst = *free;
            *free = dst.add(1 + n.max(1));
            *dst = h;
            if n == 0 {
                *dst.add(1) = 0;
            }
            for i in 1..=n {
                *dst.add(i) = copy_term_by(word(t, i), inside, free);
            }
            dst as u64
        }
        T_MAP => {
            let dst = *free;
            *free = dst.add(3);
            *dst = h;
            *dst.add(1) = word(t, 1);
            *dst.add(2) = copy_term_by(word(t, 2), inside, free);
            dst as u64
        }
        T_MAPNODE | T_HAMT => {
            let dst = *free;
            *free = dst.add(1 + n);
            *dst = h;
            for i in 1..=n {
                *dst.add(i) = copy_term_by(word(t, i), inside, free);
            }
            dst as u64
        }
        T_CLOSURE => {
            let dst = *free;
            *free = dst.add(1 + n);
            *dst = h;
            *dst.add(1) = word(t, 1);
            *dst.add(2) = word(t, 2);
            *dst.add(3) = word(t, 3);
            for i in 4..=n {
                *dst.add(i) = copy_term_by(word(t, i), inside, free);
            }
            dst as u64
        }
        T_SUBBIN => {
            let bytes = bin_bytes(t);
            let len = bytes.len();
            let words = 2 + (len + 7) / 8;
            let dst = *free;
            *free = dst.add(words);
            *dst = header(T_BINARY, (words - 1) as u64);
            *dst.add(1) = len as u64;
            *dst.add(words - 1) = 0;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.add(2) as *mut u8, len);
            dst as u64
        }
        _ => {
            let words = 1 + n.max(1);
            let dst = *free;
            *free = dst.add(words);
            std::ptr::copy_nonoverlapping(ptr(t), dst, words);
            crate::resource::retain(dst as Term);
            dst as u64
        }
    }
}

impl Ctx {
    /// Copy a term out of this process heap into a standalone fragment.
    pub fn to_fragment(&self, t: Term) -> Fragment {
        let inside = |a: usize| self.in_young(a) || self.in_old(a);
        let words = copy_size_by(t, &inside);
        let mut buf: Vec<u64> = Vec::with_capacity(words.max(1));
        unsafe { buf.set_len(words.max(1)) };
        let mut free = buf.as_mut_ptr();
        let root = unsafe { copy_term_by(t, &inside, &mut free) };
        Fragment::new(buf, root, free)
    }

    /// Copy the objects of `t` for which `inside` holds into this heap.
    pub fn copy_in_by<F: Fn(usize) -> bool>(&mut self, t: Term, inside: &F) -> Term {
        let words = copy_size_by(t, inside);
        self.reserve(words);
        let mut free = self.hp;
        let r = unsafe { copy_term_by(t, inside, &mut free) };
        self.offheap.extend(unsafe { crate::resource::collect(self.hp, free) });
        self.hp = free;
        r
    }

    /// Copy a fragment's term into this heap.
    pub fn from_fragment(&mut self, f: &Fragment) -> Term {
        let lo = f.buf.as_ptr() as usize;
        let hi = lo + f.buf.len() * 8;
        let words = copy_size(f.root, lo, hi);
        self.reserve(words);
        let mut free = self.hp;
        let r = unsafe { copy_term(f.root, lo, hi, &mut free) };
        self.offheap.extend(unsafe { crate::resource::collect(self.hp, free) });
        self.hp = free;
        r
    }
}
