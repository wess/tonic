//! `:atomics` (and `:counters`, built on it): arrays of 64-bit integers
//! shared between processes, updated with hardware atomics.
//!
//! Arrays live until their last reference is collected, including copies
//! held by other processes and ETS tables.

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::num;
use crate::term::*;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::Arc;

struct Array {
    signed: bool,
    cells: Vec<AtomicU64>,
}

fn lookup(r: Term) -> Option<Arc<Array>> {
    crate::resource::get(r)
}

/// The array and cell named by (ref, 1-based index).
fn cell(r: Term, i: Term) -> Option<(Arc<Array>, usize)> {
    let a = lookup(r)?;
    if !is_small(i) || small_val(i) < 1 || small_val(i) as usize > a.cells.len() {
        return None;
    }
    Some((a, small_val(i) as usize - 1))
}

/// A term as the cell's bit pattern, if it is in the array's range.
fn encode(a: &Array, v: Term) -> Option<u64> {
    if !is_integer(v) {
        return None;
    }
    let b = num::to_big(v);
    if a.signed {
        i64::try_from(b).ok().map(|x| x as u64)
    } else {
        u64::try_from(b).ok()
    }
}

/// Like `encode`, for increments: any 64-bit value applies modulo 2^64.
fn encode_incr(v: Term) -> Option<u64> {
    if !is_integer(v) {
        return None;
    }
    let b = num::to_big(v);
    i64::try_from(&b).map(|x| x as u64).ok().or_else(|| u64::try_from(b).ok())
}

fn decode(ctx: &mut Ctx, a: &Array, bits: u64) -> Term {
    if a.signed {
        num::int_from_i64(ctx, bits as i64)
    } else {
        num::int_from_i128(ctx, bits as i128)
    }
}

#[no_mangle]
pub extern "C" fn tn_atomics_new(c: *mut Ctx, size: Term, signed: Term) -> Term {
    let ctx = cx(c);
    if !is_small(size) || small_val(size) < 1 {
        return ctx.badarg("argument error");
    }
    let n = small_val(size) as usize;
    let a = Array { signed: signed == atom(crate::atoms::intern("true")), cells: (0..n).map(|_| AtomicU64::new(0)).collect() };
    ctx.managed_ref(a)
}

#[no_mangle]
pub extern "C" fn tn_atomics_size(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    match lookup(r) {
        Some(a) => small(a.cells.len() as i64),
        None => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_atomics_signed(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    match lookup(r) {
        Some(a) => atom(crate::atoms::intern(if a.signed { "true" } else { "false" })),
        None => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_atomics_get(c: *mut Ctx, r: Term, i: Term) -> Term {
    let ctx = cx(c);
    match cell(r, i) {
        Some((a, k)) => {
            let bits = a.cells[k].load(SeqCst);
            decode(ctx, &a, bits)
        }
        None => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_atomics_put(c: *mut Ctx, r: Term, i: Term, v: Term) -> Term {
    let ctx = cx(c);
    match cell(r, i).and_then(|(a, k)| Some((encode(&a, v)?, a, k))) {
        Some((bits, a, k)) => {
            a.cells[k].store(bits, SeqCst);
            atom(crate::atoms::intern("ok"))
        }
        None => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_atomics_add_get(c: *mut Ctx, r: Term, i: Term, incr: Term) -> Term {
    let ctx = cx(c);
    match cell(r, i).and_then(|(a, k)| Some((encode_incr(incr)?, a, k))) {
        Some((d, a, k)) => {
            let bits = a.cells[k].fetch_add(d, SeqCst).wrapping_add(d);
            decode(ctx, &a, bits)
        }
        None => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_atomics_exchange(c: *mut Ctx, r: Term, i: Term, v: Term) -> Term {
    let ctx = cx(c);
    match cell(r, i).and_then(|(a, k)| Some((encode(&a, v)?, a, k))) {
        Some((bits, a, k)) => {
            let old = a.cells[k].swap(bits, SeqCst);
            decode(ctx, &a, old)
        }
        None => ctx.badarg("argument error"),
    }
}

/// `:ok` when the cell held `expected` and now holds `desired`, otherwise
/// the value it holds.
#[no_mangle]
pub extern "C" fn tn_atomics_compare_exchange(c: *mut Ctx, r: Term, i: Term, expected: Term, desired: Term) -> Term {
    let ctx = cx(c);
    let found = cell(r, i).and_then(|(a, k)| Some((encode(&a, expected)?, encode(&a, desired)?, a, k)));
    match found {
        Some((e, d, a, k)) => match a.cells[k].compare_exchange(e, d, SeqCst, SeqCst) {
            Ok(_) => atom(crate::atoms::intern("ok")),
            Err(actual) => decode(ctx, &a, actual),
        },
        None => ctx.badarg("argument error"),
    }
}
