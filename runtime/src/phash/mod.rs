//! `erlang:phash/2` and `erlang:phash2/1,2`: ports of OTP 27's `make_hash`
//! and `make_hash2` (erl_term_hashing.c). These are the portable hashes, so
//! values match the BEAM for every term except local funs (OTP mixes in the
//! module's code checksum) and pids/refs (hashed by their printed numbers,
//! which differ between runtimes).

mod make;
mod make2;

use crate::atoms;
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::term::*;

pub use make::make_hash;
pub use make2::make_hash2;

/// `atom_hash()` from atom.c: hashpjw over the UTF-8 name, folding 2-byte
/// sequences for code points 0x80..0xFF back to their latin1 byte.
pub fn atom_hval(t: Term) -> u32 {
    let name = atoms::name(atom_idx(t)).as_bytes();
    let mut h: u32 = 0;
    let mut i = 0;
    while i < name.len() {
        let mut v = name[i];
        i += 1;
        if i < name.len() && (v & 0xFE) == 0xC2 && (name[i] & 0xC0) == 0x80 {
            v = (v << 6) | (name[i] & 0x3F);
            i += 1;
        }
        h = (h << 4).wrapping_add(v as u32);
        let g = h & 0xf000_0000;
        if g != 0 {
            h ^= g >> 24;
            h ^= g;
        }
    }
    h
}

/// The magnitude of an integer as 64-bit digits, least significant first.
fn int_digits(t: Term, mut f: impl FnMut(u64)) -> bool {
    if is_small(t) {
        let v = small_val(t);
        f(v.unsigned_abs());
        v < 0
    } else {
        let n = size_of_hdr(hdr(t)) as usize - 1;
        for i in 0..n {
            f(word(t, 2 + i));
        }
        word(t, 1) != 0
    }
}

/// Logical bytes of a bitstring plus its trailing bits (right-aligned).
fn bits_of<'a>(t: Term) -> (&'a [u8], u32, u8) {
    let nbits = bit_len(t);
    let all = bit_bytes(t);
    let sz = nbits / 8;
    let tail = (nbits % 8) as u32;
    let last = if tail > 0 { all[sz] >> (8 - tail) } else { 0 };
    (&all[..sz], tail, last)
}

/// What OTP hashes of a fun. Tonic has no per-module code checksum, so
/// `uniq` (OTP's `old_uniq`) is always 0 and local funs cannot match.
struct Fun {
    external: bool,
    module: Term,
    name: Term,
    arity: u32,
    index: u32,
    uniq: u32,
}

fn fun_parts(f: Term) -> Fun {
    let info = word(f, 3) as *const FunInfo;
    let arity = closure_arity(f) as u32;
    if info.is_null() {
        return Fun { external: false, module: NIL, name: NIL, arity, index: 0, uniq: 0 };
    }
    let fi = unsafe { &*info };
    Fun { external: fi.kind == 1, module: fi.module, name: fi.name, arity, index: fi.index as u32, uniq: 0 }
}

/// Captured free variables, in order.
fn fun_env<'a>(f: Term) -> &'a [Term] {
    let n = size_of_hdr(hdr(f)) as usize;
    unsafe { std::slice::from_raw_parts(ptr(f).add(4), n.saturating_sub(3)) }
}

/// The range argument of phash/2 and phash2/2; 0 stands for 2^32.
fn range(ctx: &mut Ctx, r: Term) -> Result<u32, Term> {
    if !is_integer(r) {
        return Err(ctx.badarg_args(&[(2, "not an integer")]));
    }
    let neg = if is_small(r) { small_val(r) <= 0 } else { word(r, 1) != 0 };
    if neg {
        return Err(ctx.badarg_args(&[(2, "out of range")]));
    }
    // OTP's error_info has no description for ranges above 2^32.
    if !is_small(r) || small_val(r) > 1 << 32 {
        return Err(ctx.badarg("argument error"));
    }
    Ok(small_val(r) as u32)
}

#[no_mangle]
pub extern "C" fn tn_phash2_1(_c: *mut Ctx, t: Term) -> Term {
    small((make_hash2(t) & ((1 << 27) - 1)) as i64)
}

#[no_mangle]
pub extern "C" fn tn_phash2(c: *mut Ctx, t: Term, r: Term) -> Term {
    match range(cx(c), r) {
        Err(e) => e,
        Ok(0) => small(make_hash2(t) as i64),
        Ok(n) => small((make_hash2(t) % n) as i64),
    }
}

#[no_mangle]
pub extern "C" fn tn_phash(c: *mut Ctx, t: Term, r: Term) -> Term {
    match range(cx(c), r) {
        Err(e) => e,
        Ok(0) => small(make_hash(t) as i64 + 1),
        Ok(n) => small((make_hash(t) % n) as i64 + 1),
    }
}
