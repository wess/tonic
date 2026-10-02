//! A port of OTP 27's `make_internal_hash` (erl_term_hashing.c), computed over
//! the BEAM encoding of our terms. Large maps (> 32 keys) on the BEAM are HAMTs
//! keyed by this hash, and their iteration order follows it; we reproduce that
//! order so `IO.inspect`/`Enum` over big maps match Elixir exactly.

use crate::atoms;
use crate::term::*;

const C1: u64 = 0x87C37B91114253D5;
const C2: u64 = 0x4CF5AD432745937F;

const H_IMMEDIATE: u64 = 1;
const H_ARRAY_ELEMENT: u64 = 2;
const H_CAR: u64 = 3;
const H_CDR: u64 = 4;
const H_STRING: u64 = 5;
const H_TUPLE: u64 = 6;
const H_FLATMAP: u64 = 7;
const H_BINARY: u64 = 11;
const H_LOCAL_FUN: u64 = 12;
const H_NEG_BIGNUM: u64 = 14;
const H_POS_BIGNUM: u64 = 15;
const H_LOCAL_REF: u64 = 16;
const H_FLOAT: u64 = 20;

const MAX_SMALL: i64 = (1 << 59) - 1;
const MIN_SMALL: i64 = -(1 << 59);

pub fn mix64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51AFD7ED558CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CEB9FE1A85EC53);
    h ^= h >> 33;
    h
}

/// The BEAM word for an immediate term, if `t` is one on the BEAM.
fn beam_immediate(t: Term) -> Option<u64> {
    if is_small(t) {
        let v = small_val(t);
        if (MIN_SMALL..=MAX_SMALL).contains(&v) {
            return Some(((v as u64) << 4) | 0xF);
        }
        return None;
    }
    if is_atom(t) {
        return Some(((atoms::beam_index(atom_idx(t)) as u64) << 6) | 0xB);
    }
    if t == NIL_LIST {
        return Some(0x3B);
    }
    if is_pid(t) {
        return Some((pid_id(t) << 4) | 0x3);
    }
    None
}

enum Item {
    Term(Term),
    CdrMarker,
}

struct H {
    a: u64,
    b: u64,
    ticks: u64,
}

impl H {
    #[inline]
    fn alpha(&mut self, e: u64) {
        let mut e = e.wrapping_mul(C1);
        e = e.rotate_left(31);
        e = e.wrapping_mul(C2);
        self.a ^= e;
        self.a = self.a.rotate_left(27);
        self.a = self.a.wrapping_add(self.b);
        self.a = self.a.wrapping_mul(5).wrapping_add(0x52DCE729);
        self.ticks += 1;
    }
    #[inline]
    fn beta(&mut self, e: u64) {
        let mut e = e.wrapping_mul(C2);
        e = e.rotate_left(33);
        e = e.wrapping_mul(C1);
        self.b ^= e;
        self.b = self.b.rotate_left(31);
        self.b = self.b.wrapping_add(self.a);
        self.b = self.b.wrapping_mul(5).wrapping_add(0x38495AB5);
        self.ticks += 1;
    }
    #[inline]
    fn alpha2(&mut self, x: u64, y: u64) {
        self.alpha((x & 0xffff_ffff) | (y << 32));
    }
    #[inline]
    fn immediate(&mut self, w: u64) {
        self.alpha(H_IMMEDIATE);
        self.beta(w);
    }
    fn push(&mut self, s: &mut Vec<Item>, t: Term) {
        match beam_immediate(t) {
            Some(w) => self.immediate(w),
            None => {
                self.alpha(H_ARRAY_ELEMENT);
                s.push(Item::Term(t));
            }
        }
    }
}

fn is_byte(t: Term) -> bool {
    is_small(t) && (0..256).contains(&small_val(t))
}

fn read_u64(b: &[u8]) -> u64 {
    let mut v = 0u64;
    for (i, &x) in b.iter().take(8).enumerate() {
        v |= (x as u64) << (8 * i);
    }
    v
}

/// `erts_internal_hash/1` (== `erts_map_hash` in release builds).
pub fn internal_hash(t: Term) -> u64 {
    if let Some(w) = beam_immediate(t) {
        return mix64(w);
    }
    make_internal_hash(t, 0)
}

fn make_internal_hash(mut term: Term, salt: u64) -> u64 {
    let mut h = H { a: salt, b: salt, ticks: 0 };
    let mut s: Vec<Item> = Vec::new();
    'outer: loop {
        // Dispatch on the current term.
        if let Some(w) = beam_immediate(term) {
            h.immediate(w);
        } else if is_cons(term) {
            let mut value: u64 = 0;
            let mut bytes: u64 = 0;
            while is_cons(term) {
                let c = head(term);
                if !is_byte(c) {
                    break;
                }
                value = (value << 8) | small_val(c) as u64;
                bytes += 1;
                if bytes % 4 == 0 {
                    h.alpha2(H_STRING | (bytes << 8), value);
                    value = 0;
                    bytes = 0;
                }
                term = tail(term);
            }
            if bytes > 0 {
                h.alpha2(H_STRING | (bytes << 8), value);
            }
            if is_cons(term) {
                let hd = head(term);
                let tl = tail(term);
                if let Some(w) = beam_immediate(hd) {
                    h.alpha2(H_IMMEDIATE, H_CAR);
                    h.beta(w);
                    if !is_cons(tl) {
                        h.alpha(H_CDR);
                    }
                    term = tl;
                } else {
                    s.push(Item::Term(tl));
                    if !is_cons(tl) {
                        s.push(Item::CdrMarker);
                    }
                    h.alpha(H_CAR);
                    term = hd;
                }
            }
            continue 'outer;
        } else if is_small(term) {
            // Integer outside the BEAM small range: a one-digit bignum.
            let v = small_val(term);
            h.alpha2(if v < 0 { H_NEG_BIGNUM } else { H_POS_BIGNUM }, 1);
            h.beta(v.unsigned_abs());
        } else if is_ptr(term) {
            match boxed_tag(term) {
                T_TUPLE => {
                    let n = tuple_size(term);
                    h.alpha(H_TUPLE);
                    h.beta(n as u64);
                    if n > 0 {
                        for i in 0..n - 1 {
                            h.push(&mut s, tuple_get(term, i));
                        }
                        term = tuple_get(term, n - 1);
                        continue 'outer;
                    }
                }
                T_MAP => {
                    // Flatmap layout (keys in flatmap order). Maps used as keys
                    // inside other maps are rare; big ones are approximated.
                    let es = crate::map::entries(term);
                    let n = es.len();
                    h.alpha(H_FLATMAP);
                    h.beta(n as u64);
                    if n > 0 {
                        for &(k, v) in &es[..n - 1] {
                            h.push(&mut s, v);
                            h.push(&mut s, k);
                        }
                        h.push(&mut s, es[n - 1].1);
                        term = es[n - 1].0;
                        continue 'outer;
                    }
                }
                T_BINARY | T_SUBBIN | T_BITS => {
                    let nbits = bit_len(term);
                    let all = bit_bytes(term);
                    let bytes = &all[..nbits / 8];
                    let size = nbits as u64;
                    h.alpha(H_BINARY);
                    h.beta(size);
                    if size > 0 {
                        let mut it = 0;
                        while it + 16 <= bytes.len() {
                            h.alpha(read_u64(&bytes[it..]));
                            h.beta(read_u64(&bytes[it + 8..]));
                            it += 16;
                        }
                        let rem = bytes.len() - it;
                        if rem > 8 {
                            let mut v = 0u64;
                            for k in 8..rem {
                                v ^= (bytes[it + k] as u64) << (8 * (k - 8));
                            }
                            v = v.wrapping_mul(C2);
                            v = v.rotate_left(33);
                            v = v.wrapping_mul(C1);
                            h.b ^= v;
                        }
                        if rem > 0 {
                            let mut v = 0u64;
                            for k in 0..rem.min(8) {
                                v ^= (bytes[it + k] as u64) << (8 * k);
                            }
                            v = v.wrapping_mul(C1);
                            v = v.rotate_left(31);
                            v = v.wrapping_mul(C2);
                            h.a ^= v;
                        }
                        if nbits % 8 != 0 {
                            h.alpha((all[nbits / 8] >> (8 - nbits % 8)) as u64);
                        }
                    }
                }
                T_BIGINT => {
                    let n = size_of_hdr(hdr(term)) as usize - 1;
                    let neg = word(term, 1) != 0;
                    h.alpha2(if neg { H_NEG_BIGNUM } else { H_POS_BIGNUM }, n as u64);
                    let mut i = 0;
                    while i + 2 <= n {
                        h.alpha(word(term, 2 + i));
                        h.beta(word(term, 3 + i));
                        i += 2;
                    }
                    if i < n {
                        h.beta(word(term, 2 + i));
                    }
                }
                T_FLOAT => {
                    h.alpha(H_FLOAT);
                    h.beta(word(term, 1));
                }
                T_REF => {
                    h.alpha2(H_LOCAL_REF, word(term, 1));
                    h.beta(0);
                }
                T_CLOSURE => {
                    h.alpha2(H_LOCAL_FUN, 0);
                    h.beta(word(term, 1));
                }
                _ => h.immediate(term),
            }
        } else {
            h.immediate(term);
        }
        // pop_next
        loop {
            match s.pop() {
                None => {
                    let ticks = h.ticks;
                    let (mut a, mut b) = (h.a ^ ticks, h.b ^ ticks);
                    a = a.wrapping_add(b);
                    b = b.wrapping_add(a);
                    a = mix64(a);
                    b = mix64(b);
                    a = a.wrapping_add(b);
                    b = b.wrapping_add(a);
                    return a ^ b;
                }
                Some(Item::CdrMarker) => {
                    h.beta(H_CDR);
                    match s.pop() {
                        Some(Item::Term(t)) => {
                            term = t;
                            continue 'outer;
                        }
                        _ => unreachable!(),
                    }
                }
                Some(Item::Term(t)) => {
                    term = t;
                    continue 'outer;
                }
            }
        }
    }
}

/// HAMT iteration key: nibbles taken least-significant first.
#[inline]
pub fn hamt_order_key(h: u64) -> u64 {
    let mut r = 0u64;
    let mut x = h;
    for _ in 0..16 {
        r = (r << 4) | (x & 0xf);
        x >>= 4;
    }
    r
}
