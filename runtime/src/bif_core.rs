//! Core BIFs: arithmetic, comparison, type tests, tuples, lists, maps,
//! conversions, closures and dynamic calls.

use crate::atoms;
use crate::cmp;
use crate::heap::Ctx;
use crate::map;
use crate::num::{self, ArithErr};
use crate::term::*;
use std::ffi::c_void;

#[inline(always)]
pub fn cx<'a>(c: *mut Ctx) -> &'a mut Ctx {
    unsafe { &mut *c }
}

macro_rules! arith2 {
    ($name:ident, $f:path, $op:expr) => {
        #[no_mangle]
        pub extern "C" fn $name(c: *mut Ctx, a: Term, b: Term) -> Term {
            let ctx = cx(c);
            match $f(ctx, a, b) {
                Ok(t) => t,
                Err(ArithErr::BadArith) => match $op {
                    "" => ctx.badarith(),
                    op => ctx.badarith_op(op, &[a, b]),
                },
                Err(ArithErr::SystemLimit) => ctx.system_limit(),
            }
        }
    };
}

arith2!(tn_add, num::add, "+");
arith2!(tn_sub, num::sub, "-");
arith2!(tn_mul, num::mul, "*");
// `/` is compiled inline on the BEAM and fails without an :erlang frame
arith2!(tn_fdiv, num::fdiv, "");
arith2!(tn_div, num::idiv, "div");
arith2!(tn_rem, num::irem, "rem");
arith2!(tn_floor_div, num::floor_div, "");
arith2!(tn_mod, num::modulo, "");
arith2!(tn_pow_int, num::pow_int, "");

#[no_mangle]
pub extern "C" fn tn_neg(c: *mut Ctx, a: Term) -> Term {
    let ctx = cx(c);
    num::neg(ctx, a).unwrap_or_else(|_| ctx.badarith_op("-", &[a]))
}
#[no_mangle]
pub extern "C" fn tn_pos(c: *mut Ctx, a: Term) -> Term {
    if is_number(a) {
        a
    } else {
        cx(c).badarith_op("+", &[a])
    }
}
#[no_mangle]
pub extern "C" fn tn_abs(c: *mut Ctx, a: Term) -> Term {
    let ctx = cx(c);
    // abs/1 is a BIF: a non-number is a badarg, not badarith
    num::abs(ctx, a).unwrap_or_else(|_| ctx.badarg_args(&[(1, "not a number")]))
}

macro_rules! bitop {
    ($name:ident, $op:expr) => {
        #[no_mangle]
        pub extern "C" fn $name(c: *mut Ctx, a: Term, b: Term) -> Term {
            let ctx = cx(c);
            num::bitop(ctx, $op, a, b).unwrap_or_else(|e| {
                if matches!(e, num::ArithErr::SystemLimit) {
                    return ctx.system_limit();
                }
                let name = match $op {
                    b'&' => "band",
                    b'|' => "bor",
                    b'^' => "bxor",
                    b'<' => "bsl",
                    _ => "bsr",
                };
                ctx.badarith_op(name, &[a, b])
            })
        }
    };
}
bitop!(tn_band, b'&');
bitop!(tn_bor, b'|');
bitop!(tn_bxor, b'^');
bitop!(tn_bsl, b'<');
bitop!(tn_bsr, b'>');
#[no_mangle]
pub extern "C" fn tn_bnot(c: *mut Ctx, a: Term) -> Term {
    let ctx = cx(c);
    num::bnot(ctx, a).unwrap_or_else(|_| ctx.badarith_op("bnot", &[a]))
}

#[no_mangle]
pub extern "C" fn tn_pow(c: *mut Ctx, a: Term, b: Term) -> Term {
    let ctx = cx(c);
    if is_integer(a) && is_integer(b) && !(is_small(b) && small_val(b) < 0) {
        return num::pow_int(ctx, a, b).unwrap_or_else(|_| ctx.badarith());
    }
    if is_number(a) && is_number(b) {
        let r = num::num_to_f64(a).powf(num::num_to_f64(b));
        if !r.is_finite() {
            return ctx.badarith();
        }
        return ctx.float(r);
    }
    ctx.badarith()
}

// ---------------- comparisons ----------------

#[no_mangle]
pub extern "C" fn tn_eq(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(cmp::eq(a, b))
}
#[no_mangle]
pub extern "C" fn tn_ne(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(!cmp::eq(a, b))
}
#[no_mangle]
pub extern "C" fn tn_eqx(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(cmp::eq_exact(a, b))
}
#[no_mangle]
pub extern "C" fn tn_nex(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(!cmp::eq_exact(a, b))
}
#[no_mangle]
pub extern "C" fn tn_lt(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(cmp::compare(a, b).is_lt())
}
#[no_mangle]
pub extern "C" fn tn_gt(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(cmp::compare(a, b).is_gt())
}
#[no_mangle]
pub extern "C" fn tn_le(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(cmp::compare(a, b).is_le())
}
#[no_mangle]
pub extern "C" fn tn_ge(_c: *mut Ctx, a: Term, b: Term) -> Term {
    boolean(cmp::compare(a, b).is_ge())
}
#[no_mangle]
pub extern "C" fn tn_compare(_c: *mut Ctx, a: Term, b: Term) -> Term {
    small(cmp::compare(a, b) as i64)
}
#[no_mangle]
pub extern "C" fn tn_max(_c: *mut Ctx, a: Term, b: Term) -> Term {
    if cmp::compare(a, b).is_lt() {
        b
    } else {
        a
    }
}
#[no_mangle]
pub extern "C" fn tn_min(_c: *mut Ctx, a: Term, b: Term) -> Term {
    if cmp::compare(a, b).is_gt() {
        b
    } else {
        a
    }
}

#[no_mangle]
pub extern "C" fn tn_not(c: *mut Ctx, a: Term) -> Term {
    if a == TRUE {
        FALSE
    } else if a == FALSE {
        TRUE
    } else {
        cx(c).badarg("argument error")
    }
}

// ---------------- type tests ----------------

macro_rules! typetest {
    ($name:ident, $f:expr) => {
        #[no_mangle]
        pub extern "C" fn $name(_c: *mut Ctx, a: Term) -> Term {
            let f: fn(Term) -> bool = $f;
            boolean(f(a))
        }
    };
}
typetest!(tn_is_atom, is_atom);
typetest!(tn_is_binary, is_binary);
typetest!(tn_is_bitstring, is_bitstring);
typetest!(tn_is_boolean, |a| a == TRUE || a == FALSE);
typetest!(tn_is_float, is_float);
typetest!(tn_is_function, is_closure);
typetest!(tn_is_integer, is_integer);
typetest!(tn_is_list, is_list);
typetest!(tn_is_map, is_map);
typetest!(tn_is_number, is_number);
typetest!(tn_is_pid, is_pid);
typetest!(tn_is_port, is_port);
typetest!(tn_is_reference, is_ref);
typetest!(tn_is_tuple, is_tuple);
typetest!(tn_is_nil, |a| a == NIL);

#[no_mangle]
pub extern "C" fn tn_is_function2(c: *mut Ctx, f: Term, n: Term) -> Term {
    if !is_small(n) || small_val(n) < 0 {
        return cx(c).badarg("argument error");
    }
    boolean(is_closure(f) && closure_arity(f) == small_val(n) as u64)
}

#[no_mangle]
pub extern "C" fn tn_is_struct(_c: *mut Ctx, m: Term) -> Term {
    boolean(is_map(m) && map::find(m, atom(a::STRUCT)).map(is_atom).unwrap_or(false))
}
#[no_mangle]
pub extern "C" fn tn_is_non_struct_map(_c: *mut Ctx, m: Term) -> Term {
    boolean(is_map(m) && !map::find(m, atom(a::STRUCT)).map(is_atom).unwrap_or(false))
}
#[no_mangle]
pub extern "C" fn tn_is_struct2(c: *mut Ctx, m: Term, name: Term) -> Term {
    if !is_atom(name) {
        return cx(c).badarg("argument error");
    }
    boolean(is_map(m) && map::find(m, atom(a::STRUCT)) == Some(name))
}
#[no_mangle]
pub extern "C" fn tn_is_exception2(c: *mut Ctx, m: Term, name: Term) -> Term {
    if !is_atom(name) {
        return cx(c).badarg("argument error");
    }
    boolean(
        is_map(m)
            && map::find(m, atom(a::EXCEPTION)) == Some(TRUE)
            && map::find(m, atom(a::STRUCT)) == Some(name),
    )
}
#[no_mangle]
pub extern "C" fn tn_is_exception(_c: *mut Ctx, m: Term) -> Term {
    boolean(is_map(m) && map::find(m, atom(a::EXCEPTION)) == Some(TRUE))
}

// ---------------- tuples ----------------

#[no_mangle]
pub extern "C" fn tn_tuple_size(c: *mut Ctx, t: Term) -> Term {
    if is_tuple(t) {
        small(tuple_size(t) as i64)
    } else {
        cx(c).badarg_args(&[(1, "not a tuple")])
    }
}

/// :erlang.size/1: a tuple's arity or a binary's byte size.
#[no_mangle]
pub extern "C" fn tn_size(c: *mut Ctx, t: Term) -> Term {
    if is_tuple(t) {
        small(tuple_size(t) as i64)
    } else if is_bitstring(t) {
        small(((bit_len(t) + 7) / 8) as i64)
    } else {
        cx(c).badarg_args(&[(1, "not tuple or binary")])
    }
}

/// Kernel.elem/2 (0-based)
#[no_mangle]
pub extern "C" fn tn_elem(c: *mut Ctx, t: Term, i: Term) -> Term {
    if is_tuple(t) && is_small(i) {
        let i = small_val(i);
        if i >= 0 && (i as usize) < tuple_size(t) {
            return tuple_get(t, i as usize);
        }
    }
    cx(c).badarg_element(if is_small(i) { small(small_val(i) + 1) } else { i }, t, 1, 2)
}

/// :erlang.element/2 (1-based)
#[no_mangle]
pub extern "C" fn tn_element(c: *mut Ctx, i: Term, t: Term) -> Term {
    if is_small(i) {
        return tn_elem(c, t, small(small_val(i) - 1));
    }
    cx(c).badarg_element(i, t, 1, 2)
}

#[no_mangle]
pub extern "C" fn tn_put_elem(c: *mut Ctx, t: Term, i: Term, v: Term) -> Term {
    let ctx = cx(c);
    if is_tuple(t) && is_small(i) {
        let idx = small_val(i);
        let n = tuple_size(t);
        if idx >= 0 && (idx as usize) < n {
            let mut r = [t, v];
            ctx.reserve_with(n + 1, &mut r);
            let p = ctx.tuple_uninit_nogc(n);
            for j in 0..n {
                unsafe { *p.add(1 + j) = tuple_get(r[0], j) };
            }
            unsafe { *p.add(1 + idx as usize) = r[1] };
            return p as Term;
        }
    }
    ctx.badarg_element(if is_small(i) { small(small_val(i) + 1) } else { i }, t, 1, 2)
}

#[no_mangle]
pub extern "C" fn tn_setelement(c: *mut Ctx, i: Term, t: Term, v: Term) -> Term {
    if is_small(i) {
        return tn_put_elem(c, t, small(small_val(i) - 1), v);
    }
    cx(c).badarg_element(i, t, 1, 2)
}

#[no_mangle]
pub extern "C" fn tn_tuple_to_list(c: *mut Ctx, t: Term) -> Term {
    let ctx = cx(c);
    if !is_tuple(t) {
        return ctx.badarg_args(&[(1, "not a tuple")]);
    }
    let n = tuple_size(t);
    let mut r = [t];
    ctx.reserve_with(3 * n, &mut r);
    let mut acc = NIL_LIST;
    for i in (0..n).rev() {
        acc = ctx.cons_nogc(tuple_get(r[0], i), acc);
    }
    acc
}

#[no_mangle]
pub extern "C" fn tn_list_to_tuple(c: *mut Ctx, l: Term) -> Term {
    let ctx = cx(c);
    let mut n = 0;
    let mut x = l;
    while is_cons(x) {
        n += 1;
        x = tail(x);
    }
    if x != NIL_LIST {
        return ctx.badarg_args(&[(1, "not a list")]);
    }
    if n == 0 {
        return crate::heap::empty_tuple();
    }
    let mut r = [l];
    ctx.reserve_with(n + 1, &mut r);
    let p = ctx.tuple_uninit_nogc(n);
    let mut x = r[0];
    for i in 0..n {
        unsafe { *p.add(1 + i) = head(x) };
        x = tail(x);
    }
    p as Term
}

#[no_mangle]
pub extern "C" fn tn_make_tuple(c: *mut Ctx, n: Term, v: Term) -> Term {
    let ctx = cx(c);
    // the BEAM's largest tuple has 2^24 - 1 elements
    if !is_small(n) || small_val(n) < 0 || small_val(n) > 16_777_215 {
        return ctx.badarg_args(&[(1, "out of range")]);
    }
    let n = small_val(n) as usize;
    if n == 0 {
        return crate::heap::empty_tuple();
    }
    let mut r = [v];
    ctx.reserve_with(n + 1, &mut r);
    let p = ctx.tuple_uninit_nogc(n);
    for i in 0..n {
        unsafe { *p.add(1 + i) = r[0] };
    }
    p as Term
}

#[no_mangle]
pub extern "C" fn tn_tuple_insert_at(c: *mut Ctx, t: Term, i: Term, v: Term) -> Term {
    let ctx = cx(c);
    if !is_tuple(t) || !is_small(i) {
        return ctx.badarg_element(if is_small(i) { small(small_val(i) + 1) } else { i }, t, 1, 2);
    }
    let n = tuple_size(t);
    let idx = small_val(i);
    if idx < 0 || idx as usize > n {
        return ctx.badarg_element(if is_small(i) { small(small_val(i) + 1) } else { i }, t, 1, 2);
    }
    let idx = idx as usize;
    let mut r = [t, v];
    ctx.reserve_with(n + 2, &mut r);
    let p = ctx.tuple_uninit_nogc(n + 1);
    let mut k = 0;
    for j in 0..=n {
        let e = if j == idx {
            r[1]
        } else {
            let e = tuple_get(r[0], k);
            k += 1;
            e
        };
        unsafe { *p.add(1 + j) = e };
    }
    p as Term
}

#[no_mangle]
pub extern "C" fn tn_tuple_delete_at(c: *mut Ctx, t: Term, i: Term) -> Term {
    let ctx = cx(c);
    if !is_tuple(t) || !is_small(i) {
        return ctx.badarg_element(if is_small(i) { small(small_val(i) + 1) } else { i }, t, 1, 2);
    }
    let n = tuple_size(t);
    let idx = small_val(i);
    if idx < 0 || idx as usize >= n {
        return ctx.badarg_element(if is_small(i) { small(small_val(i) + 1) } else { i }, t, 1, 2);
    }
    if n == 1 {
        return crate::heap::empty_tuple();
    }
    let mut r = [t];
    ctx.reserve_with(n, &mut r);
    let p = ctx.tuple_uninit_nogc(n - 1);
    let mut k = 0;
    for j in 0..n {
        if j == idx as usize {
            continue;
        }
        unsafe { *p.add(1 + k) = tuple_get(r[0], j) };
        k += 1;
    }
    p as Term
}

#[no_mangle]
pub extern "C" fn tn_tuple_append(c: *mut Ctx, t: Term, v: Term) -> Term {
    if !is_tuple(t) {
        return cx(c).badarg_args(&[(1, "not a tuple")]);
    }
    tn_tuple_insert_at(c, t, small(tuple_size(t) as i64), v)
}

// ---------------- lists ----------------

#[no_mangle]
pub extern "C" fn tn_hd(c: *mut Ctx, l: Term) -> Term {
    if is_cons(l) {
        head(l)
    } else {
        cx(c).badarg_args(&[(1, "not a nonempty list")])
    }
}
#[no_mangle]
pub extern "C" fn tn_tl(c: *mut Ctx, l: Term) -> Term {
    if is_cons(l) {
        tail(l)
    } else {
        cx(c).badarg_args(&[(1, "not a nonempty list")])
    }
}

pub fn list_len(l: Term) -> Option<usize> {
    let mut n = 0;
    let mut x = l;
    while is_cons(x) {
        n += 1;
        x = tail(x);
    }
    if x == NIL_LIST {
        Some(n)
    } else {
        None
    }
}

#[no_mangle]
pub extern "C" fn tn_length(c: *mut Ctx, l: Term) -> Term {
    match list_len(l) {
        Some(n) => small(n as i64),
        None => cx(c).badarg_args(&[(1, "not a list")]),
    }
}

#[no_mangle]
pub extern "C" fn tn_reverse(c: *mut Ctx, l: Term) -> Term {
    tn_reverse2(c, l, NIL_LIST)
}

#[no_mangle]
pub extern "C" fn tn_reverse2(c: *mut Ctx, l: Term, tl: Term) -> Term {
    let ctx = cx(c);
    let n = match list_len(l) {
        Some(n) => n,
        None => return ctx.badarg("argument error"),
    };
    let mut r = [l, tl];
    ctx.reserve_with(3 * n, &mut r);
    let mut acc = r[1];
    let mut x = r[0];
    while is_cons(x) {
        acc = ctx.cons_nogc(head(x), acc);
        x = tail(x);
    }
    acc
}

/// `++`
#[no_mangle]
pub extern "C" fn tn_append(c: *mut Ctx, l: Term, r: Term) -> Term {
    let ctx = cx(c);
    if r == NIL_LIST && is_list(l) {
        if list_len(l).is_some() {
            return l;
        }
    }
    if l == NIL_LIST {
        return r;
    }
    let n = match list_len(l) {
        Some(n) => n,
        None => return ctx.badarg("argument error"),
    };
    let mut rr = [l, r];
    ctx.reserve_with(3 * n, &mut rr);
    // Build copy of l with tail r, front to back.
    let first = ctx.alloc_nogc(3 * n);
    let mut x = rr[0];
    for i in 0..n {
        unsafe {
            let cell = first.add(3 * i);
            *cell = header(T_CONS, 2);
            *cell.add(1) = head(x);
            *cell.add(2) = if i + 1 < n {
                cell.add(3) as u64
            } else {
                rr[1]
            };
        }
        x = tail(x);
    }
    first as Term
}

/// `--`
#[no_mangle]
pub extern "C" fn tn_subtract(c: *mut Ctx, l: Term, r: Term) -> Term {
    let ctx = cx(c);
    let (n, m) = match (list_len(l), list_len(r)) {
        (Some(n), Some(m)) => (n, m),
        _ => return ctx.badarg("argument error"),
    };
    let mut rr = [l, r];
    ctx.reserve_with(3 * n, &mut rr);
    let mut rem: Vec<Term> = Vec::with_capacity(m);
    let mut x = rr[1];
    while is_cons(x) {
        rem.push(head(x));
        x = tail(x);
    }
    let mut keep: Vec<Term> = Vec::with_capacity(n);
    let mut x = rr[0];
    while is_cons(x) {
        let e = head(x);
        if let Some(pos) = rem.iter().position(|&y| cmp::eq_exact(y, e)) {
            rem.remove(pos);
        } else {
            keep.push(e);
        }
        x = tail(x);
    }
    let mut acc = NIL_LIST;
    for &e in keep.iter().rev() {
        acc = ctx.cons_nogc(e, acc);
    }
    acc
}

#[no_mangle]
pub extern "C" fn tn_lists_member(c: *mut Ctx, e: Term, l: Term) -> Term {
    let mut x = l;
    while is_cons(x) {
        if cmp::eq_exact(head(x), e) {
            return TRUE;
        }
        x = tail(x);
    }
    if x != NIL_LIST {
        return cx(c).badarg_args(&[(2, "not a list")]);
    }
    FALSE
}

/// :lists.keyfind(key, pos, list) (pos 1-based)
#[no_mangle]
pub extern "C" fn tn_keyfind(c: *mut Ctx, k: Term, pos: Term, l: Term) -> Term {
    if !is_small(pos) || small_val(pos) < 1 {
        return cx(c).badarg("argument error");
    }
    let p = small_val(pos) as usize - 1;
    let mut x = l;
    while is_cons(x) {
        let t = head(x);
        if is_tuple(t) && tuple_size(t) > p && cmp::eq(tuple_get(t, p), k) {
            return t;
        }
        x = tail(x);
    }
    FALSE
}

/// Stable sort by term order (:lists.sort/1).
#[no_mangle]
pub extern "C" fn tn_sort(c: *mut Ctx, l: Term) -> Term {
    let ctx = cx(c);
    let n = match list_len(l) {
        Some(n) => n,
        None => return ctx.badarg("argument error"),
    };
    let mut r = [l];
    ctx.reserve_with(3 * n, &mut r);
    let mut v: Vec<Term> = Vec::with_capacity(n);
    let mut x = r[0];
    while is_cons(x) {
        v.push(head(x));
        x = tail(x);
    }
    v.sort_by(|a, b| cmp::compare(*a, *b));
    let mut acc = NIL_LIST;
    for &e in v.iter().rev() {
        acc = ctx.cons_nogc(e, acc);
    }
    acc
}

/// Sort descending (stable w.r.t. equal elements keeping original order).
#[no_mangle]
pub extern "C" fn tn_sort_desc(c: *mut Ctx, l: Term) -> Term {
    let ctx = cx(c);
    let n = match list_len(l) {
        Some(n) => n,
        None => return ctx.badarg("argument error"),
    };
    let mut r = [l];
    ctx.reserve_with(3 * n, &mut r);
    let mut v: Vec<Term> = Vec::with_capacity(n);
    let mut x = r[0];
    while is_cons(x) {
        v.push(head(x));
        x = tail(x);
    }
    v.sort_by(|a, b| cmp::compare(*b, *a));
    let mut acc = NIL_LIST;
    for &e in v.iter().rev() {
        acc = ctx.cons_nogc(e, acc);
    }
    acc
}

/// :lists.seq(from, to) — used by ranges.
#[no_mangle]
pub extern "C" fn tn_seq(c: *mut Ctx, from: Term, to: Term, step: Term) -> Term {
    let ctx = cx(c);
    if !(is_small(from) && is_small(to) && is_small(step)) || small_val(step) == 0 {
        return ctx.badarg("argument error");
    }
    let (f, t, s) = (small_val(from), small_val(to), small_val(step));
    let n: i64 = if (s > 0 && f > t) || (s < 0 && f < t) {
        0
    } else {
        (t - f) / s + 1
    };
    let n = n as usize;
    ctx.reserve(3 * n);
    let mut acc = NIL_LIST;
    for i in (0..n as i64).rev() {
        acc = ctx.cons_nogc(small(f + i * s), acc);
    }
    acc
}

#[no_mangle]
pub extern "C" fn tn_list_last(c: *mut Ctx, l: Term) -> Term {
    if !is_cons(l) {
        return cx(c).badarg("argument error");
    }
    let mut x = l;
    while is_cons(tail(x)) {
        x = tail(x);
    }
    head(x)
}

// ---------------- maps ----------------

#[no_mangle]
pub extern "C" fn tn_map_size(c: *mut Ctx, m: Term) -> Term {
    if is_map(m) {
        small(map::size(m) as i64)
    } else {
        cx(c).badmap(m)
    }
}

/// Raw lookup: value or NONE (no exception). For pattern matching.
#[no_mangle]
pub extern "C" fn tn_map_find(_c: *mut Ctx, m: Term, k: Term) -> Term {
    if is_map(m) {
        map::find(m, k).unwrap_or(NONE)
    } else {
        NONE
    }
}

/// :erlang.map_get/2, raises.
#[no_mangle]
pub extern "C" fn tn_map_get(c: *mut Ctx, k: Term, m: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    match map::find(m, k) {
        Some(v) => v,
        None => ctx.badkey(k, m),
    }
}

/// Map.get(m, k, default)
#[no_mangle]
pub extern "C" fn tn_map_get3(c: *mut Ctx, m: Term, k: Term, d: Term) -> Term {
    if !is_map(m) {
        return cx(c).badmap(m);
    }
    map::find(m, k).unwrap_or(d)
}

#[no_mangle]
pub extern "C" fn tn_is_map_key(c: *mut Ctx, m: Term, k: Term) -> Term {
    if !is_map(m) {
        return cx(c).badmap(m);
    }
    boolean(map::find(m, k).is_some())
}

/// :maps.find → {:ok, v} | :error
#[no_mangle]
pub extern "C" fn tn_map_fetch(c: *mut Ctx, m: Term, k: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    match map::find(m, k) {
        Some(v) => ctx.tuple(&[atom(a::OK), v]),
        None => atom(a::ERROR),
    }
}

#[no_mangle]
pub extern "C" fn tn_map_put(c: *mut Ctx, m: Term, k: Term, v: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    map::put(ctx, m, k, v)
}

#[no_mangle]
pub extern "C" fn tn_map_remove(c: *mut Ctx, m: Term, k: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    map::remove(ctx, m, k)
}

#[no_mangle]
pub extern "C" fn tn_map_merge(c: *mut Ctx, a: Term, b: Term) -> Term {
    let ctx = cx(c);
    if !is_map(a) {
        return ctx.badmap(a);
    }
    if !is_map(b) {
        return ctx.badmap(b);
    }
    map::merge(ctx, a, b)
}

#[no_mangle]
pub extern "C" fn tn_map_keys(c: *mut Ctx, m: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    map::keys(ctx, m)
}

#[no_mangle]
pub extern "C" fn tn_map_values(c: *mut Ctx, m: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    map::values(ctx, m)
}

#[no_mangle]
pub extern "C" fn tn_map_to_list(c: *mut Ctx, m: Term) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    map::to_list(ctx, m)
}

/// :maps.from_list — list of 2-tuples.
#[no_mangle]
pub extern "C" fn tn_map_from_list(c: *mut Ctx, l: Term) -> Term {
    let ctx = cx(c);
    let base = ctx.roots.len();
    let mut x = l;
    while is_cons(x) {
        let t = head(x);
        if !is_tuple(t) || tuple_size(t) != 2 {
            ctx.roots.truncate(base);
            return ctx.badarg("argument error");
        }
        ctx.roots.push(tuple_get(t, 0));
        ctx.roots.push(tuple_get(t, 1));
        x = tail(x);
    }
    if x != NIL_LIST {
        ctx.roots.truncate(base);
        return ctx.badarg("argument error");
    }
    let n = (ctx.roots.len() - base) / 2;
    let m = map::from_root_pairs(ctx, base, n);
    ctx.roots.truncate(base);
    m
}

/// Build a map from `n` key/value pairs laid out in memory (k0,v0,k1,v1...).
#[no_mangle]
pub extern "C" fn tn_map_build(c: *mut Ctx, pairs: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    let base = ctx.roots.len();
    for i in 0..(2 * n as usize) {
        ctx.roots.push(unsafe { *pairs.add(i) });
    }
    let m = map::from_root_pairs(ctx, base, n as usize);
    ctx.roots.truncate(base);
    m
}

/// `%{m | k1 => v1, ...}`: every key must exist.
#[no_mangle]
pub extern "C" fn tn_map_update_many(c: *mut Ctx, m: Term, pairs: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    for i in 0..n as usize {
        let k = unsafe { *pairs.add(2 * i) };
        if map::find(m, k).is_none() {
            return ctx.badkey(k, m);
        }
    }
    tn_map_put_many(c, m, pairs, n)
}

#[no_mangle]
pub extern "C" fn tn_map_put_many(c: *mut Ctx, m: Term, pairs: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    if !is_map(m) {
        return ctx.badmap(m);
    }
    let base = ctx.roots.len();
    ctx.roots.push(m);
    for i in 0..(2 * n as usize) {
        ctx.roots.push(unsafe { *pairs.add(i) });
    }
    for i in 0..n as usize {
        let cur = ctx.roots[base];
        let k = ctx.roots[base + 1 + 2 * i];
        let v = ctx.roots[base + 2 + 2 * i];
        let nm = map::put(ctx, cur, k, v);
        ctx.roots[base] = nm;
    }
    let r = ctx.roots[base];
    ctx.roots.truncate(base);
    r
}

// ---------------- atoms & conversion ----------------

#[no_mangle]
pub extern "C" fn tn_atom_to_binary(c: *mut Ctx, t: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(t) {
        return ctx.badarg_args(&[(1, "not an atom")]);
    }
    let s = atoms::name(atom_idx(t));
    ctx.str(s)
}

#[no_mangle]
pub extern "C" fn tn_binary_to_atom(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    match std::str::from_utf8(bin_bytes(b)) {
        Ok(s) => atom(atoms::intern(s)),
        Err(_) => ctx.badarg_args(&[(1, "not a binary")]),
    }
}

#[no_mangle]
pub extern "C" fn tn_binary_to_existing_atom(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let s = String::from_utf8_lossy(bin_bytes(b)).into_owned();
    match atoms::lookup(&s) {
        Some(i) => atom(i),
        None => ctx.badarg_args(&[(1, "not an already existing atom")]),
    }
}

#[no_mangle]
pub extern "C" fn tn_integer_to_binary(c: *mut Ctx, i: Term, base: Term) -> Term {
    let ctx = cx(c);
    if !is_integer(i) || !is_small(base) || !(2..=36).contains(&small_val(base)) {
        let mut errs = Vec::new();
        if !is_integer(i) {
            errs.push((1, "not an integer"));
        }
        if !is_small(base) || !(2..=36).contains(&small_val(base)) {
            errs.push((2, "not an integer in the range 2 through 36"));
        }
        return ctx.badarg_args(&errs);
    }
    let s = num::int_to_string(i, small_val(base) as u32);
    ctx.str(&s)
}

#[no_mangle]
pub extern "C" fn tn_binary_to_integer(c: *mut Ctx, b: Term, base: Term) -> Term {
    let ctx = cx(c);
    if !is_small(base) || !(2..=36).contains(&small_val(base)) {
        return ctx.badarg_args(&[(2, "not an integer in the range 2 through 36")]);
    }
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let s = String::from_utf8_lossy(bin_bytes(b)).into_owned();
    match num::parse_int(ctx, &s, small_val(base) as u32) {
        Some(t) => t,
        None => ctx.badarg_args(&[(1, "not a textual representation of an integer")]),
    }
}

/// Float.parse-like: {float, rest} | :error
#[no_mangle]
pub extern "C" fn tn_float_parse(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }
    let ds = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == ds {
        return atom(a::ERROR);
    }
    if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'-' || bytes[j] == b'+') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let s = String::from_utf8_lossy(&bytes[..i]).into_owned();
    let f: f64 = match s.parse::<f64>() {
        Ok(f) if f.is_finite() => f,
        _ => return atom(a::ERROR),
    };
    let total = bytes.len();
    let bi = ctx.push(b);
    let fl = ctx.float(f);
    let fi = ctx.push(fl);
    let rest = ctx.sub_binary(ctx.get(bi), i, total - i);
    let ri = ctx.push(rest);
    let t = ctx.tuple(&[ctx.get(fi), ctx.get(ri)]);
    ctx.truncate(bi);
    t
}

fn erl_float_syntax(b: &[u8]) -> bool {
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let st = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i > st
    };
    if !digits(&mut i) || i >= b.len() || b[i] != b'.' {
        return false;
    }
    i += 1;
    if !digits(&mut i) {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}

#[no_mangle]
pub extern "C" fn tn_binary_to_float(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a textual representation of a float")]);
    }
    let s = String::from_utf8_lossy(bin_bytes(b)).into_owned();
    // Erlang syntax: [+-]digits.digits[(e|E)[+-]digits]
    if !erl_float_syntax(s.as_bytes()) {
        return ctx.badarg_args(&[(1, "not a textual representation of a float")]);
    }
    match s.parse::<f64>() {
        Ok(f) if f.is_finite() => ctx.float(f),
        _ => ctx.badarg_args(&[(1, "not a textual representation of a float")]),
    }
}

#[no_mangle]
pub extern "C" fn tn_float_to_binary(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    if !is_float(f) {
        return ctx.badarg_args(&[(1, "not a float")]);
    }
    // :erlang.float_to_binary/1 is [{scientific, 20}].
    let s = format!("{:.20e}", float_val(f));
    let s = match s.split_once('e') {
        Some((m, e)) => {
            let (sign, digits) = if let Some(r) = e.strip_prefix('-') { ('-', r) } else { ('+', e) };
            format!("{}e{}{:0>2}", m, sign, digits)
        }
        None => s,
    };
    ctx.str(&s)
}

/// Shortest round-trip representation (Float.to_string / [:short]).
#[no_mangle]
pub extern "C" fn tn_float_short(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    if !is_float(f) {
        return ctx.badarg_args(&[(1, "not a float")]);
    }
    let s = num::float_to_string(float_val(f));
    ctx.str(&s)
}

#[no_mangle]
pub extern "C" fn tn_to_float(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if is_float(n) {
        return n;
    }
    if is_integer(n) {
        let f = num::int_to_f64(n);
        // floats are finite on the BEAM
        if f.is_finite() {
            return ctx.float(f);
        }
    }
    ctx.badarg_args(&[(1, "not a number")])
}

#[no_mangle]
pub extern "C" fn tn_trunc(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if is_integer(n) {
        return n;
    }
    if is_float(n) {
        return float_to_int(ctx, float_val(n).trunc());
    }
    ctx.badarg_args(&[(1, "not a number")])
}

fn float_to_int(ctx: &mut Ctx, f: f64) -> Term {
    if f.abs() < 4.0e18 {
        num::int_from_i64(ctx, f as i64)
    } else {
        use num_traits::FromPrimitive;
        let b = num_bigint::BigInt::from_f64(f).unwrap_or_default();
        num::from_big(ctx, &b)
    }
}

#[no_mangle]
pub extern "C" fn tn_round(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if is_integer(n) {
        return n;
    }
    if is_float(n) {
        return float_to_int(ctx, float_val(n).round());
    }
    ctx.badarg_args(&[(1, "not a number")])
}
#[no_mangle]
pub extern "C" fn tn_ceil(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if is_integer(n) {
        return n;
    }
    if is_float(n) {
        return float_to_int(ctx, float_val(n).ceil());
    }
    ctx.badarg_args(&[(1, "not a number")])
}
#[no_mangle]
pub extern "C" fn tn_floor(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if is_integer(n) {
        return n;
    }
    if is_float(n) {
        return float_to_int(ctx, float_val(n).floor());
    }
    ctx.badarg_args(&[(1, "not a number")])
}

// ---------------- identity / misc ----------------

#[no_mangle]
pub extern "C" fn tn_self(c: *mut Ctx) -> Term {
    pid(cx(c).pid())
}

#[no_mangle]
pub extern "C" fn tn_make_ref(c: *mut Ctx) -> Term {
    cx(c).make_ref()
}

/// Protocol dispatch key: struct module atom, or the builtin type's module atom.
#[no_mangle]
pub extern "C" fn tn_impl_for(_c: *mut Ctx, t: Term) -> Term {
    if is_small(t) {
        return atom(a::E_INTEGER);
    }
    if is_atom(t) {
        return atom(a::E_ATOM);
    }
    if is_pid(t) {
        return atom(a::E_PID);
    }
    if t == NIL_LIST {
        return atom(a::E_LIST);
    }
    match boxed_tag(t) {
        T_BIGINT => atom(a::E_INTEGER),
        T_FLOAT => atom(a::E_FLOAT),
        T_BINARY | T_SUBBIN | T_BITS => atom(a::E_BITSTRING),
        T_CONS => atom(a::E_LIST),
        T_TUPLE => atom(a::E_TUPLE),
        T_CLOSURE => atom(a::E_FUNCTION),
        T_REF => atom(a::E_REFERENCE),
        T_PORT => atom(a::E_PORT),
        T_MAP => match map::find(t, atom(a::STRUCT)) {
            Some(s) if is_atom(s) => s,
            _ => atom(a::E_MAP),
        },
        _ => atom(a::E_MAP),
    }
}

/// Builtin type module for a term ignoring structs (for Any fallback).
#[no_mangle]
pub extern "C" fn tn_builtin_type(_c: *mut Ctx, t: Term) -> Term {
    if is_map(t) {
        return atom(a::E_MAP);
    }
    tn_impl_for(_c, t)
}

// ---------------- closures & dynamic calls ----------------

/// Called by compiled code when `f.(args)` has a non-function or wrong arity.
#[no_mangle]
pub extern "C" fn tn_bad_fun(c: *mut Ctx, f: Term, args: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    let base = ctx.roots.len();
    ctx.roots.push(f);
    for i in 0..n as usize {
        ctx.roots.push(unsafe { *args.add(i) });
    }
    let l = ctx.list_from_roots(base + 1, n as usize, NIL_LIST);
    let f = ctx.roots[base];
    ctx.roots.truncate(base);
    if is_closure(f) {
        ctx.bad_arity(f, l)
    } else {
        ctx.bad_function(f)
    }
}

/// Lookup `m.f/arity`; returns function pointer or null (with exception set).
#[no_mangle]
pub extern "C" fn tn_lookup_mfa(c: *mut Ctx, m: Term, f: Term, arity: u64) -> *const c_void {
    let ctx = cx(c);
    if !is_atom(m) {
        // Not a module: Elixir raises BadMapError/UndefinedFunctionError; use undef.
        ctx.undef(m, f, arity);
        return std::ptr::null();
    }
    match crate::sched::lookup_fun(m, f, arity) {
        Some(p) => p as *const c_void,
        None => {
            ctx.undef(m, f, arity);
            std::ptr::null()
        }
    }
}

fn eval_entry(name: &str, arity: u64) -> Option<*const c_void> {
    let module = atom(crate::atoms::intern("Elixir.Tonic.Eval.Modules"));
    let name = atom(crate::atoms::intern(name));
    crate::sched::lookup_fun(module, name, arity).map(|pointer| pointer as *const c_void)
}

#[no_mangle]
pub extern "C" fn tn_function_exported(c: *mut Ctx, m: Term, f: Term, arity: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(m) || !is_atom(f) || !is_small(arity) {
        return ctx.badarg("argument error");
    }
    let arity_value = small_val(arity);
    if !(-(1i64 << 59)..(1i64 << 59)).contains(&arity_value) {
        return ctx.badarg("argument error");
    }
    // OTP narrows the small integer before checking the maximum export arity.
    let ar = arity_value as u32 as u64;
    if ar > 255 { return FALSE; }
    if f == atom(crate::atoms::intern("module_info")) && (ar == 0 || ar == 1) && crate::sched::module_exists(m) {
        return TRUE;
    }
    if crate::sched::function_exported(m, f, ar) { return TRUE; }
    if let Some(predicate) = eval_entry("defines?", 4) {
        let args = [m, f, small(ar as i64), FALSE];
        let defined = unsafe { crate::sched::tonic_call_fnptr(c, predicate, 4, args.as_ptr()) };
        if defined != FALSE { return defined; }
    }
    if let Some(name) = crate::atoms::name(atom_idx(f)).strip_prefix("MACRO-") {
        if ar == 0 { return FALSE; }
        let name = atom(crate::atoms::intern(name));
        return tn_macro_exported(c, m, name, small(ar as i64 - 1));
    }
    FALSE
}

#[no_mangle]
pub extern "C" fn tn_macro_exported(c: *mut Ctx, module: Term, name: Term, arity: Term) -> Term {
    if !is_atom(module) || !is_atom(name) || !is_small(arity) || !(0..=255).contains(&small_val(arity)) {
        return cx(c).badarg("argument error");
    }
    let (Some(loaded), Some(predicate)) = (eval_entry("loaded?", 1), eval_entry("macro?", 4)) else { return FALSE; };
    let state = unsafe { crate::sched::tonic_call_fnptr(c, loaded, 1, &module) };
    if state != TRUE { return state; }
    let args = [module, name, arity, FALSE];
    unsafe { crate::sched::tonic_call_fnptr(c, predicate, 4, args.as_ptr()) }
}

#[no_mangle]
pub extern "C" fn tn_module_loaded(c: *mut Ctx, m: Term) -> Term {
    if !is_atom(m) { return cx(c).badarg("argument error"); }
    if crate::sched::module_exists(m) { return TRUE; }
    let Some(predicate) = eval_entry("loaded?", 1) else { return FALSE; };
    unsafe { crate::sched::tonic_call_fnptr(c, predicate, 1, &m) }
}

fn list_to_vec(l: Term) -> Option<Vec<Term>> {
    let mut v = Vec::new();
    let mut x = l;
    while is_cons(x) {
        v.push(head(x));
        x = tail(x);
    }
    if x == NIL_LIST {
        Some(v)
    } else {
        None
    }
}

/// apply(fun, args)
#[no_mangle]
pub extern "C" fn tn_apply_fun(c: *mut Ctx, f: Term, args: Term) -> Term {
    let ctx = cx(c);
    let v = match list_to_vec(args) {
        Some(v) => v,
        None => return ctx.badarg("argument error"),
    };
    if !is_closure(f) || closure_arity(f) != v.len() as u64 {
        return tn_bad_fun(c, f, v.as_ptr(), v.len() as u64);
    }
    unsafe { crate::sched::tonic_call_closure(c, f, v.len() as u64, v.as_ptr()) }
}

/// apply(module, fun, args)
/// Shared dispatch for apply, remote calls and external captures. The evaluator
/// is optional: looking it up must never retain it in ordinary programs.
#[no_mangle]
pub extern "C" fn tn_call_mfa(c: *mut Ctx, m: Term, f: Term, args: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    if !is_atom(m) || !is_atom(f) { return ctx.badarg("argument error"); }
    if n > 255 {
        tn_undef_raise(c, m, f, args, n);
        return NONE;
    }
    if let Some(pointer) = crate::sched::lookup_fun(m, f, n) {
        return unsafe { crate::sched::tonic_call_fnptr(c, pointer as *const c_void, n, args) };
    }
    let base = ctx.roots.len();
    ctx.roots.push(m);
    ctx.roots.push(f);
    for i in 0..n as usize { ctx.roots.push(unsafe { *args.add(i) }); }
    let dispatcher = eval_entry("apply", 3);
    let predicate = eval_entry("defines?", 4);
    if let (Some(dispatcher), Some(predicate)) = (dispatcher, predicate) {
        let query = [ctx.roots[base], ctx.roots[base + 1], small(n as i64), FALSE];
        let defined = unsafe { crate::sched::tonic_call_fnptr(c, predicate, 4, query.as_ptr()) };
        if defined == NONE { ctx.truncate(base); return NONE; }
        if defined == TRUE {
            let arguments = ctx.list_from_roots(base + 2, n as usize, NIL_LIST);
            let invoke = [ctx.roots[base], ctx.roots[base + 1], arguments];
            let result = unsafe { crate::sched::tonic_call_fnptr(c, dispatcher, 3, invoke.as_ptr()) };
            ctx.truncate(base);
            return result;
        }
    }
    if n > 0 {
        if let Some(name) = crate::atoms::name(atom_idx(f)).strip_prefix("MACRO-") {
            if let Some(dispatcher) = eval_entry("call_macro", 5) {
                let name = atom(crate::atoms::intern(name));
                let exported = tn_macro_exported(c, m, name, small(n as i64 - 1));
                if exported == NONE { ctx.truncate(base); return NONE; }
                if exported == TRUE {
                    let arguments = ctx.list_from_roots(base + 3, n as usize - 1, NIL_LIST);
                    let invoke = [ctx.roots[base], name, arguments, ctx.roots[base + 2], FALSE];
                    let result = unsafe { crate::sched::tonic_call_fnptr(c, dispatcher, 5, invoke.as_ptr()) };
                    ctx.truncate(base);
                    return result;
                }
            }
        }
    }
    let m = ctx.roots[base];
    let f = ctx.roots[base + 1];
    // the predicate can collect; the original caller's argument array is stale
    let values = ctx.roots[base + 2..base + 2 + n as usize].to_vec();
    tn_undef_raise(c, m, f, values.as_ptr(), n);
    cx(c).truncate(base);
    NONE
}

#[no_mangle]
pub extern "C" fn tn_apply_mfa(c: *mut Ctx, m: Term, f: Term, args: Term) -> Term {
    let Some(values) = list_to_vec(args) else { return cx(c).badarg("argument error"); };
    tn_call_mfa(c, m, f, values.as_ptr(), values.len() as u64)
}

/// `x.field` on a map, or zero-arity remote call when `x` is a module atom.
#[no_mangle]
pub extern "C" fn tn_dot(c: *mut Ctx, x: Term, key: Term) -> Term {
    let ctx = cx(c);
    if is_map(x) {
        return match map::find(x, key) {
            Some(v) => v,
            None => ctx.badkey(key, x),
        };
    }
    if is_atom(x) {
        return tn_call_mfa(c, x, key, std::ptr::null(), 0);
    }
    ctx.badmap(x)
}

/// Access: `x[key]` for maps, keyword lists and nil.
#[no_mangle]
pub extern "C" fn tn_access_get(c: *mut Ctx, x: Term, key: Term) -> Term {
    let ctx = cx(c);
    if is_map(x) {
        if map::find(x, atom(a::STRUCT)).is_some() {
            let s = map::find(x, atom(a::STRUCT)).unwrap();
            return ctx.undef(s, atom(atoms::intern("fetch")), 2);
        }
        return map::find(x, key).unwrap_or(NIL);
    }
    if x == NIL {
        return NIL;
    }
    if is_list(x) {
        if !is_atom(key) {
            return ctx.badarg("the Access calls for keywords expect the key to be an atom");
        }
        let mut l = x;
        while is_cons(l) {
            let t = head(l);
            if is_tuple(t) && tuple_size(t) == 2 && tuple_get(t, 0) == key {
                return tuple_get(t, 1);
            }
            l = tail(l);
        }
        return NIL;
    }
    ctx.badarg("the Access module does not support this data type")
}

// ---------------- exceptions ----------------

/// Take the pending exception as {kind, reason, stacktrace}; returns NONE if
/// the process is being killed (handlers must not run).
#[no_mangle]
pub extern "C" fn tn_exc_get(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    if ctx.kill_reason != 0 {
        return NONE;
    }
    ctx.flush_pending_args();
    let k = ctx.exc_kind;
    let r = ctx.exc_reason;
    let s = ctx.exc_stack;
    let t = ctx.tuple(&[k, r, if s == 0 { NIL_LIST } else { s }]);
    ctx.exc_kind = 0;
    ctx.exc_reason = 0;
    ctx.exc_stack = 0;
    t
}

/// Re-raise a {kind, reason, stack} triple taken with `tn_exc_get`.
#[no_mangle]
pub extern "C" fn tn_exc_reraise(c: *mut Ctx, t: Term) -> Term {
    let ctx = cx(c);
    ctx.exc_kind = tuple_get(t, 0);
    ctx.exc_reason = tuple_get(t, 1);
    ctx.exc_stack = tuple_get(t, 2);
    NONE
}

#[no_mangle]
pub extern "C" fn tn_error(c: *mut Ctx, reason: Term) -> Term {
    cx(c).raise(atom(a::ERROR), reason)
}
#[no_mangle]
pub extern "C" fn tn_throw(c: *mut Ctx, v: Term) -> Term {
    cx(c).raise(atom(a::THROW), v)
}
#[no_mangle]
pub extern "C" fn tn_exit(c: *mut Ctx, v: Term) -> Term {
    cx(c).raise(atom(a::EXIT_KIND), v)
}
#[no_mangle]
pub extern "C" fn tn_raise3(c: *mut Ctx, kind: Term, reason: Term, stack: Term) -> Term {
    let ctx = cx(c);
    // raise captures a stacktrace (may collect): keep `stack` rooted.
    let si = ctx.push(stack);
    ctx.raise(kind, reason);
    ctx.exc_stack = ctx.get(si);
    ctx.truncate(si);
    NONE
}

/// badarg carrying per-argument messages (`%{position => message}`), for
/// Erlang BIFs written in Elixir whose BEAM errors come from erl_erts_errors.
#[no_mangle]
pub extern "C" fn tn_badarg_args(c: *mut Ctx, errs: Term) -> Term {
    let ctx = cx(c);
    let ei = ctx.push(errs);
    ctx.raise_error(atom(a::BADARG));
    ctx.pending_args = ctx.get(ei);
    ctx.truncate(ei);
    NONE
}

#[no_mangle]
pub extern "C" fn tn_raise_case_clause(c: *mut Ctx, v: Term) -> Term {
    cx(c).case_clause(v)
}
#[no_mangle]
pub extern "C" fn tn_raise_match(c: *mut Ctx, v: Term) -> Term {
    cx(c).match_error(v)
}
#[no_mangle]
pub extern "C" fn tn_raise_with_clause(c: *mut Ctx, v: Term) -> Term {
    cx(c).with_clause(v)
}
#[no_mangle]
pub extern "C" fn tn_raise_try_clause(c: *mut Ctx, v: Term) -> Term {
    cx(c).try_clause(v)
}
#[no_mangle]
pub extern "C" fn tn_raise_cond_clause(c: *mut Ctx) -> Term {
    cx(c).cond_clause()
}
#[no_mangle]
pub extern "C" fn tn_raise_badbool(c: *mut Ctx, v: Term, op: Term) -> Term {
    cx(c).bad_boolean(v, op)
}
#[no_mangle]
pub extern "C" fn tn_raise_function_clause(
    c: *mut Ctx,
    m: Term,
    f: Term,
    args: *const Term,
    n: u64,
) -> Term {
    let ctx = cx(c);
    let base = ctx.roots.len();
    for i in 0..n as usize {
        ctx.roots.push(unsafe { *args.add(i) });
    }
    let l = ctx.list_from_roots(base, n as usize, NIL_LIST);
    ctx.roots.truncate(base);
    ctx.function_clause(m, f, n, l)
}

/// Reduction budget exhausted; returns 0 if the process got killed.
#[no_mangle]
pub extern "C" fn tn_yield(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    if ctx.yield_reds() {
        TRUE
    } else {
        NONE
    }
}

/// Generic allocation for compiled code.
#[no_mangle]
pub extern "C" fn tn_alloc(c: *mut Ctx, words: u64) -> *mut u64 {
    cx(c).alloc(words as usize)
}

#[no_mangle]
pub extern "C" fn tn_gc(c: *mut Ctx) -> Term {
    cx(c).gc(0);
    atom(a::OK)
}

#[no_mangle]
pub extern "C" fn tn_heap_size(c: *mut Ctx) -> Term {
    small(cx(c).heap_words as i64)
}

/// Called by compiled code when an Erlang BIF failed: prepend the BIF's own
/// `{m, f, args, [error_info: ...]}` frame unless the runtime already did.
#[no_mangle]
pub extern "C" fn tn_bif_frame(c: *mut Ctx, m: Term, f: Term, n: Term, a0: Term, a1: Term, a2: Term, a3: Term) -> Term {
    let ctx = cx(c);
    let st = ctx.exc_stack;
    if st != 0 && is_cons(st) {
        let top = head(st);
        if is_tuple(top) && tuple_size(top) == 4 && tuple_get(top, 0) == m && tuple_get(top, 1) == f {
            return NONE;
        }
    }
    // Only when the BIF itself raised (not code it called back into).
    if st == 0 || ctx.raise_frame != ctx.frame {
        return NONE;
    }
    let n = (n as usize).min(4);
    let mut args = [a0, a1, a2, a3];
    // elem(tuple, i) failing reports :erlang.element(i + 1, tuple)
    if n == 2 && f == atom(crate::atoms::intern("element")) && is_small(a1) && !is_small(a0) {
        args = [small(small_val(a1) + 1), a0, a2, a3];
    }
    let base = ctx.roots.len();
    for &a in &args[..n] {
        ctx.roots.push(a);
    }
    let al = ctx.list_from_roots(base, n, NIL_LIST);
    ctx.roots.truncate(base);
    ctx.push_top_frame(m, f, al);
    NONE
}

/// The exception module an `:error` reason normalizes to (mirrors
/// ErlangError.normalize/2); used by compiled `rescue` guards.
#[no_mangle]
pub extern "C" fn tn_exc_module(_c: *mut Ctx, r: Term) -> Term {
    use crate::atoms::intern;
    if is_map(r) && map::find(r, atom(a::EXCEPTION)) == Some(TRUE) {
        if let Some(m) = map::find(r, atom(a::STRUCT)) {
            return m;
        }
    }
    let name: &str = if is_atom(r) {
        let i = atom_idx(r);
        if i == a::BADARG {
            "Elixir.ArgumentError"
        } else if i == a::BADARITH {
            "Elixir.ArithmeticError"
        } else {
            match crate::atoms::name(i) {
                "system_limit" => "Elixir.SystemLimitError",
                "cond_clause" => "Elixir.CondClauseError",
                "undef" => "Elixir.UndefinedFunctionError",
                "function_clause" => "Elixir.FunctionClauseError",
                _ => "Elixir.ErlangError",
            }
        }
    } else if is_tuple(r) && tuple_size(r) >= 2 && is_atom(tuple_get(r, 0)) {
        let tag = crate::atoms::name(atom_idx(tuple_get(r, 0)));
        match (tag, tuple_size(r)) {
            ("badarity", 2) => "Elixir.BadArityError",
            ("badfun", 2) => "Elixir.BadFunctionError",
            ("badstruct", 3) => "Elixir.BadStructError",
            ("badmatch", 2) => "Elixir.MatchError",
            ("badmap", 2) => "Elixir.BadMapError",
            ("badbool", 3) => "Elixir.BadBooleanError",
            ("badkey", 2) | ("badkey", 3) => "Elixir.KeyError",
            ("case_clause", 2) => "Elixir.CaseClauseError",
            ("else_clause", 2) => "Elixir.WithClauseError",
            ("try_clause", 2) => "Elixir.TryClauseError",
            ("badarg", 2) => "Elixir.ArgumentError",
            _ => "Elixir.ErlangError",
        }
    } else {
        "Elixir.ErlangError"
    };
    atom(intern(name))
}

/// After an undefined dynamic call: put the arguments in the top stack entry
/// (`{m, f, args, []}`) as the BEAM does.
#[no_mangle]
pub extern "C" fn tn_undef_args(c: *mut Ctx, args: *const Term, n: u64) {
    let ctx = cx(c);
    let base = ctx.roots.len();
    for i in 0..n as usize {
        ctx.roots.push(unsafe { *args.add(i) });
    }
    let l = ctx.list_from_roots(base, n as usize, NIL_LIST);
    ctx.roots.truncate(base);
    attach_undef_args(ctx, l);
}

/// The undef error's top stack entry carries the call's arguments.
fn attach_undef_args(ctx: &mut Ctx, l: Term) {
    let st = ctx.exc_stack;
    if st == 0 || !is_cons(st) {
        return;
    }
    let top = head(st);
    if !(is_tuple(top) && tuple_size(top) == 4 && is_small(tuple_get(top, 2))) {
        return;
    }
    let li = ctx.push(l);
    let st = ctx.exc_stack;
    let top = head(st);
    let e = ctx.tuple(&[tuple_get(top, 0), tuple_get(top, 1), ctx.get(li), tuple_get(top, 3)]);
    let st = ctx.exc_stack;
    let ns = ctx.cons(e, tail(st));
    ctx.exc_stack = ns;
    ctx.truncate(li);
}

/// Function lookup without raising (null when undefined).
#[no_mangle]
pub extern "C" fn tn_lookup_mfa_q(_c: *mut Ctx, m: Term, f: Term, arity: u64) -> *const c_void {
    if !is_atom(m) {
        return std::ptr::null();
    }
    match crate::sched::lookup_fun(m, f, arity) {
        Some(p) => p as *const c_void,
        None => std::ptr::null(),
    }
}

/// Raise undef for `m.f(args...)`; the arguments are copied to the heap
/// (rooted) before raising, since raising may collect.
#[no_mangle]
pub extern "C" fn tn_undef_raise(c: *mut Ctx, m: Term, f: Term, args: *const Term, n: u64) {
    let ctx = cx(c);
    let base = ctx.roots.len();
    for i in 0..n as usize {
        ctx.roots.push(unsafe { *args.add(i) });
    }
    let l = ctx.list_from_roots(base, n as usize, NIL_LIST);
    ctx.roots.truncate(base);
    let li = ctx.push(l);
    let mi = ctx.push(m);
    tn_lookup_mfa(c, ctx.get(mi), f, n);
    let l = ctx.get(li);
    ctx.truncate(li);
    attach_undef_args(ctx, l);
}

static EXT_TRAMPS: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn tn_set_ext_tramps(p: *const usize, n: u64) {
    let v = unsafe { std::slice::from_raw_parts(p, n as usize) }.to_vec();
    let _ = EXT_TRAMPS.set(v);
}

fn ext_fun_info(m: Term, f: Term, arity: u64) -> *const FunInfo {
    use std::collections::HashMap;
    static INFOS: std::sync::OnceLock<parking_lot::Mutex<HashMap<(u64, u64, u64), usize>>> = std::sync::OnceLock::new();
    let mut t = INFOS.get_or_init(|| parking_lot::Mutex::new(HashMap::new())).lock();
    let p = *t.entry((m, f, arity)).or_insert_with(|| {
        Box::leak(Box::new(FunInfo { kind: 1, module: m, name: f, arity, index: 0 })) as *const FunInfo as usize
    });
    p as *const FunInfo
}

/// :erlang.make_fun(m, f, arity): an external fun `&m.f/arity`.
#[no_mangle]
pub extern "C" fn tn_make_ext_fun(c: *mut Ctx, m: Term, f: Term, a: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(m) || !is_atom(f) || !is_small(a) || small_val(a) < 0 {
        return ctx.badarg("argument error");
    }
    let arity = small_val(a) as u64;
    let tr = match EXT_TRAMPS.get().and_then(|v| v.get(arity as usize)) {
        Some(t) => *t,
        None => return ctx.badarg("argument error"),
    };
    let info = ext_fun_info(m, f, arity);
    let p = ctx.alloc(6);
    unsafe {
        *p = header(T_CLOSURE, 5);
        *p.add(1) = tr as u64;
        *p.add(2) = arity;
        *p.add(3) = info as u64;
        *p.add(4) = m;
        *p.add(5) = f;
    }
    p as Term
}
