//! `make_hash`: the legacy multiplicative hash behind `erlang:phash/2`.

use super::{atom_hval, bits_of, fun_env, fun_parts, int_digits, make_hash2};
use crate::term::*;

// Primes just above 2^28.
const F1: u32 = 268_440_163;
const F2: u32 = 268_439_161;
const F3: u32 = 268_435_459;
const F4: u32 = 268_436_141;
const F5: u32 = 268_438_633;
const F6: u32 = 268_437_017;
const F8: u32 = 268_437_511;
const F9: u32 = 268_439_627;
const F10: u32 = 268_440_479;
const F11: u32 = 268_440_577;
const F12: u32 = 268_440_581;
const F13: u32 = 268_440_593;
const F14: u32 = 268_440_611;

enum Op {
    Term(Term),
    /// A list tail still to visit after a non-byte head.
    Cdr(Term),
    /// The multiply that closes a list once its tail is hashed.
    CdrPost,
    TupleEnd(u32),
}

/// UINT32_HASH_STEP: the word's bytes, least significant first.
fn step(hash: u32, x: u32, p: u32) -> u32 {
    (0..4).fold(hash, |h, i| h.wrapping_mul(p).wrapping_add((x >> (8 * i)) & 0xFF))
}

fn mul(h: u32, p: u32, x: u32) -> u32 {
    h.wrapping_mul(p).wrapping_add(x)
}

/// Integers hash their magnitude bytewise; the top digit gives only 4 bytes
/// when its upper half is zero, which makes smalls and bignums agree.
fn hash_int(mut hash: u32, t: Term) -> u32 {
    let mut digits = Vec::new();
    let neg = int_digits(t, |d| digits.push(d));
    let last = digits.len() - 1;
    for (i, &d) in digits.iter().enumerate() {
        let n = if i == last && d >> 32 == 0 { 4 } else { 8 };
        for j in 0..n {
            hash = mul(hash, F2, ((d >> (8 * j)) & 0xFF) as u32);
        }
    }
    hash.wrapping_mul(if neg { F4 } else { F3 })
}

fn hash_bits(mut hash: u32, t: Term) -> u32 {
    let (bytes, tail_bits, last) = bits_of(t);
    for &b in bytes {
        hash = mul(hash, F1, b as u32);
    }
    if tail_bits > 0 {
        hash = mul(mul(hash, F1, last as u32), F12, tail_bits);
    }
    mul(hash, F4, bytes.len() as u32)
}

fn is_byte(t: Term) -> bool {
    is_small(t) && (0..256).contains(&small_val(t))
}

pub fn make_hash(t: Term) -> u32 {
    let mut hash: u32 = 0;
    let mut s: Vec<Op> = Vec::new();
    let mut op = Op::Term(t);
    loop {
        match op {
            Op::CdrPost => hash = hash.wrapping_mul(F8),
            Op::TupleEnd(n) => hash = mul(hash, F9, n),
            Op::Cdr(tl) if !is_cons(tl) => {
                s.push(Op::CdrPost);
                s.push(Op::Term(tl));
            }
            Op::Cdr(l) | Op::Term(l) if is_cons(l) => {
                // Byte runs fold in with F2 alone, unlike integer elements.
                let mut l = l;
                while is_byte(head(l)) {
                    hash = mul(hash, F2, small_val(head(l)) as u32);
                    if !is_cons(tail(l)) {
                        break;
                    }
                    l = tail(l);
                }
                if is_byte(head(l)) {
                    s.push(Op::CdrPost);
                    s.push(Op::Term(tail(l)));
                } else {
                    s.push(Op::Cdr(tail(l)));
                    s.push(Op::Term(head(l)));
                }
            }
            Op::Cdr(_) => unreachable!(),
            Op::Term(t) => {
                if is_integer(t) {
                    hash = hash_int(hash, t);
                } else if is_atom(t) {
                    hash = mul(hash, F1, atom_hval(t));
                } else if t == NIL_LIST {
                    hash = mul(hash, F3, 1);
                } else if is_pid(t) {
                    hash = step(hash, pid_id(t) as u32, F5).wrapping_mul(F6);
                } else if is_ptr(t) {
                    match boxed_tag(t) {
                        T_TUPLE => {
                            let n = tuple_size(t);
                            s.push(Op::TupleEnd(n as u32));
                            for i in (0..n).rev() {
                                s.push(Op::Term(tuple_get(t, i)));
                            }
                        }
                        T_MAP => hash = mul(hash, F13, F14).wrapping_add(make_hash2(t)),
                        T_BINARY | T_SUBBIN | T_BITS => hash = hash_bits(hash, t),
                        T_FLOAT => {
                            let f = float_val(t);
                            let w = if f == 0.0 { 0 } else { f.to_bits() };
                            hash = mul(hash, F6, (w as u32) ^ (w >> 32) as u32);
                        }
                        T_REF => hash = step(hash, word(t, 1) as u32, F9).wrapping_mul(F10),
                        T_CLOSURE => {
                            let fun = fun_parts(t);
                            if fun.external {
                                hash = mul(hash, F11, fun.arity);
                                hash = mul(hash, F1, atom_hval(fun.module));
                                hash = mul(hash, F1, atom_hval(fun.name));
                            } else {
                                let env = fun_env(t);
                                hash = mul(hash, F10, env.len() as u32);
                                hash = mul(hash, F1, atom_hval(fun.module));
                                hash = mul(hash, F2, fun.index);
                                hash = mul(hash, F2, fun.uniq);
                                for &e in env.iter().rev() {
                                    s.push(Op::Term(e));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        match s.pop() {
            Some(next) => op = next,
            None => return hash,
        }
    }
}
