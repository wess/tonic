//! Persistent maps, laid out like the BEAM's.
//!
//! Layout:  MAP  = [hdr, count(raw), root(0 | FLAT | HAMT)]
//!          FLAT = T_MAPNODE [hdr, k0, v0, k1, v1, ...]   keys in map-key order
//!          HAMT = T_HAMT    [hdr, bitmap(small int), child...]
//!
//! Maps of up to 32 keys are flat (OTP's flatmap: keys sorted in map-key
//! order, atoms by atom-table index). Larger maps are HAMTs keyed by
//! `erts_internal_hash` four bits per level, least significant nibble first,
//! exactly like OTP's hashmaps, so their iteration order matches the BEAM.
//! A HAMT child is either a subnode or a FLAT leaf holding one key/value
//! pair (more only on a full 64-bit hash collision).

use crate::beamhash::internal_hash;
use crate::cmp::map_key_cmp;
use crate::heap::Ctx;
use crate::term::*;
use std::cmp::Ordering::*;

#[repr(C, align(8))]
pub struct StaticMap {
    pub h: u64,
    pub count: u64,
    pub root: u64,
}
pub static EMPTY_MAP: StaticMap = StaticMap {
    h: header(T_MAP, 2),
    count: 0,
    root: 0,
};
pub fn empty() -> Term {
    &EMPTY_MAP as *const _ as u64
}

/// Above this many keys the BEAM stores a map as a HAMT.
pub const SMALL_MAP_LIMIT: usize = 32;
const LEVELS: u32 = 16;

#[inline]
pub fn size(m: Term) -> usize {
    word(m, 1) as usize
}
#[inline]
fn root(m: Term) -> Term {
    word(m, 2)
}
#[inline]
fn nwords(n: Term) -> usize {
    size_of_hdr(hdr(n)) as usize
}
#[inline]
fn is_hamt(n: Term) -> bool {
    tag_of_hdr(hdr(n)) == T_HAMT
}
/// Pairs in a FLAT node.
#[inline]
fn flat_len(n: Term) -> usize {
    nwords(n) / 2
}
#[inline]
fn fkey(n: Term, i: usize) -> Term {
    word(n, 1 + 2 * i)
}
#[inline]
fn fval(n: Term, i: usize) -> Term {
    word(n, 2 + 2 * i)
}
#[inline]
fn bitmap(n: Term) -> u32 {
    small_val(word(n, 1)) as u32
}
#[inline]
fn child(n: Term, i: usize) -> Term {
    word(n, 2 + i)
}
#[inline]
fn nib(h: u64, lvl: u32) -> u32 {
    ((h >> (4 * lvl)) & 0xf) as u32
}

/// Index of `k` in a FLAT node: Ok(i) when present, Err(insertion point).
fn flat_search(n: Term, k: Term) -> Result<usize, usize> {
    let len = flat_len(n);
    if len <= 8 {
        for i in 0..len {
            match map_key_cmp(k, fkey(n, i)) {
                Equal => return Ok(i),
                Less => return Err(i),
                Greater => {}
            }
        }
        return Err(len);
    }
    let (mut lo, mut hi) = (0, len);
    while lo < hi {
        let mid = (lo + hi) / 2;
        match map_key_cmp(k, fkey(n, mid)) {
            Equal => return Ok(mid),
            Less => hi = mid,
            Greater => lo = mid + 1,
        }
    }
    Err(lo)
}

pub fn find(m: Term, k: Term) -> Option<Term> {
    let r = root(m);
    if r == 0 {
        return None;
    }
    if !is_hamt(r) {
        return flat_search(r, k).ok().map(|i| fval(r, i));
    }
    let h = internal_hash(k);
    let mut n = r;
    let mut lvl = 0;
    loop {
        let bit = 1u32 << nib(h, lvl);
        let bm = bitmap(n);
        if bm & bit == 0 {
            return None;
        }
        let c = child(n, (bm & (bit - 1)).count_ones() as usize);
        if is_hamt(c) {
            n = c;
            lvl += 1;
        } else {
            return flat_search(c, k).ok().map(|i| fval(c, i));
        }
    }
}

// ---------------------------------------------------------------------------
// Allocation helpers (callers have reserved).

fn alloc(ctx: &mut Ctx, tag: u64, payload: usize) -> *mut u64 {
    let p = ctx.alloc_nogc(1 + payload);
    unsafe { *p = header(tag, payload as u64) };
    p
}

fn new_map(ctx: &mut Ctx, count: usize, root: Term) -> Term {
    if count == 0 {
        return empty();
    }
    let p = ctx.alloc_nogc(3);
    unsafe {
        *p = header(T_MAP, 2);
        *p.add(1) = count as u64;
        *p.add(2) = root;
    }
    p as Term
}

fn leaf(ctx: &mut Ctx, k: Term, v: Term) -> Term {
    let p = alloc(ctx, T_MAPNODE, 2);
    unsafe {
        *p.add(1) = k;
        *p.add(2) = v;
    }
    p as Term
}

/// Copy of FLAT `n` with the value at `i` replaced.
fn flat_set(ctx: &mut Ctx, n: Term, i: usize, v: Term) -> Term {
    let w = nwords(n);
    let p = alloc(ctx, T_MAPNODE, w);
    unsafe {
        std::ptr::copy_nonoverlapping((n as *const u64).add(1), p.add(1), w);
        *p.add(2 + 2 * i) = v;
    }
    p as Term
}

/// Copy of FLAT `n` with (k, v) inserted at pair index `i`.
fn flat_insert(ctx: &mut Ctx, n: Term, i: usize, k: Term, v: Term) -> Term {
    let len = if n == 0 { 0 } else { flat_len(n) };
    let p = alloc(ctx, T_MAPNODE, 2 * len + 2);
    unsafe {
        for j in 0..i {
            *p.add(1 + 2 * j) = fkey(n, j);
            *p.add(2 + 2 * j) = fval(n, j);
        }
        *p.add(1 + 2 * i) = k;
        *p.add(2 + 2 * i) = v;
        for j in i..len {
            *p.add(3 + 2 * j) = fkey(n, j);
            *p.add(4 + 2 * j) = fval(n, j);
        }
    }
    p as Term
}

/// Copy of FLAT `n` without pair `i`.
fn flat_delete(ctx: &mut Ctx, n: Term, i: usize) -> Term {
    let len = flat_len(n);
    let p = alloc(ctx, T_MAPNODE, 2 * len - 2);
    unsafe {
        let mut o = 1;
        for j in 0..len {
            if j != i {
                *p.add(o) = fkey(n, j);
                *p.add(o + 1) = fval(n, j);
                o += 2;
            }
        }
    }
    p as Term
}

/// Copy of HAMT node `n` with child slot `pos` replaced.
fn hamt_set(ctx: &mut Ctx, n: Term, pos: usize, c: Term) -> Term {
    let w = nwords(n);
    let p = alloc(ctx, T_HAMT, w);
    unsafe {
        std::ptr::copy_nonoverlapping((n as *const u64).add(1), p.add(1), w);
        *p.add(2 + pos) = c;
    }
    p as Term
}

/// Copy of HAMT node `n` with a child for `bit` inserted at `pos`.
fn hamt_insert(ctx: &mut Ctx, n: Term, bit: u32, pos: usize, c: Term) -> Term {
    let cnt = nwords(n) - 1;
    let p = alloc(ctx, T_HAMT, cnt + 2);
    unsafe {
        *p.add(1) = small((bitmap(n) | bit) as i64);
        for j in 0..pos {
            *p.add(2 + j) = child(n, j);
        }
        *p.add(2 + pos) = c;
        for j in pos..cnt {
            *p.add(3 + j) = child(n, j);
        }
    }
    p as Term
}

/// Copy of HAMT node `n` without the child for `bit` at `pos`.
fn hamt_remove(ctx: &mut Ctx, n: Term, bit: u32, pos: usize) -> Term {
    let cnt = nwords(n) - 1;
    let p = alloc(ctx, T_HAMT, cnt);
    unsafe {
        *p.add(1) = small((bitmap(n) & !bit) as i64);
        let mut o = 2;
        for j in 0..cnt {
            if j != pos {
                *p.add(o) = child(n, j);
                o += 1;
            }
        }
    }
    p as Term
}

/// A subtree at `lvl` holding two leaves with different hashes.
fn split(ctx: &mut Ctx, a: Term, ha: u64, b: Term, hb: u64, lvl: u32) -> Term {
    let (na, nb) = (nib(ha, lvl), nib(hb, lvl));
    if na == nb {
        let sub = split(ctx, a, ha, b, hb, lvl + 1);
        let p = alloc(ctx, T_HAMT, 2);
        unsafe {
            *p.add(1) = small((1u32 << na) as i64);
            *p.add(2) = sub;
        }
        return p as Term;
    }
    let p = alloc(ctx, T_HAMT, 3);
    let (first, second) = if na < nb { (a, b) } else { (b, a) };
    unsafe {
        *p.add(1) = small(((1u32 << na) | (1u32 << nb)) as i64);
        *p.add(2) = first;
        *p.add(3) = second;
    }
    p as Term
}

// ---------------------------------------------------------------------------
// put

/// Words a HAMT insert may allocate (excluding the map header).
fn hamt_put_need(r: Term, h: u64) -> usize {
    let mut n = r;
    let mut lvl = 0;
    let mut need = 0;
    loop {
        need += nwords(n) + 2;
        let bit = 1u32 << nib(h, lvl);
        let bm = bitmap(n);
        if bm & bit == 0 {
            return need + 3;
        }
        let c = child(n, (bm & (bit - 1)).count_ones() as usize);
        if is_hamt(c) {
            n = c;
            lvl += 1;
        } else {
            // replace/extend the leaf, or split it into a chain of nodes
            return need + nwords(c) + 3 + 3 + 4 * (LEVELS - lvl) as usize;
        }
    }
}

/// Inserts into the HAMT subtree `n` at `lvl`; returns the new subtree and
/// whether a key was added, or None when the value is unchanged.
fn hamt_ins(ctx: &mut Ctx, n: Term, lvl: u32, h: u64, k: Term, v: Term) -> Option<(Term, bool)> {
    let bit = 1u32 << nib(h, lvl);
    let bm = bitmap(n);
    let pos = (bm & (bit - 1)).count_ones() as usize;
    if bm & bit == 0 {
        let l = leaf(ctx, k, v);
        return Some((hamt_insert(ctx, n, bit, pos, l), true));
    }
    let c = child(n, pos);
    let (nc, added) = if is_hamt(c) {
        hamt_ins(ctx, c, lvl + 1, h, k, v)?
    } else {
        match flat_search(c, k) {
            Ok(i) => {
                if fval(c, i) == v {
                    return None;
                }
                (flat_set(ctx, c, i, v), false)
            }
            Err(i) => {
                let hc = internal_hash(fkey(c, 0));
                if hc == h {
                    (flat_insert(ctx, c, i, k, v), true)
                } else {
                    let l = leaf(ctx, k, v);
                    (split(ctx, c, hc, l, h, lvl + 1), true)
                }
            }
        }
    };
    Some((hamt_set(ctx, n, pos, nc), added))
}

/// Insert or replace. All arguments may be unrooted; this function roots them.
pub fn put(ctx: &mut Ctx, m: Term, k: Term, v: Term) -> Term {
    let r = root(m);
    if r == 0 {
        let mut rs = [k, v];
        ctx.reserve_with(3 + 3, &mut rs);
        let l = leaf(ctx, rs[0], rs[1]);
        return new_map(ctx, 1, l);
    }
    if !is_hamt(r) {
        let found = flat_search(r, k);
        if let Ok(i) = found {
            if fval(r, i) == v {
                return m;
            }
        }
        let len = flat_len(r);
        if found.is_err() && len == SMALL_MAP_LIMIT {
            // Grows into a HAMT.
            let base = ctx.roots.len();
            for i in 0..len {
                ctx.roots.push(fkey(r, i));
                ctx.roots.push(fval(r, i));
            }
            ctx.roots.push(k);
            ctx.roots.push(v);
            let res = from_root_pairs(ctx, base, len + 1);
            ctx.roots.truncate(base);
            return res;
        }
        let mut rs = [m, k, v];
        ctx.reserve_with(3 + 1 + 2 * (len + 1), &mut rs);
        let [m, k, v] = rs;
        let r = root(m);
        return match found {
            Ok(i) => {
                let nr = flat_set(ctx, r, i, v);
                new_map(ctx, len, nr)
            }
            Err(i) => {
                let nr = flat_insert(ctx, r, i, k, v);
                new_map(ctx, len + 1, nr)
            }
        };
    }
    let h = internal_hash(k);
    let need = 3 + hamt_put_need(r, h);
    let mut rs = [m, k, v];
    ctx.reserve_with(need, &mut rs);
    let [m, k, v] = rs;
    match hamt_ins(ctx, root(m), 0, h, k, v) {
        None => m,
        Some((nr, added)) => new_map(ctx, size(m) + added as usize, nr),
    }
}

// ---------------------------------------------------------------------------
// remove

enum Del {
    NotFound,
    /// The subtree is now this node (or 0 when empty).
    New(Term),
}

fn hamt_remove_need(r: Term, h: u64) -> usize {
    let mut n = r;
    let mut lvl = 0;
    let mut need = 0;
    loop {
        need += nwords(n) + 1;
        let bit = 1u32 << nib(h, lvl);
        let bm = bitmap(n);
        if bm & bit == 0 {
            return need;
        }
        let c = child(n, (bm & (bit - 1)).count_ones() as usize);
        if is_hamt(c) {
            n = c;
            lvl += 1;
        } else {
            return need + nwords(c) + 1;
        }
    }
}

fn hamt_del(ctx: &mut Ctx, n: Term, lvl: u32, h: u64, k: Term) -> Del {
    let bit = 1u32 << nib(h, lvl);
    let bm = bitmap(n);
    if bm & bit == 0 {
        return Del::NotFound;
    }
    let pos = (bm & (bit - 1)).count_ones() as usize;
    let c = child(n, pos);
    let nc = if is_hamt(c) {
        match hamt_del(ctx, c, lvl + 1, h, k) {
            Del::NotFound => return Del::NotFound,
            Del::New(x) => x,
        }
    } else {
        match flat_search(c, k) {
            Err(_) => return Del::NotFound,
            Ok(_) if flat_len(c) == 1 => 0,
            Ok(i) => flat_delete(ctx, c, i),
        }
    };
    let cnt = nwords(n) - 1;
    if nc == 0 {
        if cnt == 1 {
            return Del::New(0);
        }
        if cnt == 2 && lvl > 0 {
            // A subnode left with a single leaf collapses into it.
            let other = child(n, 1 - pos);
            if !is_hamt(other) {
                return Del::New(other);
            }
        }
        return Del::New(hamt_remove(ctx, n, bit, pos));
    }
    if cnt == 1 && lvl > 0 && !is_hamt(nc) {
        return Del::New(nc);
    }
    Del::New(hamt_set(ctx, n, pos, nc))
}

pub fn remove(ctx: &mut Ctx, m: Term, k: Term) -> Term {
    let r = root(m);
    if r == 0 {
        return m;
    }
    if !is_hamt(r) {
        let Ok(i) = flat_search(r, k) else { return m };
        let len = flat_len(r);
        if len == 1 {
            return empty();
        }
        let mut rs = [m];
        ctx.reserve_with(3 + 1 + 2 * len, &mut rs);
        let nr = flat_delete(ctx, root(rs[0]), i);
        return new_map(ctx, len - 1, nr);
    }
    if find(m, k).is_none() {
        return m;
    }
    let n = size(m);
    if n - 1 <= SMALL_MAP_LIMIT {
        // Shrinks back to a flat map.
        let base = ctx.roots.len();
        for (kk, vv) in entries(m) {
            if map_key_cmp(kk, k) != Equal {
                ctx.roots.push(kk);
                ctx.roots.push(vv);
            }
        }
        let res = from_root_pairs(ctx, base, n - 1);
        ctx.roots.truncate(base);
        return res;
    }
    let h = internal_hash(k);
    let need = 3 + hamt_remove_need(r, h);
    let mut rs = [m, k];
    ctx.reserve_with(need, &mut rs);
    let [m, k] = rs;
    match hamt_del(ctx, root(m), 0, h, k) {
        Del::NotFound => m,
        Del::New(nr) => new_map(ctx, n - 1, nr),
    }
}

// ---------------------------------------------------------------------------
// Bulk construction

/// HAMT nibble order: nibbles taken least-significant first.
#[inline]
fn order_key(h: u64) -> u64 {
    crate::beamhash::hamt_order_key(h)
}

fn is_leaf_group(es: &[(u64, u64, usize)], lvl: u32) -> bool {
    es.len() == 1 || lvl == LEVELS || es.iter().all(|e| e.1 == es[0].1)
}

/// Splits `es` (sorted by order key) into runs sharing the nibble at `lvl`.
fn groups(es: &[(u64, u64, usize)], lvl: u32) -> Vec<(u32, usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < es.len() {
        let nb = nib(es[i].1, lvl);
        let mut j = i + 1;
        while j < es.len() && nib(es[j].1, lvl) == nb {
            j += 1;
        }
        out.push((nb, i, j));
        i = j;
    }
    out
}

/// Words needed for the subtree over `es` at `lvl`.
fn build_need(es: &[(u64, u64, usize)], lvl: u32) -> usize {
    if is_leaf_group(es, lvl) {
        return 1 + 2 * es.len();
    }
    let mut need = 2;
    for (_, i, j) in groups(es, lvl) {
        need += 1 + build_need(&es[i..j], lvl + 1);
    }
    need
}

fn build(ctx: &mut Ctx, base: usize, es: &[(u64, u64, usize)], lvl: u32) -> Term {
    if is_leaf_group(es, lvl) {
        // A leaf (several pairs only for a full hash collision).
        let mut idx: Vec<usize> = es.iter().map(|e| e.2).collect();
        {
            let roots = &ctx.roots;
            idx.sort_by(|&a, &b| map_key_cmp(roots[base + 2 * a], roots[base + 2 * b]));
        }
        let p = alloc(ctx, T_MAPNODE, 2 * idx.len());
        for (o, &i) in idx.iter().enumerate() {
            unsafe {
                *p.add(1 + 2 * o) = ctx.roots[base + 2 * i];
                *p.add(2 + 2 * o) = ctx.roots[base + 2 * i + 1];
            }
        }
        return p as Term;
    }
    let gs = groups(es, lvl);
    let mut kids = Vec::with_capacity(gs.len());
    let mut bm = 0u32;
    for &(nb, i, j) in &gs {
        bm |= 1 << nb;
        kids.push(build(ctx, base, &es[i..j], lvl + 1));
    }
    let p = alloc(ctx, T_HAMT, 1 + kids.len());
    unsafe {
        *p.add(1) = small(bm as i64);
        for (o, &c) in kids.iter().enumerate() {
            *p.add(2 + o) = c;
        }
    }
    p as Term
}

/// Build a map from key/value pairs stored in `ctx.roots[base .. base + 2n]`
/// (k0, v0, k1, v1, ...). Later keys win.
pub fn from_root_pairs(ctx: &mut Ctx, base: usize, n: usize) -> Term {
    if n == 0 {
        return empty();
    }
    let mut idx: Vec<usize> = (0..n).collect();
    {
        let roots = &ctx.roots;
        idx.sort_by(|&a, &b| map_key_cmp(roots[base + 2 * a], roots[base + 2 * b]));
    }
    // Dedupe: keep the last occurrence among equal keys (the sort is stable).
    let mut uniq: Vec<usize> = Vec::with_capacity(n);
    for &i in &idx {
        if let Some(&last) = uniq.last() {
            if map_key_cmp(ctx.roots[base + 2 * last], ctx.roots[base + 2 * i]) == Equal {
                *uniq.last_mut().unwrap() = i;
                continue;
            }
        }
        uniq.push(i);
    }
    let cnt = uniq.len();
    if cnt <= SMALL_MAP_LIMIT {
        ctx.reserve(3 + 1 + 2 * cnt);
        let p = alloc(ctx, T_MAPNODE, 2 * cnt);
        for (o, &i) in uniq.iter().enumerate() {
            unsafe {
                *p.add(1 + 2 * o) = ctx.roots[base + 2 * i];
                *p.add(2 + 2 * o) = ctx.roots[base + 2 * i + 1];
            }
        }
        return new_map(ctx, cnt, p as Term);
    }
    let mut es: Vec<(u64, u64, usize)> = uniq
        .iter()
        .map(|&i| {
            let h = internal_hash(ctx.roots[base + 2 * i]);
            (order_key(h), h, i)
        })
        .collect();
    es.sort_by(|a, b| a.0.cmp(&b.0));
    ctx.reserve(3 + build_need(&es, 0));
    let r = build(ctx, base, &es, 0);
    new_map(ctx, cnt, r)
}

// ---------------------------------------------------------------------------
// Traversal

fn walk(n: Term, f: &mut impl FnMut(Term, Term)) {
    if is_hamt(n) {
        // BEAM iteration order: the HAMT walked from the highest slot down.
        for i in (0..nwords(n) - 1).rev() {
            walk(child(n, i), f);
        }
    } else {
        for i in 0..flat_len(n) {
            f(fkey(n, i), fval(n, i));
        }
    }
}

/// Visits the entries in BEAM iteration order.
pub fn for_each(m: Term, mut f: impl FnMut(Term, Term)) {
    let r = root(m);
    if r != 0 {
        walk(r, &mut f);
    }
}

/// Entries in BEAM iteration order (valid until the next GC).
pub fn entries(m: Term) -> Vec<(Term, Term)> {
    let mut v = Vec::with_capacity(size(m));
    for_each(m, |k, val| v.push((k, val)));
    v
}

/// Entries in BEAM iteration order (what `:maps.to_list/1` returns).
pub fn entries_beam(m: Term) -> Vec<(Term, Term)> {
    entries(m)
}

/// `:maps.to_list/1` — list of {k, v} in BEAM iteration order.
pub fn to_list(ctx: &mut Ctx, m: Term) -> Term {
    let n = size(m);
    let mut r = [m];
    ctx.reserve_with(6 * n, &mut r);
    let es = entries(r[0]);
    let mut acc = NIL_LIST;
    for &(k, v) in es.iter().rev() {
        let t = ctx.tuple_uninit_nogc(2);
        unsafe {
            *t.add(1) = k;
            *t.add(2) = v;
        }
        acc = ctx.cons_nogc(t as Term, acc);
    }
    acc
}

pub fn keys(ctx: &mut Ctx, m: Term) -> Term {
    let n = size(m);
    let mut r = [m];
    ctx.reserve_with(3 * n, &mut r);
    let es = entries(r[0]);
    let mut acc = NIL_LIST;
    for &(k, _) in es.iter().rev() {
        acc = ctx.cons_nogc(k, acc);
    }
    acc
}

pub fn values(ctx: &mut Ctx, m: Term) -> Term {
    let n = size(m);
    let mut r = [m];
    ctx.reserve_with(3 * n, &mut r);
    let es = entries(r[0]);
    let mut acc = NIL_LIST;
    for &(_, v) in es.iter().rev() {
        acc = ctx.cons_nogc(v, acc);
    }
    acc
}

pub fn merge(ctx: &mut Ctx, a: Term, b: Term) -> Term {
    if size(b) == 0 {
        return a;
    }
    if size(a) == 0 {
        return b;
    }
    if size(b) <= 4 || size(b) * 8 < size(a) {
        let base = ctx.roots.len();
        ctx.roots.push(a);
        ctx.roots.push(b);
        for (k, v) in entries(b) {
            ctx.roots.push(k);
            ctx.roots.push(v);
        }
        let cnt = (ctx.roots.len() - base - 2) / 2;
        for i in 0..cnt {
            let cur = ctx.roots[base];
            let k = ctx.roots[base + 2 + 2 * i];
            let v = ctx.roots[base + 3 + 2 * i];
            let nm = put(ctx, cur, k, v);
            ctx.roots[base] = nm;
        }
        let r = ctx.roots[base];
        ctx.roots.truncate(base);
        return r;
    }
    let base = ctx.roots.len();
    for (k, v) in entries(a) {
        ctx.roots.push(k);
        ctx.roots.push(v);
    }
    for (k, v) in entries(b) {
        ctx.roots.push(k);
        ctx.roots.push(v);
    }
    let n = (ctx.roots.len() - base) / 2;
    let r = from_root_pairs(ctx, base, n);
    ctx.roots.truncate(base);
    r
}

pub fn map_compare(a: Term, b: Term, exact: bool) -> std::cmp::Ordering {
    let (sa, sb) = (size(a), size(b));
    if sa != sb {
        return sa.cmp(&sb);
    }
    // Term order compares keys (then values) in sorted term order.
    let mut ea = entries(a);
    let mut eb = entries(b);
    ea.sort_by(|x, y| crate::cmp::compare_exact(x.0, y.0));
    eb.sort_by(|x, y| crate::cmp::compare_exact(x.0, y.0));
    for i in 0..ea.len() {
        let o = crate::cmp::compare_exact(ea[i].0, eb[i].0);
        if o != Equal {
            return o;
        }
    }
    for i in 0..ea.len() {
        let o = if exact {
            crate::cmp::compare_exact(ea[i].1, eb[i].1)
        } else {
            crate::cmp::compare(ea[i].1, eb[i].1)
        };
        if o != Equal {
            return o;
        }
    }
    Equal
}
