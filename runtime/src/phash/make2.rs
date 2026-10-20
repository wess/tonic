//! `make_hash2`: Bob Jenkins' mix over 32-bit words, walking the term with an
//! explicit stack exactly as the C does so nested terms combine in the same
//! order.

use super::{atom_hval, bits_of, int_digits};
use crate::term::*;

const HCONST: u32 = 0x9e37_79b9;
const HCONST_2: u32 = 0x3c6e_f372;
const HCONST_3: u32 = 0xdaa6_6d2b;
const HCONST_4: u32 = 0x78dd_e6e4;
const HCONST_5: u32 = 0x1715_609d;
const HCONST_7: u32 = 0x5384_540f;
const HCONST_9: u32 = 0x8ff3_4781;
const HCONST_10: u32 = 0x2e2a_c13a;
const HCONST_11: u32 = 0xcc62_3af3;
const HCONST_12: u32 = 0x6a99_b4ac;
const HCONST_13: u32 = 0x08d1_2e65;
const HCONST_14: u32 = 0xa708_a81e;
const HCONST_15: u32 = 0x4540_21d7;
const HCONST_16: u32 = 0xe377_9b90;
const HCONST_19: u32 = 0xbe1e_08bb;

/// OTP's NIL_DEF tag value, hashed for `[]` when it is not the first term.
const NIL_DEF: u32 = 2;

fn mix3(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 13);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 8);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 13);
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 12);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 16);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 5);
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 3);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 10);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 15);
    (a, b, c)
}

fn mix(a: u32, b: u32, c: u32) -> u32 {
    mix3(a, b, c).2
}

/// UINT32_HASH_2
fn h2(hash: u32, x: u32, y: u32, k: u32) -> u32 {
    mix(k.wrapping_add(x), k.wrapping_add(y), hash)
}

/// UINT32_HASH
fn h1(hash: u32, x: u32, k: u32) -> u32 {
    h2(hash, x, 0, k)
}

fn word32(k: &[u8]) -> u32 {
    u32::from_le_bytes([k[0], k[1], k[2], k[3]])
}

/// `block_hash`: 12 bytes per round, the tail folded in with the length.
fn block_hash(bytes: &[u8], init: u32) -> u32 {
    let (mut a, mut b, mut c) = (HCONST, HCONST, init);
    let mut k = bytes;
    while k.len() >= 12 {
        a = a.wrapping_add(word32(k));
        b = b.wrapping_add(word32(&k[4..]));
        c = c.wrapping_add(word32(&k[8..]));
        (a, b, c) = mix3(a, b, c);
        k = &k[12..];
    }
    c = c.wrapping_add(bytes.len() as u32);
    let sh = |i: usize, s: u32| (k[i] as u32) << s;
    let n = k.len();
    // The first byte of c is reserved for the length.
    for i in (8..n).rev() {
        c = c.wrapping_add(sh(i, 8 * (i as u32 - 7)));
    }
    for i in (4..n.min(8)).rev() {
        b = b.wrapping_add(sh(i, 8 * (i as u32 - 4)));
    }
    for i in (0..n.min(4)).rev() {
        a = a.wrapping_add(sh(i, 8 * i as u32));
    }
    mix(a, b, c)
}

enum Op {
    Term(Term),
    Pair,
    Tail(u32, u32),
}

fn is_byte(t: Term) -> bool {
    is_small(t) && (0..256).contains(&small_val(t))
}

fn hash_int(hash: u32, t: Term) -> u32 {
    if is_small(t) {
        let v = small_val(t);
        if (-(1 << 27)..1 << 27).contains(&v) {
            // SINT32_HASH: negative numbers are mixed twice.
            let y = v as i32;
            let hash = if y < 0 { h1(hash, y.wrapping_neg() as u32, HCONST) } else { hash };
            return h1(hash, y as u32, HCONST);
        }
    }
    let mut hash = hash;
    let mut digits = Vec::new();
    let neg = int_digits(t, |d| digits.push(d));
    let con = if neg { HCONST_10 } else { HCONST_11 };
    for d in digits {
        hash = h2(hash, d as u32, (d >> 32) as u32, con);
    }
    hash
}

pub fn make_hash2(t: Term) -> u32 {
    let mut hash: u32 = 0;
    let mut xor: u32 = 0;
    let mut s: Vec<Op> = Vec::new();
    let mut term = t;
    loop {
        // Each arm either continues with a child term or falls through to
        // pop the next pending item (OTP's `hash2_common`).
        if is_cons(term) {
            let (mut c, mut sh) = (0u32, 0u32);
            let mut p = term;
            while is_byte(head(p)) {
                sh = (sh << 8).wrapping_add(small_val(head(p)) as u32);
                if c == 3 {
                    hash = h1(hash, sh, HCONST_4);
                    c = 0;
                    sh = 0;
                } else {
                    c += 1;
                }
                term = tail(p);
                if !is_cons(term) {
                    break;
                }
                p = term;
            }
            if c > 0 {
                hash = h1(hash, sh, HCONST_4);
            }
            if is_cons(term) {
                s.push(Op::Term(tail(p)));
                term = head(p);
            }
            continue;
        }
        if is_integer(term) {
            hash = hash_int(hash, term);
        } else if is_atom(term) {
            let v = atom_hval(term);
            hash = if hash == 0 { v } else { h1(hash, v, HCONST_3) };
        } else if term == NIL_LIST {
            hash = if hash == 0 { 3_468_870_702 } else { h1(hash, NIL_DEF, HCONST_2) };
        } else if is_pid(term) {
            hash = h1(hash, pid_id(term) as u32, HCONST_5);
        } else if is_ptr(term) {
            match boxed_tag(term) {
                T_TUPLE => {
                    let n = tuple_size(term);
                    hash = h1(hash, n as u32, HCONST_9);
                    if n > 0 {
                        for i in (1..n).rev() {
                            s.push(Op::Term(tuple_get(term, i)));
                        }
                        term = tuple_get(term, 0);
                        continue;
                    }
                }
                T_MAP => {
                    let n = crate::map::size(term);
                    hash = h1(hash, n as u32, HCONST_16);
                    if n > 0 {
                        // Pairs hash independently and xor together, so
                        // iteration order does not matter.
                        s.push(Op::Tail(hash, xor));
                        hash = 0;
                        xor = 0;
                        crate::map::for_each(term, |k, v| {
                            s.push(Op::Pair);
                            s.push(Op::Term(v));
                            s.push(Op::Term(k));
                        });
                    }
                }
                T_BINARY | T_SUBBIN | T_BITS => {
                    let con = HCONST_13.wrapping_add(hash);
                    let (bytes, tail_bits, last) = bits_of(term);
                    if bytes.is_empty() && tail_bits == 0 {
                        hash = con;
                    } else {
                        hash = block_hash(bytes, con);
                        if tail_bits > 0 {
                            hash = h2(hash, tail_bits, last as u32, HCONST_15);
                        }
                    }
                }
                T_FLOAT => {
                    let f = float_val(term);
                    let w = if f == 0.0 { 0 } else { f.to_bits() };
                    hash = h2(hash, (w >> 32) as u32, w as u32, HCONST_12);
                }
                T_REF => hash = h1(hash, word(term, 1) as u32, HCONST_7),
                T_CLOSURE => {
                    let fun = super::fun_parts(term);
                    if fun.external {
                        hash = h2(hash, fun.arity, atom_hval(fun.module), HCONST);
                        hash = h1(hash, atom_hval(fun.name), HCONST_14);
                    } else {
                        let env = super::fun_env(term);
                        hash = h2(hash, env.len() as u32, atom_hval(fun.module), HCONST);
                        hash = h2(hash, fun.index, fun.uniq, HCONST);
                        if let Some((&first, rest)) = env.split_first() {
                            for &e in rest.iter().rev() {
                                s.push(Op::Term(e));
                            }
                            term = first;
                            continue;
                        }
                    }
                }
                _ => {}
            }
        }
        loop {
            match s.pop() {
                None => return hash,
                Some(Op::Term(t)) => {
                    term = t;
                    break;
                }
                Some(Op::Pair) => {
                    xor ^= hash;
                    hash = 0;
                }
                Some(Op::Tail(h, x)) => {
                    hash = h1(h, xor, HCONST_19);
                    xor = x;
                }
            }
        }
    }
}
