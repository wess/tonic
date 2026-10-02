//! ETS: shared term storage. Objects are copied into owned fragments on
//! insert and back into the caller's heap on lookup, like the BEAM.

use crate::atoms;
use crate::bif_core::cx;
use crate::cmp;
use crate::heap::{Ctx, Fragment};
use crate::term::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Set,
    OrderedSet,
    Bag,
    DuplicateBag,
}

struct Table {
    id: u64,
    /// Reference id used in the table's `#Reference<...>` identifier.
    ref_id: u64,
    name: u64,
    named: bool,
    kind: Kind,
    keypos: usize,
    owner: u64,
    protection: u64,
    /// Set/bag: buckets keyed by the exact hash of the key; ordered_set: a
    /// single bucket (0) kept sorted.
    buckets: HashMap<u64, Vec<Fragment>>,
    sorted: Vec<Fragment>,
    /// Insertion order of bucket hashes, for stable iteration.
    order: Vec<u64>,
    size: usize,
}

struct Registry {
    tables: HashMap<u64, Arc<Mutex<Table>>>,
    names: HashMap<u64, u64>,
    refs: HashMap<u64, u64>,
    next: u64,
}

fn reg() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| {
        Mutex::new(Registry { tables: HashMap::new(), names: HashMap::new(), refs: HashMap::new(), next: 1 })
    })
}

const NO_TABLE: &str = "the table identifier does not refer to an existing ETS table";

fn find(tab: Term) -> Option<Arc<Mutex<Table>>> {
    let r = reg().lock();
    let id = if is_atom(tab) {
        *r.names.get(&atom_idx(tab))?
    } else if is_ref(tab) {
        *r.refs.get(&word(tab, 1))?
    } else {
        return None;
    };
    r.tables.get(&id).cloned()
}

fn key_of(t: &Table, obj: Term) -> Term {
    tuple_get(obj, t.keypos - 1)
}

fn frag_key(t: &Table, f: &Fragment) -> Term {
    key_of(t, f.root)
}

fn ordered_pos(t: &Table, key: Term) -> Result<usize, usize> {
    t.sorted.binary_search_by(|f| cmp::compare(frag_key(t, f), key))
}

fn tid_term(ctx: &mut Ctx, t: &Table) -> Term {
    if t.named {
        atom(t.name)
    } else {
        let p = ctx.alloc(2);
        unsafe {
            *p = header(T_REF, 1);
            *p.add(1) = t.ref_id;
        }
        p as Term
    }
}

fn a(name: &str) -> Term {
    atom(atoms::intern(name))
}

#[no_mangle]
pub extern "C" fn tn_ets_new(c: *mut Ctx, name: Term, opts: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(name) {
        return ctx.badarg_args(&[(1, "not an atom")]);
    }
    let mut kind = Kind::Set;
    let mut named = false;
    let mut keypos = 1usize;
    let mut protection = a("protected");
    let mut l = opts;
    while is_cons(l) {
        let o = head(l);
        if is_atom(o) {
            match atoms::name(atom_idx(o)) {
                "set" => kind = Kind::Set,
                "ordered_set" => kind = Kind::OrderedSet,
                "bag" => kind = Kind::Bag,
                "duplicate_bag" => kind = Kind::DuplicateBag,
                "named_table" => named = true,
                "public" | "protected" | "private" => protection = o,
                "compressed" => {}
                _ => return ctx.badarg_args(&[(2, "invalid options")]),
            }
        } else if is_tuple(o) && tuple_size(o) == 2 && is_atom(tuple_get(o, 0)) {
            match atoms::name(atom_idx(tuple_get(o, 0))) {
                "keypos" if is_small(tuple_get(o, 1)) && small_val(tuple_get(o, 1)) >= 1 => {
                    keypos = small_val(tuple_get(o, 1)) as usize
                }
                "read_concurrency" | "write_concurrency" | "decentralized_counters" | "heir" => {}
                _ => return ctx.badarg_args(&[(2, "invalid options")]),
            }
        } else if is_tuple(o) && tuple_size(o) == 3 && tuple_get(o, 0) == a("heir") {
        } else {
            return ctx.badarg_args(&[(2, "invalid options")]);
        }
        l = tail(l);
    }
    if l != NIL_LIST {
        return ctx.badarg_args(&[(2, "not a list")]);
    }
    let owner = ctx.pid();
    let mut r = reg().lock();
    if named && r.names.contains_key(&atom_idx(name)) {
        drop(r);
        return ctx.badarg_args(&[(1, "table name already exists")]);
    }
    let id = r.next;
    r.next += 1;
    let ref_id = crate::sched::next_ref_id();
    let t = Table {
        id,
        ref_id,
        name: atom_idx(name),
        named,
        kind,
        keypos,
        owner,
        protection,
        buckets: HashMap::new(),
        sorted: Vec::new(),
        order: Vec::new(),
        size: 0,
    };
    if named {
        r.names.insert(atom_idx(name), id);
    }
    r.refs.insert(ref_id, id);
    let arc = Arc::new(Mutex::new(t));
    r.tables.insert(id, arc.clone());
    drop(r);
    let t = arc.lock();
    tid_term(ctx, &t)
}

/// Remove all tables owned by a process that exited.
pub fn owner_exited(pid: u64) {
    let mut r = reg().lock();
    let dead: Vec<u64> = r
        .tables
        .iter()
        .filter(|(_, t)| t.lock().owner == pid)
        .map(|(id, _)| *id)
        .collect();
    for id in dead {
        if let Some(t) = r.tables.remove(&id) {
            let t = t.lock();
            if t.named {
                r.names.remove(&t.name);
            }
            r.refs.remove(&t.ref_id);
        }
    }
}

fn insert_one(ctx: &Ctx, t: &mut Table, obj: Term, only_new: bool) -> bool {
    let key = key_of(t, obj);
    match t.kind {
        Kind::OrderedSet => match ordered_pos(t, key) {
            Ok(i) => {
                if only_new {
                    return false;
                }
                t.sorted[i] = ctx.to_fragment(obj);
                true
            }
            Err(i) => {
                t.sorted.insert(i, ctx.to_fragment(obj));
                t.size += 1;
                true
            }
        },
        _ => {
            let h = cmp::hash(key);
            let kind = t.kind;
            let keypos = t.keypos;
            if !t.buckets.contains_key(&h) {
                t.order.push(h);
            }
            let bucket = t.buckets.entry(h).or_default();
            let same_key: Vec<usize> = bucket
                .iter()
                .enumerate()
                .filter(|(_, f)| cmp::eq_exact(tuple_get(f.root, keypos - 1), key))
                .map(|(i, _)| i)
                .collect();
            if only_new && !same_key.is_empty() {
                return false;
            }
            match kind {
                Kind::Set => {
                    if let Some(&i) = same_key.first() {
                        bucket[i] = ctx.to_fragment(obj);
                    } else {
                        bucket.push(ctx.to_fragment(obj));
                        t.size += 1;
                    }
                }
                Kind::Bag => {
                    if !same_key.iter().any(|&i| cmp::eq_exact(bucket[i].root, obj)) {
                        bucket.push(ctx.to_fragment(obj));
                        t.size += 1;
                    }
                }
                _ => {
                    bucket.push(ctx.to_fragment(obj));
                    t.size += 1;
                }
            }
            true
        }
    }
}

fn check_obj(t: &Table, obj: Term) -> bool {
    is_tuple(obj) && tuple_size(obj) >= t.keypos
}

fn do_insert(c: *mut Ctx, tab: Term, objs: Term, only_new: bool) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    let list: Vec<Term> = if is_tuple(objs) {
        vec![objs]
    } else {
        let mut v = Vec::new();
        let mut l = objs;
        while is_cons(l) {
            v.push(head(l));
            l = tail(l);
        }
        if l != NIL_LIST {
            return ctx.badarg_args(&[(2, "not a tuple or a list of tuples")]);
        }
        v
    };
    if list.iter().any(|&o| !check_obj(&t, o)) {
        return ctx.badarg_args(&[(2, "not a tuple or a list of tuples")]);
    }
    if only_new {
        // All or nothing.
        for &o in &list {
            if lookup_count(&t, key_of(&t, o)) > 0 {
                return FALSE;
            }
        }
    }
    for &o in &list {
        insert_one(ctx, &mut t, o, false);
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_ets_insert(c: *mut Ctx, tab: Term, objs: Term) -> Term {
    do_insert(c, tab, objs, false)
}

#[no_mangle]
pub extern "C" fn tn_ets_insert_new(c: *mut Ctx, tab: Term, objs: Term) -> Term {
    do_insert(c, tab, objs, true)
}

fn matching<'a>(t: &'a Table, key: Term) -> Vec<&'a Fragment> {
    match t.kind {
        Kind::OrderedSet => match ordered_pos(t, key) {
            Ok(i) => vec![&t.sorted[i]],
            Err(_) => vec![],
        },
        _ => match t.buckets.get(&cmp::hash(key)) {
            Some(b) => b.iter().filter(|f| cmp::eq_exact(frag_key(t, f), key)).collect(),
            None => vec![],
        },
    }
}

fn lookup_count(t: &Table, key: Term) -> usize {
    matching(t, key).len()
}

fn frags_to_list(ctx: &mut Ctx, frags: &[&Fragment]) -> Term {
    let base = ctx.roots.len();
    for f in frags {
        let x = ctx.from_fragment(f);
        ctx.roots.push(x);
    }
    let l = ctx.list_from_roots(base, frags.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

#[no_mangle]
pub extern "C" fn tn_ets_lookup(c: *mut Ctx, tab: Term, key: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    // Copy the key out of the caller's heap first? Not needed: no GC before matching.
    let m = matching(&t, key);
    frags_to_list(ctx, &m)
}

#[no_mangle]
pub extern "C" fn tn_ets_member(c: *mut Ctx, tab: Term, key: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    boolean(lookup_count(&t, key) > 0)
}

/// lookup_element(tab, key, pos, default) — `default` is NONE-like 0 when absent.
#[no_mangle]
pub extern "C" fn tn_ets_lookup_element(c: *mut Ctx, tab: Term, key: Term, pos: Term, default: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    if !is_small(pos) || small_val(pos) < 1 {
        return ctx.badarg_args(&[(3, "not a valid position")]);
    }
    let p = small_val(pos) as usize;
    let m = matching(&t, key);
    if m.is_empty() {
        if is_tuple(default) && tuple_size(default) == 1 {
            return tuple_get(default, 0);
        }
        return ctx.badarg_args(&[(2, "not a key that exists in the table")]);
    }
    if m.iter().any(|f| tuple_size(f.root) < p) {
        return ctx.badarg_args(&[(3, "out of range")]);
    }
    if t.kind == Kind::Bag || t.kind == Kind::DuplicateBag {
        let base = ctx.roots.len();
        for f in &m {
            let el = Fragment { buf: Vec::new(), root: 0 };
            let _ = el;
            let x = ctx.from_fragment(f);
            let e = tuple_get(x, p - 1);
            ctx.roots.push(e);
        }
        let l = ctx.list_from_roots(base, m.len(), NIL_LIST);
        ctx.roots.truncate(base);
        return l;
    }
    let x = ctx.from_fragment(m[0]);
    tuple_get(x, p - 1)
}

#[no_mangle]
pub extern "C" fn tn_ets_delete_table(c: *mut Ctx, tab: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    let mut r = reg().lock();
    r.tables.remove(&t.id);
    if t.named {
        r.names.remove(&t.name);
    }
    r.refs.remove(&t.ref_id);
    TRUE
}

fn remove_where(t: &mut Table, key: Term, pred: &dyn Fn(Term) -> bool) -> Vec<Fragment> {
    let mut removed = Vec::new();
    match t.kind {
        Kind::OrderedSet => {
            if let Ok(i) = ordered_pos(t, key) {
                if pred(t.sorted[i].root) {
                    removed.push(t.sorted.remove(i));
                }
            }
        }
        _ => {
            let h = cmp::hash(key);
            let keypos = t.keypos;
            if let Some(b) = t.buckets.get_mut(&h) {
                let mut i = 0;
                while i < b.len() {
                    if cmp::eq_exact(tuple_get(b[i].root, keypos - 1), key) && pred(b[i].root) {
                        removed.push(b.remove(i));
                    } else {
                        i += 1;
                    }
                }
                if b.is_empty() {
                    t.buckets.remove(&h);
                    t.order.retain(|x| *x != h);
                }
            }
        }
    }
    t.size -= removed.len();
    removed
}

#[no_mangle]
pub extern "C" fn tn_ets_delete(c: *mut Ctx, tab: Term, key: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    remove_where(&mut t, key, &|_| true);
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_ets_delete_object(c: *mut Ctx, tab: Term, obj: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    if !check_obj(&t, obj) {
        return ctx.badarg_args(&[(2, "not a tuple")]);
    }
    let key = key_of(&t, obj);
    remove_where(&mut t, key, &|o| cmp::eq_exact(o, obj));
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_ets_take(c: *mut Ctx, tab: Term, key: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    let removed = remove_where(&mut t, key, &|_| true);
    let refs: Vec<&Fragment> = removed.iter().collect();
    frags_to_list(ctx, &refs)
}

#[no_mangle]
pub extern "C" fn tn_ets_delete_all_objects(c: *mut Ctx, tab: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    t.buckets.clear();
    t.sorted.clear();
    t.order.clear();
    t.size = 0;
    TRUE
}

fn all_frags(t: &Table) -> Vec<&Fragment> {
    match t.kind {
        Kind::OrderedSet => t.sorted.iter().collect(),
        _ => t.order.iter().flat_map(|h| t.buckets[h].iter()).collect(),
    }
}

#[no_mangle]
pub extern "C" fn tn_ets_tab2list(c: *mut Ctx, tab: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    let all = all_frags(&t);
    frags_to_list(ctx, &all)
}

/// Keys in iteration order (for first/next/last/prev, implemented in Elixir).
#[no_mangle]
pub extern "C" fn tn_ets_keys(c: *mut Ctx, tab: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    let mut keys: Vec<Fragment> = Vec::new();
    let mut seen: Vec<Term> = Vec::new();
    for f in all_frags(&t) {
        let k = frag_key(&t, f);
        if !seen.iter().any(|&s| cmp::eq_exact(s, k)) {
            seen.push(k);
            keys.push(Fragment { buf: Vec::new(), root: 0 });
            let n = keys.len() - 1;
            keys[n] = copy_out(k);
        }
    }
    let refs: Vec<&Fragment> = keys.iter().collect();
    frags_to_list(ctx, &refs)
}

/// Copy a term that lives in a fragment into a new standalone fragment.
fn copy_out(t: Term) -> Fragment {
    let words = crate::heap::copy_size(t, 0, usize::MAX);
    let mut buf: Vec<u64> = Vec::with_capacity(words.max(1));
    unsafe { buf.set_len(words.max(1)) };
    let mut free = buf.as_mut_ptr();
    let root = unsafe { crate::heap::copy_term(t, 0, usize::MAX, &mut free) };
    Fragment { buf, root }
}

/// The next key after `key` in iteration order, or `$end_of_table`.
#[no_mangle]
pub extern "C" fn tn_ets_next(c: *mut Ctx, tab: Term, key: Term, dir: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    let end = a("$end_of_table");
    let forward = dir == TRUE;
    let found: Option<Fragment> = if t.kind == Kind::OrderedSet {
        if forward {
            let i = match ordered_pos(&t, key) {
                Ok(i) => i + 1,
                Err(i) => i,
            };
            t.sorted.get(i).map(|f| copy_out(frag_key(&t, f)))
        } else {
            let i = match ordered_pos(&t, key) {
                Ok(i) | Err(i) => i,
            };
            if i == 0 {
                None
            } else {
                t.sorted.get(i - 1).map(|f| copy_out(frag_key(&t, f)))
            }
        }
    } else {
        let all = all_frags(&t);
        let pos = all.iter().position(|f| cmp::eq_exact(frag_key(&t, f), key));
        match pos {
            None => return ctx.badarg_args(&[(2, "not a key that exists in the table")]),
            Some(i) => {
                let k = frag_key(&t, all[i]);
                let mut j = i + 1;
                while j < all.len() && cmp::eq_exact(frag_key(&t, all[j]), k) {
                    j += 1;
                }
                all.get(j).map(|f| copy_out(frag_key(&t, f)))
            }
        }
    };
    match found {
        Some(f) => ctx.from_fragment(&f),
        None => end,
    }
}

#[no_mangle]
pub extern "C" fn tn_ets_first(c: *mut Ctx, tab: Term, dir: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let t = t.lock();
    let all = all_frags(&t);
    let f = if dir == TRUE { all.first() } else { all.last() };
    match f {
        Some(f) => {
            let k = copy_out(frag_key(&t, f));
            ctx.from_fragment(&k)
        }
        None => a("$end_of_table"),
    }
}

#[no_mangle]
pub extern "C" fn tn_ets_info(c: *mut Ctx, tab: Term, item: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return a("undefined") };
    let t = t.lock();
    match atoms::name(atom_idx(item)) {
        "size" => small(t.size as i64),
        "name" => atom(t.name),
        "named_table" => boolean(t.named),
        "keypos" => small(t.keypos as i64),
        "owner" => pid(t.owner),
        "protection" => t.protection,
        "type" => a(match t.kind {
            Kind::Set => "set",
            Kind::OrderedSet => "ordered_set",
            Kind::Bag => "bag",
            Kind::DuplicateBag => "duplicate_bag",
        }),
        "id" => tid_term(ctx, &t),
        "memory" => small(t.size as i64 * 8),
        "heir" => a("none"),
        "compressed" | "read_concurrency" | "write_concurrency" | "decentralized_counters" => FALSE,
        _ => ctx.badarg_args(&[(2, "not a valid info item")]),
    }
}

#[no_mangle]
pub extern "C" fn tn_ets_whereis(c: *mut Ctx, name: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(name) {
        return ctx.badarg_args(&[(1, "not an atom")]);
    }
    match find(name) {
        Some(t) => {
            let t = t.lock();
            let p = ctx.alloc(2);
            unsafe {
                *p = header(T_REF, 1);
                *p.add(1) = t.ref_id;
            }
            p as Term
        }
        None => a("undefined"),
    }
}

#[no_mangle]
pub extern "C" fn tn_ets_all(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let tabs: Vec<Arc<Mutex<Table>>> = reg().lock().tables.values().cloned().collect();
    let base = ctx.roots.len();
    for t in &tabs {
        let t = t.lock();
        let x = tid_term(ctx, &t);
        ctx.roots.push(x);
    }
    let l = ctx.list_from_roots(base, tabs.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

#[no_mangle]
pub extern "C" fn tn_ets_rename(c: *mut Ctx, tab: Term, name: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    let mut r = reg().lock();
    if t.named {
        if r.names.contains_key(&atom_idx(name)) {
            return ctx.badarg_args(&[(2, "table name already exists")]);
        }
        r.names.remove(&t.name);
        r.names.insert(atom_idx(name), t.id);
    }
    t.name = atom_idx(name);
    name
}

/// Replace the object(s) for `key` with `obj` (used by update_counter /
/// update_element implemented in Elixir). Returns true.
#[no_mangle]
pub extern "C" fn tn_ets_replace(c: *mut Ctx, tab: Term, obj: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    let key = key_of(&t, obj);
    remove_where(&mut t, key, &|_| true);
    insert_one(ctx, &mut t, obj, false);
    TRUE
}

/// Atomic update_counter for set/ordered_set: `ops` is a list of
/// {pos, incr} or {pos, incr, threshold, setvalue}; `default` is an object
/// tuple to insert when the key is missing, or [] for none. Returns the list
/// of new values, or the atom `missing` / `badop`.
#[no_mangle]
pub extern "C" fn tn_ets_update_counter(c: *mut Ctx, tab: Term, key: Term, ops: Term, default: Term) -> Term {
    let ctx = cx(c);
    let Some(t) = find(tab) else { return ctx.badarg_args(&[(1, NO_TABLE)]) };
    let mut t = t.lock();
    if t.kind == Kind::Bag || t.kind == Kind::DuplicateBag {
        return ctx.badarg_args(&[(1, "the table type is not supported by this operation")]);
    }
    let base = ctx.roots.len();
    let current = {
        let m = matching(&t, key);
        match m.first() {
            Some(f) => ctx.from_fragment(f),
            None => {
                if is_tuple(default) {
                    default
                } else {
                    return ctx.badarg_args(&[(2, "not a key that exists in the table")]);
                }
            }
        }
    };
    ctx.roots.push(current);
    ctx.roots.push(ops);
    // Work on a Vec copy of the tuple elements.
    let n = tuple_size(ctx.roots[base]);
    let mut elems: Vec<Term> = (0..n).map(|i| tuple_get(ctx.roots[base], i)).collect();
    let mut results: Vec<i64> = Vec::new();
    let mut l = ctx.roots[base + 1];
    while is_cons(l) {
        let op = head(l);
        if !is_tuple(op) || !(tuple_size(op) == 2 || tuple_size(op) == 4) {
            ctx.roots.truncate(base);
            return ctx.badarg_args(&[(3, "not a valid update operation")]);
        }
        let pos = tuple_get(op, 0);
        let incr = tuple_get(op, 1);
        if !is_small(pos) || !is_small(incr) || small_val(pos) < 1 || small_val(pos) as usize > n || small_val(pos) as usize == t.keypos {
            ctx.roots.truncate(base);
            return ctx.badarg_args(&[(3, "the position is out of range or refers to the key")]);
        }
        let p = small_val(pos) as usize - 1;
        let cur = elems[p];
        if !is_small(cur) {
            ctx.roots.truncate(base);
            return ctx.badarg_args(&[(1, "the element to update is not an integer")]);
        }
        let mut v = small_val(cur) + small_val(incr);
        if tuple_size(op) == 4 {
            let th = small_val(tuple_get(op, 2));
            let set = small_val(tuple_get(op, 3));
            if (small_val(incr) >= 0 && v > th) || (small_val(incr) < 0 && v < th) {
                v = set;
            }
        }
        elems[p] = small(v);
        results.push(v);
        l = tail(l);
    }
    let newobj = ctx.tuple(&elems);
    ctx.roots.truncate(base);
    let k = key_of(&t, newobj);
    remove_where(&mut t, k, &|_| true);
    insert_one(ctx, &mut t, newobj, false);
    let mut v: Vec<Term> = results.into_iter().map(small).collect();
    ctx.list_from_vec(&mut v)
}
