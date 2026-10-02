//! Argument-order adapters and small extras (Erlang-compatible signatures).

use crate::atoms;
use crate::bif_core::*;
use crate::heap::Ctx;
use crate::term::*;

#[no_mangle]
pub extern "C" fn tn_is_map_key_erl(c: *mut Ctx, k: Term, m: Term) -> Term {
    tn_is_map_key(c, m, k)
}
#[no_mangle]
pub extern "C" fn tn_map_get_erl(c: *mut Ctx, k: Term, m: Term) -> Term {
    tn_map_get(c, k, m)
}
#[no_mangle]
pub extern "C" fn tn_map_get3_erl(c: *mut Ctx, k: Term, m: Term, d: Term) -> Term {
    tn_map_get3(c, m, k, d)
}
#[no_mangle]
pub extern "C" fn tn_map_fetch_erl(c: *mut Ctx, k: Term, m: Term) -> Term {
    tn_map_fetch(c, m, k)
}
#[no_mangle]
pub extern "C" fn tn_map_put_erl(c: *mut Ctx, k: Term, v: Term, m: Term) -> Term {
    tn_map_put(c, m, k, v)
}
#[no_mangle]
pub extern "C" fn tn_map_remove_erl(c: *mut Ctx, k: Term, m: Term) -> Term {
    tn_map_remove(c, m, k)
}
#[no_mangle]
pub extern "C" fn tn_error2(c: *mut Ctx, reason: Term, args: Term) -> Term {
    tn_error3(c, reason, args, NIL_LIST)
}

/// erlang:error(Reason, Args, Options): the caller's stack entry carries
/// `Args` (unless :none) and `Options` (e.g. error_info) in its location.
#[no_mangle]
pub extern "C" fn tn_error3(c: *mut Ctx, reason: Term, args: Term, opts: Term) -> Term {
    let ctx = cx(c);
    let ai = ctx.push(args);
    let oi = ctx.push(opts);
    ctx.raise(atom(a::ERROR), reason);
    let st = ctx.exc_stack;
    if st != 0 && is_cons(st) {
        let top = head(st);
        if is_tuple(top) && tuple_size(top) == 4 {
            let args = ctx.get(ai);
            let a2 = if is_list(args) { args } else { tuple_get(top, 2) };
            // location ++ opts
            let mut items: Vec<Term> = Vec::new();
            let mut l = tuple_get(top, 3);
            while is_cons(l) {
                items.push(head(l));
                l = tail(l);
            }
            let mut o = ctx.get(oi);
            while is_cons(o) {
                items.push(head(o));
                o = tail(o);
            }
            let base = ctx.roots.len();
            for it in &items {
                ctx.roots.push(*it);
            }
            let loc = ctx.list_from_roots(base, items.len(), NIL_LIST);
            ctx.roots.truncate(base);
            let li = ctx.push(loc);
            let top = head(ctx.exc_stack);
            let e = ctx.tuple(&[tuple_get(top, 0), tuple_get(top, 1), if is_list(ctx.get(ai)) { ctx.get(ai) } else { a2 }, ctx.get(li)]);
            let rest = tail(ctx.exc_stack);
            let ns = ctx.cons(e, rest);
            ctx.exc_stack = ns;
        }
    }
    ctx.truncate(ai);
    NONE
}
#[no_mangle]
pub extern "C" fn tn_phash2_1(c: *mut Ctx, t: Term) -> Term {
    tn_phash2(c, t, NIL)
}
#[no_mangle]
pub extern "C" fn tn_whereis_erl(c: *mut Ctx, n: Term) -> Term {
    let r = crate::bif_proc::tn_whereis(c, n);
    if r == NIL {
        atom(a::UNDEFINED)
    } else {
        r
    }
}
#[no_mangle]
pub extern "C" fn tn_dict_get1(c: *mut Ctx, k: Term) -> Term {
    let r = crate::bif_proc::tn_dict_get(c, k, NONE_MARK);
    if r == NONE_MARK {
        atom(a::UNDEFINED)
    } else {
        r
    }
}
const NONE_MARK: Term = TIMEOUT_MARK;

#[no_mangle]
pub extern "C" fn tn_integer_to_binary10(c: *mut Ctx, i: Term) -> Term {
    tn_integer_to_binary(c, i, small(10))
}
#[no_mangle]
pub extern "C" fn tn_binary_to_integer10(c: *mut Ctx, b: Term) -> Term {
    tn_binary_to_integer(c, b, small(10))
}
#[no_mangle]
pub extern "C" fn tn_integer_to_list10(c: *mut Ctx, i: Term) -> Term {
    crate::bif_bin::tn_integer_to_list(c, i, small(10))
}
#[no_mangle]
pub extern "C" fn tn_atom_to_list(c: *mut Ctx, a_: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(a_) {
        return ctx.badarg_args(&[(1, "not an atom")]);
    }
    let s = atoms::name(atom_idx(a_));
    let mut v: Vec<Term> = s.chars().map(|ch| small(ch as i64)).collect();
    ctx.list_from_vec(&mut v)
}
#[no_mangle]
pub extern "C" fn tn_list_to_atom(c: *mut Ctx, l: Term) -> Term {
    let b = crate::bif_bin::tn_chardata_to_binary(c, l);
    if b == NONE {
        return NONE;
    }
    tn_binary_to_atom(c, b)
}
#[no_mangle]
pub extern "C" fn tn_list_to_integer(c: *mut Ctx, l: Term) -> Term {
    let b = crate::bif_bin::tn_chardata_to_binary(c, l);
    if b == NONE {
        return NONE;
    }
    tn_binary_to_integer(c, b, small(10))
}
#[no_mangle]
pub extern "C" fn tn_math_sqrt(c: *mut Ctx, x: Term) -> Term {
    crate::bif_misc::tn_math1(c, small(0), x)
}
#[no_mangle]
pub extern "C" fn tn_math_pow(c: *mut Ctx, x: Term, y: Term) -> Term {
    crate::bif_misc::tn_math2(c, small(0), x, y)
}
#[no_mangle]
pub extern "C" fn tn_math_pi(c: *mut Ctx) -> Term {
    cx(c).float(std::f64::consts::PI)
}

/// Kernel.inspect helper for atoms used as module names etc.
#[no_mangle]
pub extern "C" fn tn_atom_length(c: *mut Ctx, a_: Term) -> Term {
    let _ = c;
    small(atoms::name(atom_idx(a_)).len() as i64)
}

/// String.to_atom for charlists/binaries
#[no_mangle]
pub extern "C" fn tn_to_atom(c: *mut Ctx, b: Term) -> Term {
    tn_binary_to_atom(c, b)
}

/// Monitor with explicit type (erlang:monitor(process, Pid))
#[no_mangle]
pub extern "C" fn tn_monitor2(c: *mut Ctx, _ty: Term, p: Term) -> Term {
    crate::bif_proc::tn_monitor(c, p)
}

/// Unique integer (System.unique_integer)
#[no_mangle]
pub extern "C" fn tn_unique_integer(c: *mut Ctx) -> Term {
    let _ = c;
    small(crate::sched::next_ref_id() as i64)
}

/// Pretty much :erlang.term_to_binary is not supported; identity marker.
#[no_mangle]
pub extern "C" fn tn_term_equal_hash(_c: *mut Ctx, t: Term) -> Term {
    small((crate::cmp::hash(t) >> 2) as i64)
}

/// Sort a list with the runtime comparator in descending order.
#[no_mangle]
pub extern "C" fn tn_sort_desc_list(c: *mut Ctx, l: Term) -> Term {
    tn_sort_desc(c, l)
}

/// :erlang.register(name, pid)
#[no_mangle]
pub extern "C" fn tn_register_erl(c: *mut Ctx, name: Term, p: Term) -> Term {
    crate::bif_proc::tn_register(c, p, name)
}

/// :erlang.send_after(time, dest, msg)
#[no_mangle]
pub extern "C" fn tn_send_after_erl(c: *mut Ctx, time: Term, dest: Term, msg: Term) -> Term {
    crate::bif_proc::tn_send_after(c, dest, msg, time)
}

/// :erlang.insert_element(index1, tuple, term)
#[no_mangle]
pub extern "C" fn tn_insert_element_erl(c: *mut Ctx, i: Term, t: Term, v: Term) -> Term {
    if is_small(i) {
        return tn_tuple_insert_at(c, t, small(small_val(i) - 1), v);
    }
    cx(c).badarg_element(i, t, 1, 2)
}

/// :erlang.delete_element(index1, tuple)
#[no_mangle]
pub extern "C" fn tn_delete_element_erl(c: *mut Ctx, i: Term, t: Term) -> Term {
    if is_small(i) {
        return tn_tuple_delete_at(c, t, small(small_val(i) - 1));
    }
    cx(c).badarg_element(i, t, 1, 2)
}
