//! Term encoding helpers. See `tonic_shared::enc` for the layout.

pub use tonic_shared::atom as a;
pub use tonic_shared::enc::*;

pub type Term = u64;

#[inline(always)]
pub fn is_small(t: Term) -> bool {
    t & 1 == 1
}
#[inline(always)]
pub fn small_val(t: Term) -> i64 {
    (t as i64) >> 1
}
#[inline(always)]
pub fn is_atom(t: Term) -> bool {
    t & 7 == 2
}
#[inline(always)]
pub fn atom_idx(t: Term) -> u64 {
    t >> 3
}
#[inline(always)]
pub fn is_pid(t: Term) -> bool {
    t & 7 == 6
}
#[inline(always)]
pub fn pid_id(t: Term) -> u64 {
    t >> 3
}
#[inline(always)]
pub fn is_ptr(t: Term) -> bool {
    t & 7 == 0 && t != 0
}
#[inline(always)]
pub fn ptr(t: Term) -> *mut u64 {
    t as *mut u64
}
#[inline(always)]
pub fn hdr(t: Term) -> u64 {
    unsafe { *ptr(t) }
}
#[inline(always)]
pub fn tag_of_hdr(h: u64) -> u64 {
    h & 0xff
}
#[inline(always)]
pub fn size_of_hdr(h: u64) -> u64 {
    h >> 8
}
#[inline(always)]
pub fn boxed_tag(t: Term) -> u64 {
    tag_of_hdr(hdr(t))
}
#[inline(always)]
pub fn is_boxed_tag(t: Term, tag: u64) -> bool {
    is_ptr(t) && boxed_tag(t) == tag
}
#[inline(always)]
pub fn word(t: Term, i: usize) -> u64 {
    unsafe { *ptr(t).add(i) }
}
#[inline(always)]
pub fn set_word(t: Term, i: usize, v: u64) {
    unsafe { *ptr(t).add(i) = v }
}

pub const TRUE: Term = atom(a::TRUE);
pub const FALSE: Term = atom(a::FALSE);
pub const NIL: Term = atom(a::NIL);

#[inline(always)]
pub fn boolean(b: bool) -> Term {
    if b {
        TRUE
    } else {
        FALSE
    }
}
#[inline(always)]
pub fn is_truthy(t: Term) -> bool {
    t != NIL && t != FALSE
}

// ---- Tuples ----
#[inline(always)]
pub fn is_tuple(t: Term) -> bool {
    is_boxed_tag(t, T_TUPLE)
}
#[inline(always)]
pub fn tuple_size(t: Term) -> usize {
    size_of_hdr(hdr(t)) as usize
}
#[inline(always)]
pub fn tuple_get(t: Term, i: usize) -> Term {
    word(t, 1 + i)
}

// ---- Lists ----
#[inline(always)]
pub fn is_cons(t: Term) -> bool {
    is_boxed_tag(t, T_CONS)
}
#[inline(always)]
pub fn is_list(t: Term) -> bool {
    t == NIL_LIST || is_cons(t)
}
#[inline(always)]
pub fn head(t: Term) -> Term {
    word(t, 1)
}
#[inline(always)]
pub fn tail(t: Term) -> Term {
    word(t, 2)
}

// ---- Numbers ----
#[inline(always)]
pub fn is_float(t: Term) -> bool {
    is_boxed_tag(t, T_FLOAT)
}
#[inline(always)]
pub fn float_val(t: Term) -> f64 {
    f64::from_bits(word(t, 1))
}
#[inline(always)]
pub fn is_bigint(t: Term) -> bool {
    is_boxed_tag(t, T_BIGINT)
}
#[inline(always)]
pub fn is_integer(t: Term) -> bool {
    is_small(t) || is_bigint(t)
}
#[inline(always)]
pub fn is_number(t: Term) -> bool {
    is_integer(t) || is_float(t)
}
#[inline(always)]
pub fn fits_small(i: i64) -> bool {
    (SMALL_MIN..=SMALL_MAX).contains(&i)
}

// ---- Binaries ----
#[inline(always)]
pub fn is_binary(t: Term) -> bool {
    is_ptr(t) && {
        let tg = boxed_tag(t);
        tg == T_BINARY || tg == T_SUBBIN
    }
}

/// Borrow the bytes of a binary or sub-binary. The slice is only valid until the
/// next allocation (GC may move the data).
#[inline]
pub fn bin_bytes<'a>(t: Term) -> &'a [u8] {
    unsafe {
        let h = hdr(t);
        if tag_of_hdr(h) == T_BINARY {
            let len = word(t, 1) as usize;
            std::slice::from_raw_parts(ptr(t).add(2) as *const u8, len)
        } else {
            let base = word(t, 1);
            let off = word(t, 2) as usize;
            let len = word(t, 3) as usize;
            let bl = word(base, 1) as usize;
            debug_assert!(off + len <= bl);
            std::slice::from_raw_parts((ptr(base).add(2) as *const u8).add(off), len)
        }
    }
}

/// Any bitstring (binary or non-byte-aligned bitstring).
#[inline(always)]
pub fn is_bitstring(t: Term) -> bool {
    is_ptr(t) && {
        let tg = boxed_tag(t);
        tg == T_BINARY || tg == T_SUBBIN || tg == T_BITS
    }
}

/// Size in bits of a bitstring.
#[inline]
pub fn bit_len(t: Term) -> usize {
    if boxed_tag(t) == T_BITS {
        word(t, 1) as usize
    } else {
        bin_len(t) * 8
    }
}

/// Bytes backing a bitstring (the last byte may be partial, MSB-first).
#[inline]
pub fn bit_bytes<'a>(t: Term) -> &'a [u8] {
    if boxed_tag(t) == T_BITS {
        let n = (word(t, 1) as usize + 7) / 8;
        unsafe { std::slice::from_raw_parts(ptr(t).add(2) as *const u8, n) }
    } else {
        bin_bytes(t)
    }
}

#[inline(always)]
pub fn bin_len(t: Term) -> usize {
    if boxed_tag(t) == T_BINARY {
        word(t, 1) as usize
    } else {
        word(t, 3) as usize
    }
}

// ---- Maps ----
#[inline(always)]
pub fn is_map(t: Term) -> bool {
    is_boxed_tag(t, T_MAP)
}

// ---- Closures ----
#[inline(always)]
pub fn is_closure(t: Term) -> bool {
    is_boxed_tag(t, T_CLOSURE)
}
#[inline(always)]
pub fn closure_arity(t: Term) -> u64 {
    word(t, 2)
}

#[repr(C)]
pub struct FunInfo {
    /// 0 = anonymous fn, 1 = capture of named function
    pub kind: u64,
    pub module: Term,
    pub name: Term,
    pub arity: u64,
    pub index: u64,
}

// ---- References ----
#[inline(always)]
pub fn is_ref(t: Term) -> bool {
    is_boxed_tag(t, T_REF)
}
