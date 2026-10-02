//! Term ordering, equality and hashing.

use crate::atoms;
use crate::num::*;
use crate::term::*;
use std::cmp::Ordering;
use std::cmp::Ordering::*;

/// Rank of a term's type in Erlang term order:
/// number < atom < reference < fun < port < pid < tuple < map < list < bitstring
#[inline]
pub fn type_rank(t: Term) -> u8 {
    if is_small(t) {
        return 0;
    }
    if is_atom(t) {
        return 1;
    }
    if is_pid(t) {
        return 5;
    }
    if t == NIL_LIST {
        return 8;
    }
    match boxed_tag(t) {
        T_FLOAT | T_BIGINT => 0,
        T_REF => 2,
        T_CLOSURE => 3,
        T_TUPLE => 6,
        T_MAP => 7,
        T_CONS => 8,
        T_BINARY | T_SUBBIN | T_BITS => 9,
        _ => 10,
    }
}

fn cmp_numbers(a: Term, b: Term, exact: bool) -> Ordering {
    let af = is_float(a);
    let bf = is_float(b);
    match (af, bf) {
        (false, false) => cmp_int(a, b),
        (true, true) => float_val(a).partial_cmp(&float_val(b)).unwrap_or(Equal),
        (false, true) => {
            let o = cmp_int_float(a, float_val(b));
            if o == Equal && exact {
                Less
            } else {
                o
            }
        }
        (true, false) => {
            let o = cmp_int_float(b, float_val(a)).reverse();
            if o == Equal && exact {
                Greater
            } else {
                o
            }
        }
    }
}

pub fn atom_cmp_names(a: Term, b: Term) -> Ordering {
    if a == b {
        return Equal;
    }
    atoms::name(atom_idx(a)).as_bytes().cmp(atoms::name(atom_idx(b)).as_bytes())
}

/// Full term comparison. `exact`: 1 and 1.0 differ (used for ===-consistent ordering).
pub fn compare(a: Term, b: Term) -> Ordering {
    compare_x(a, b, false, false)
}

pub fn compare_exact(a: Term, b: Term) -> Ordering {
    compare_x(a, b, true, false)
}

/// Map key order (OTP 26+ `erts_cmp_flatmap_keys`): two atoms compare by
/// their BEAM atom-table index, anything else by exact term order.
#[inline]
pub fn map_key_cmp(a: Term, b: Term) -> Ordering {
    if a == b {
        return Equal;
    }
    if is_small(a) && is_small(b) {
        return small_val(a).cmp(&small_val(b));
    }
    if is_atom(a) && is_atom(b) {
        if a == b {
            return Equal;
        }
        return atoms::beam_index(atom_idx(a))
            .cmp(&atoms::beam_index(atom_idx(b)))
            .then(atom_idx(a).cmp(&atom_idx(b)));
    }
    compare_x(a, b, true, false)
}

fn compare_x(mut a: Term, mut b: Term, exact: bool, atom_idx_order: bool) -> Ordering {
    loop {
        if a == b {
            return Equal;
        }
        let ra = type_rank(a);
        let rb = type_rank(b);
        if ra != rb {
            return ra.cmp(&rb);
        }
        match ra {
            0 => return cmp_numbers(a, b, exact),
            1 => {
                return if atom_idx_order {
                    atom_idx(a).cmp(&atom_idx(b))
                } else {
                    atom_cmp_names(a, b)
                }
            }
            2 => return word(a, 1).cmp(&word(b, 1)),
            3 => {
                let o = word(a, 1).cmp(&word(b, 1));
                if o != Equal {
                    return o;
                }
                let na = size_of_hdr(hdr(a));
                let nb = size_of_hdr(hdr(b));
                if na != nb {
                    return na.cmp(&nb);
                }
                for i in 4..=na as usize {
                    let o = compare_x(word(a, i), word(b, i), exact, atom_idx_order);
                    if o != Equal {
                        return o;
                    }
                }
                return Equal;
            }
            5 => return pid_id(a).cmp(&pid_id(b)),
            6 => {
                let na = tuple_size(a);
                let nb = tuple_size(b);
                if na != nb {
                    return na.cmp(&nb);
                }
                if na == 0 {
                    return Equal;
                }
                for i in 0..na - 1 {
                    let o = compare_x(tuple_get(a, i), tuple_get(b, i), exact, atom_idx_order);
                    if o != Equal {
                        return o;
                    }
                }
                a = tuple_get(a, na - 1);
                b = tuple_get(b, na - 1);
                continue;
            }
            7 => return crate::map::map_compare(a, b, exact),
            8 => {
                if a == NIL_LIST {
                    return Less;
                }
                if b == NIL_LIST {
                    return Greater;
                }
                let o = compare_x(head(a), head(b), exact, atom_idx_order);
                if o != Equal {
                    return o;
                }
                a = tail(a);
                b = tail(b);
                continue;
            }
            9 => return bit_bytes(a).cmp(bit_bytes(b)).then(bit_len(a).cmp(&bit_len(b))),
            _ => return a.cmp(&b),
        }
    }
}

/// `===`
pub fn eq_exact(a: Term, b: Term) -> bool {
    if a == b {
        return true;
    }
    if !is_ptr(a) || !is_ptr(b) {
        return false;
    }
    compare_exact(a, b) == Equal
}

/// `==`
pub fn eq(a: Term, b: Term) -> bool {
    if a == b {
        return true;
    }
    if !is_ptr(a) && !is_ptr(b) {
        return false;
    }
    compare(a, b) == Equal
}

#[inline]
fn mix(h: u64, v: u64) -> u64 {
    tonic_shared::mix64(h ^ v.wrapping_mul(0x9E3779B97F4A7C15))
}

/// Structural hash consistent with `eq_exact`.
pub fn hash(t: Term) -> u64 {
    hash_x(t, 0x1234_5678)
}

fn hash_x(mut t: Term, mut h: u64) -> u64 {
    loop {
        if is_small(t) {
            return mix(h, t);
        }
        if is_atom(t) {
            // Hash names, so hashes are stable regardless of atom indices.
            let mut x: u64 = 0xcbf29ce484222325;
            for &c in atoms::name(atom_idx(t)).as_bytes() {
                x = (x ^ c as u64).wrapping_mul(0x100000001b3);
            }
            return mix(h, x ^ 0xa70a);
        }
        if !is_ptr(t) {
            return mix(h, t);
        }
        match boxed_tag(t) {
            T_FLOAT => return mix(h, word(t, 1) ^ 0xf10a7),
            T_BIGINT => {
                let n = size_of_hdr(hdr(t)) as usize;
                for i in 1..=n {
                    h = mix(h, word(t, i));
                }
                return h;
            }
            T_BINARY | T_SUBBIN | T_BITS => {
                let mut x: u64 = 0xcbf29ce484222325;
                for &c in bit_bytes(t) {
                    x = (x ^ c as u64).wrapping_mul(0x100000001b3);
                }
                return mix(h, x ^ (bit_len(t) as u64 % 8));
            }
            T_TUPLE => {
                let n = tuple_size(t);
                h = mix(h, 0x7u64 + n as u64);
                if n == 0 {
                    return h;
                }
                for i in 0..n - 1 {
                    h = hash_x(tuple_get(t, i), h);
                }
                t = tuple_get(t, n - 1);
                continue;
            }
            T_CONS => {
                h = hash_x(head(t), mix(h, 0xc0));
                t = tail(t);
                continue;
            }
            T_MAP => {
                let mut acc = mix(h, 0x3a9);
                crate::map::for_each(t, |k, v| {
                    acc = hash_x(v, hash_x(k, acc));
                });
                return acc;
            }
            T_REF => return mix(h, word(t, 1) ^ 0x4ef),
            T_CLOSURE => {
                h = mix(h, word(t, 1));
                let n = size_of_hdr(hdr(t)) as usize;
                for i in 4..=n {
                    h = hash_x(word(t, i), h);
                }
                return h;
            }
            _ => return mix(h, t),
        }
    }
}
