//! Per-process heaps and the copying garbage collector.
//!
//! Every Elixir process owns a private semispace heap. Collection is a Cheney
//! copy driven from precise roots:
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

pub const MIN_HEAP_WORDS: usize = 2048;

#[repr(C)]
pub struct Ctx {
    // ---- fields read/written by compiled code (offsets are ABI) ----
    pub frame: *mut u64, // 0
    pub hp: *mut u64,    // 8
    pub hend: *mut u64,  // 16
    pub reds: i64,       // 24
    // ---- runtime private ----
    pub heap_start: *mut u64,
    pub heap_words: usize,
    pub heap_buf: Vec<u64>,
    pub min_heap: usize,
    /// Shadow-stack words scanned by the last collection.
    pub last_stack_words: usize,
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
            reds: REDUCTIONS,
            heap_start: start,
            heap_words: min_heap,
            heap_buf,
            min_heap,
            last_stack_words: 0,
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
    pub fn in_heap(&self, p: *const u64) -> bool {
        p >= self.heap_start as *const u64
            && (p as usize) < self.heap_start as usize + self.heap_words * 8
    }

    pub fn gc(&mut self, need: usize) {
        let used = (self.hp as usize - self.heap_start as usize) / 8;
        let to_words = self.min_heap.max(used + need + 64);
        self.collect_into(to_words);
        let live = (self.hp as usize - self.heap_start as usize) / 8;
        // Like the BEAM (whose stack lives in the heap block), a deep stack
        // counts towards the heap size so deep recursion doesn't trigger a
        // full stack scan every few allocations.
        let live = live + self.last_stack_words / 2;
        if (live + need) * 2 > self.heap_words {
            // Grow: copy once more into a heap with comfortable headroom.
            let grown = ((live + need) * 3).max(self.min_heap);
            self.collect_into(grown);
        } else if self.heap_words > self.min_heap * 4 && (live + need) * 8 < self.heap_words {
            let shrunk = ((live + need) * 3).max(self.min_heap);
            self.collect_into(shrunk);
        }
    }

    fn collect_into(&mut self, to_words: usize) {
        self.gc_count += 1;
        let mut to: Vec<u64> = Vec::with_capacity(to_words);
        let to_start = to.as_mut_ptr();
        unsafe { to.set_len(to_words) };
        let from_lo = self.heap_start as usize;
        let from_hi = from_lo + self.heap_words * 8;
        let mut gc = Cheney {
            from_lo,
            from_hi,
            free: to_start,
        };
        unsafe {
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
            // Cheney scan.
            let mut scan = to_start;
            while scan < gc.free {
                let h = *scan;
                let n = size_of_hdr(h) as usize;
                match tag_of_hdr(h) {
                    T_TUPLE => {
                        for i in 1..=n {
                            *scan.add(i) = gc.evac(*scan.add(i));
                        }
                    }
                    T_CONS => {
                        *scan.add(1) = gc.evac(*scan.add(1));
                        *scan.add(2) = gc.evac(*scan.add(2));
                    }
                    T_MAP => {
                        *scan.add(2) = gc.evac(*scan.add(2));
                    }
                    T_MAPNODE | T_HAMT => {
                        for i in 1..=n {
                            *scan.add(i) = gc.evac(*scan.add(i));
                        }
                    }
                    T_CLOSURE => {
                        for i in 4..=n {
                            *scan.add(i) = gc.evac(*scan.add(i));
                        }
                    }
                    T_SUBBIN => {
                        *scan.add(1) = gc.evac(*scan.add(1));
                    }
                    _ => {}
                }
                scan = scan.add(1 + n.max(1));
            }
        }
        let used_words = (gc.free as usize - to_start as usize) / 8;
        self.heap_buf = to;
        self.heap_start = to_start;
        self.heap_words = to_words;
        self.hp = unsafe { to_start.add(used_words) };
        self.hend = unsafe { to_start.add(to_words) };
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

struct Cheney {
    from_lo: usize,
    from_hi: usize,
    free: *mut u64,
}

impl Cheney {
    #[inline(always)]
    unsafe fn evac(&mut self, t: u64) -> u64 {
        if t & 7 != 0 || t == 0 {
            return t;
        }
        let a = t as usize;
        if a < self.from_lo || a >= self.from_hi {
            return t; // static literal or foreign
        }
        let p = t as *mut u64;
        let h = *p;
        if tag_of_hdr(h) == T_FORWARD {
            return *p.add(1);
        }
        let n = 1 + (size_of_hdr(h) as usize).max(1);
        let dst = self.free;
        std::ptr::copy_nonoverlapping(p, dst, n);
        self.free = dst.add(n);
        *p = T_FORWARD;
        *p.add(1) = dst as u64;
        dst as u64
    }
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
        if boff == 0 && off == 0 && len == bin_len(base) {
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
}
unsafe impl Send for Fragment {}

/// Words needed to copy `t` out of the region [lo, hi).
pub fn copy_size(t: Term, lo: usize, hi: usize) -> usize {
    let mut total = 0usize;
    let mut stack: Vec<Term> = vec![t];
    while let Some(mut t) = stack.pop() {
        loop {
            if !is_ptr(t) || (t as usize) < lo || (t as usize) >= hi {
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
    if !is_ptr(t) || (t as usize) < lo || (t as usize) >= hi {
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
                *cell.add(1) = copy_term(hd, lo, hi, free);
                let tl = word(cur, 2);
                prev = cell;
                if is_ptr(tl) && (tl as usize) >= lo && (tl as usize) < hi && boxed_tag(tl) == T_CONS {
                    cur = tl;
                    continue;
                }
                *cell.add(2) = copy_term(tl, lo, hi, free);
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
                *dst.add(i) = copy_term(word(t, i), lo, hi, free);
            }
            dst as u64
        }
        T_MAP => {
            let dst = *free;
            *free = dst.add(3);
            *dst = h;
            *dst.add(1) = word(t, 1);
            *dst.add(2) = copy_term(word(t, 2), lo, hi, free);
            dst as u64
        }
        T_MAPNODE | T_HAMT => {
            let dst = *free;
            *free = dst.add(1 + n);
            *dst = h;
            for i in 1..=n {
                *dst.add(i) = copy_term(word(t, i), lo, hi, free);
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
                *dst.add(i) = copy_term(word(t, i), lo, hi, free);
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
            dst as u64
        }
    }
}

impl Ctx {
    /// Copy a term out of this process heap into a standalone fragment.
    pub fn to_fragment(&self, t: Term) -> Fragment {
        let lo = self.heap_start as usize;
        let hi = lo + self.heap_words * 8;
        let words = copy_size(t, lo, hi);
        let mut buf: Vec<u64> = Vec::with_capacity(words.max(1));
        unsafe { buf.set_len(words.max(1)) };
        let mut free = buf.as_mut_ptr();
        let root = unsafe { copy_term(t, lo, hi, &mut free) };
        Fragment { buf, root }
    }

    /// Copy a fragment's term into this heap.
    pub fn from_fragment(&mut self, f: &Fragment) -> Term {
        let lo = f.buf.as_ptr() as usize;
        let hi = lo + f.buf.len() * 8;
        let words = copy_size(f.root, lo, hi);
        self.reserve(words);
        let mut free = self.hp;
        let r = unsafe { copy_term(f.root, lo, hi, &mut free) };
        self.hp = free;
        r
    }
}
