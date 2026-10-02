//! Process BIFs: spawn, send, receive, links, monitors, registry, timers,
//! process dictionary.

use crate::atoms;
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::map;
use crate::sched::{self, Entry, Signal, SpawnOpts};
use crate::term::*;

fn resolve_dest(ctx: &mut Ctx, dest: Term) -> Result<Option<u64>, Term> {
    if is_pid(dest) {
        return Ok(Some(pid_id(dest)));
    }
    if is_ref(dest) {
        // a process alias; inactive aliases drop the message
        return Ok(sched::alias_lookup(word(dest, 1)));
    }
    if is_atom(dest) {
        return match sched::whereis(atom_idx(dest)) {
            Some(p) => Ok(Some(p)),
            None => Err(ctx.badarg_args(&[(1, "invalid destination")])),
        };
    }
    if is_tuple(dest) && tuple_size(dest) == 2 && is_atom(tuple_get(dest, 0)) {
        return resolve_dest(ctx, tuple_get(dest, 0));
    }
    Err(ctx.badarg("argument error"))
}

#[no_mangle]
pub extern "C" fn tn_send(c: *mut Ctx, dest: Term, msg: Term) -> Term {
    let ctx = cx(c);
    match resolve_dest(ctx, dest) {
        Ok(Some(p)) => {
            let f = ctx.to_fragment(msg);
            sched::send_fragment(p, f);
            msg
        }
        Ok(None) => msg,
        Err(e) => e,
    }
}

fn do_spawn(ctx: &mut Ctx, f: Term, link: bool, monitor: Option<u64>) -> Result<u64, Term> {
    if !is_closure(f) || closure_arity(f) != 0 {
        return Err(ctx.bad_function(f));
    }
    let frag = ctx.to_fragment(f);
    do_spawn_entry(ctx, Entry::Closure(frag), link, monitor)
}

fn do_spawn_entry(ctx: &mut Ctx, entry: Entry, link: bool, monitor: Option<u64>) -> Result<u64, Term> {
    let me = ctx.pid();
    // (the guard must be dropped before spawning: spawn locks it again)
    let gl = sched::group_leaders().lock().get(&me).copied();
    Ok(sched::spawn(
        entry,
        SpawnOpts {
            link_to: if link { Some(me) } else { None },
            parent: Some(me),
            monitor_by: monitor.map(|r| (r, me)),
            group_leader: gl,
        },
    ))
}

#[no_mangle]
pub extern "C" fn tn_spawn(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    match do_spawn(ctx, f, false, None) {
        Ok(p) => pid(p),
        Err(e) => e,
    }
}

#[no_mangle]
pub extern "C" fn tn_spawn_link(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    match do_spawn(ctx, f, true, None) {
        Ok(p) => pid(p),
        Err(e) => e,
    }
}

#[no_mangle]
pub extern "C" fn tn_spawn_monitor(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    let refid = sched::next_ref_id();
    match do_spawn(ctx, f, false, Some(refid)) {
        Ok(p) => {
            let r = sched::mk_ref_term(ctx, refid);
            ctx.tuple(&[pid(p), r])
        }
        Err(e) => e,
    }
}

#[no_mangle]
pub extern "C" fn tn_link(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(p) {
        return ctx.badarg("argument error");
    }
    let me = ctx.pid();
    if pid_id(p) == me {
        return TRUE;
    }
    if !sched::link(me, pid_id(p)) {
        // Linking to a dead process delivers a noproc exit signal.
        let r = atom(a::NOPROC);
        let f = ctx.to_fragment(r);
        sched::send_signal(
            me,
            Signal::Exit {
                from: pid_id(p),
                reason: f,
                link: true,
            },
        );
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_unlink(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    if is_pid(p) {
        sched::unlink(ctx.pid(), pid_id(p));
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_monitor(c: *mut Ctx, target: Term) -> Term {
    monitor_impl(cx(c), target, None)
}

/// monitor(:process, target, tag: tag): DOWN messages carry `tag` in
/// place of :DOWN.
#[no_mangle]
pub extern "C" fn tn_monitor_tag(c: *mut Ctx, target: Term, tag: Term) -> Term {
    let ctx = cx(c);
    let f = ctx.to_fragment(tag);
    monitor_impl(ctx, target, Some(f))
}

fn monitor_impl(ctx: &mut Ctx, target: Term, tag: Option<crate::heap::Fragment>) -> Term {
    let tpid = if is_pid(target) {
        Some(pid_id(target))
    } else if is_atom(target) {
        sched::whereis(atom_idx(target))
    } else {
        return ctx.badarg("argument error");
    };
    let refid = sched::next_ref_id();
    let has_tag = tag.is_some();
    if let Some(f) = tag {
        sched::monitor_tag_set(refid, f);
    }
    let me = ctx.pid();
    let ok = match tpid {
        Some(p) => sched::monitor(me, p, refid),
        None => false,
    };
    let r = sched::mk_ref_term(ctx, refid);
    if !ok {
        let ri = ctx.push(r);
        let head = if has_tag {
            match sched::monitor_tag_take(refid) {
                Some(f) => ctx.from_fragment(&f),
                None => atom(a::DOWN),
            }
        } else {
            atom(a::DOWN)
        };
        let hi = ctx.push(head);
        let t = ctx.tuple(&[
            ctx.get(hi),
            ctx.get(ri),
            atom(a::PROCESS),
            target,
            atom(a::NOPROC),
        ]);
        let f = ctx.to_fragment(t);
        sched::send_fragment(me, f);
        let r = ctx.get(ri);
        ctx.truncate(ri);
        return r;
    }
    r
}

#[no_mangle]
pub extern "C" fn tn_demonitor(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    if !is_ref(r) {
        return ctx.badarg("argument error");
    }
    sched::demonitor(word(r, 1));
    TRUE
}

/// demonitor returning whether the monitor was active (demonitor(ref, [:info])).
#[no_mangle]
pub extern "C" fn tn_demonitor_info(c: *mut Ctx, r: Term) -> Term {
    if !is_ref(r) {
        return cx(c).badarg_args(&[(1, "not a reference")]);
    }
    boolean(sched::demonitor(word(r, 1)))
}

/// Block until the mailbox has a message, without consuming it (hibernate).
#[no_mangle]
pub extern "C" fn tn_wait_message(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let m = ctx.recv_next(atom(a::INFINITY));
    ctx.recv_started = false;
    ctx.recv_cursor = 0;
    if m == NONE {
        return NONE;
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_priority_of(_c: *mut Ctx, p: Term) -> Term {
    if !is_pid(p) {
        return NIL;
    }
    match sched::priority_of(pid_id(p)) {
        Some(v) => atom(v),
        None => NIL,
    }
}

/// erlang:start_timer(ms, dest, msg): sends {:timeout, tref, msg}.
#[no_mangle]
pub extern "C" fn tn_start_timer(c: *mut Ctx, ms: Term, dest: Term, msg: Term) -> Term {
    let ctx = cx(c);
    if !is_small(ms) || small_val(ms) < 0 {
        return ctx.badarg("argument error");
    }
    let to = match resolve_dest(ctx, dest) {
        Ok(Some(p)) => Some(p),
        Ok(None) => None,
        Err(e) => return e,
    };
    let id = sched::next_ref_id();
    let base = ctx.roots.len();
    ctx.roots.push(msg);
    let r = sched::mk_ref_term(ctx, id);
    ctx.roots.push(r);
    let t = ctx.tuple(&[atom(a::TIMEOUT), ctx.roots[base + 1], ctx.roots[base]]);
    let r = ctx.roots[base + 1];
    ctx.roots.truncate(base);
    if let Some(p) = to {
        let ri = ctx.push(r);
        let f = ctx.to_fragment(t);
        sched::send_after_id(p, f, small_val(ms) as u64, id);
        let r = ctx.get(ri);
        ctx.truncate(ri);
        return r;
    }
    r
}

#[no_mangle]
pub extern "C" fn tn_read_timer(_c: *mut Ctx, r: Term) -> Term {
    if !is_ref(r) {
        return FALSE;
    }
    match sched::read_timer(word(r, 1)) {
        Some(ms) => small(ms as i64),
        None => FALSE,
    }
}

/// Process.exit(pid, reason)
#[no_mangle]
pub extern "C" fn tn_exit2(c: *mut Ctx, p: Term, reason: Term) -> Term {
    let ctx = cx(c);
    let target = match resolve_dest(ctx, p) {
        Ok(Some(t)) => t,
        Ok(None) => return TRUE,
        Err(e) => return e,
    };
    let me = ctx.pid();
    let f = ctx.to_fragment(reason);
    sched::send_signal(
        target,
        Signal::Exit {
            from: me,
            reason: f,
            link: false,
        },
    );
    if target == me {
        // Deliver immediately.
        if !ctx.drain() {
            return NONE;
        }
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_process_flag(c: *mut Ctx, flag: Term, v: Term) -> Term {
    let ctx = cx(c);
    if flag == atom(a::TRAP_EXIT) {
        let old = sched::set_trap_exit(ctx.pid(), v == TRUE);
        return boolean(old);
    }
    if flag == atom(atoms::intern("priority")) && is_atom(v) {
        let old = sched::set_priority(ctx.pid(), atom_idx(v));
        return atom(old);
    }
    ctx.badarg("argument error")
}

#[no_mangle]
pub extern "C" fn tn_is_alive(c: *mut Ctx, p: Term) -> Term {
    if !is_pid(p) {
        return cx(c).badarg("argument error");
    }
    boolean(sched::is_alive(pid_id(p)))
}

#[no_mangle]
pub extern "C" fn tn_register(c: *mut Ctx, p: Term, name: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(p) || !is_atom(name) || name == NIL {
        return ctx.badarg("argument error");
    }
    if sched::register_name(atom_idx(name), pid_id(p)) {
        TRUE
    } else {
        ctx.badarg("could not register process")
    }
}

#[no_mangle]
pub extern "C" fn tn_unregister(c: *mut Ctx, name: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(name) {
        return ctx.badarg("argument error");
    }
    if sched::unregister_name(atom_idx(name)) {
        TRUE
    } else {
        ctx.badarg("argument error")
    }
}

#[no_mangle]
pub extern "C" fn tn_whereis(_c: *mut Ctx, name: Term) -> Term {
    if !is_atom(name) {
        return NIL;
    }
    match sched::whereis(atom_idx(name)) {
        Some(p) => pid(p),
        None => NIL,
    }
}

#[no_mangle]
pub extern "C" fn tn_registered(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let mut v: Vec<Term> = sched::registered().into_iter().map(atom).collect();
    ctx.list_from_vec(&mut v)
}

#[no_mangle]
pub extern "C" fn tn_process_list(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let mut v: Vec<Term> = sched::all_pids().into_iter().map(pid).collect();
    ctx.list_from_vec(&mut v)
}

#[no_mangle]
pub extern "C" fn tn_process_links(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let mut v: Vec<Term> = sched::links_of(pid_id(p)).into_iter().map(pid).collect();
    ctx.list_from_vec(&mut v)
}

#[no_mangle]
pub extern "C" fn tn_message_queue_len(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    if !ctx.drain() {
        return NONE;
    }
    small(ctx.saveq.len() as i64)
}

/// The calling process' mailbox, oldest first.
#[no_mangle]
pub extern "C" fn tn_message_queue(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    if !ctx.drain() {
        return NONE;
    }
    let base = ctx.roots.len();
    let n = ctx.saveq.len();
    for i in 0..n {
        let m = ctx.saveq[i];
        ctx.roots.push(m);
    }
    let l = ctx.list_from_roots(base, n, NIL_LIST);
    ctx.roots.truncate(base);
    l
}

// ---------------- receive ----------------

#[no_mangle]
pub extern "C" fn tn_recv_next(c: *mut Ctx, timeout: Term) -> Term {
    cx(c).recv_next(timeout)
}

#[no_mangle]
pub extern "C" fn tn_recv_accept(c: *mut Ctx) -> Term {
    cx(c).recv_accept();
    TRUE
}

// ---------------- timers ----------------

#[no_mangle]
pub extern "C" fn tn_send_after(c: *mut Ctx, dest: Term, msg: Term, ms: Term) -> Term {
    let ctx = cx(c);
    if !is_small(ms) || small_val(ms) < 0 {
        return ctx.badarg("argument error");
    }
    let to = match resolve_dest(ctx, dest) {
        Ok(Some(p)) => p,
        Ok(None) => return ctx.make_ref(),
        Err(e) => return e,
    };
    let f = ctx.to_fragment(msg);
    let id = sched::send_after(to, f, small_val(ms) as u64);
    sched::mk_ref_term(ctx, id)
}

#[no_mangle]
pub extern "C" fn tn_cancel_timer(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    if !is_ref(r) {
        return ctx.badarg("argument error");
    }
    match sched::cancel_timer(word(r, 1)) {
        Some(ms) => small(ms as i64),
        None => FALSE,
    }
}

// ---------------- process dictionary ----------------

fn dict(ctx: &Ctx) -> Term {
    if ctx.dict == 0 {
        map::empty()
    } else {
        ctx.dict
    }
}

#[no_mangle]
pub extern "C" fn tn_dict_get(c: *mut Ctx, k: Term, d: Term) -> Term {
    let ctx = cx(c);
    map::find(dict(ctx), k).unwrap_or(d)
}

#[no_mangle]
pub extern "C" fn tn_dict_put(c: *mut Ctx, k: Term, v: Term) -> Term {
    let ctx = cx(c);
    let old = map::find(dict(ctx), k).unwrap_or(NIL);
    let oi = ctx.push(old);
    let d = dict(ctx);
    let nd = map::put(ctx, d, k, v);
    ctx.dict = nd;
    let o = ctx.get(oi);
    ctx.truncate(oi);
    o
}

#[no_mangle]
pub extern "C" fn tn_dict_delete(c: *mut Ctx, k: Term) -> Term {
    let ctx = cx(c);
    let old = map::find(dict(ctx), k).unwrap_or(NIL);
    let oi = ctx.push(old);
    let d = dict(ctx);
    let nd = map::remove(ctx, d, k);
    ctx.dict = nd;
    let o = ctx.get(oi);
    ctx.truncate(oi);
    o
}

#[no_mangle]
pub extern "C" fn tn_dict_all(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let d = dict(ctx);
    map::to_list(ctx, d)
}

#[no_mangle]
pub extern "C" fn tn_node(_c: *mut Ctx) -> Term {
    atom(atoms::intern("nonode@nohost"))
}

/// node(pid | ref | port) — everything is local.
#[no_mangle]
pub extern "C" fn tn_node1(c: *mut Ctx, t: Term) -> Term {
    if is_pid(t) || is_ref(t) {
        return atom(atoms::intern("nonode@nohost"));
    }
    cx(c).badarg_args(&[(1, "not a pid, port or reference")])
}

/// Group leader of `p` (a pid) or nil when it uses the standard I/O.
#[no_mangle]
pub extern "C" fn tn_group_leader_of(_c: *mut Ctx, p: Term) -> Term {
    if !is_pid(p) {
        return NIL;
    }
    match sched::group_leaders().lock().get(&pid_id(p)) {
        Some(&gl) => pid(gl),
        None => NIL,
    }
}

/// Sets (or with nil, clears) the group leader of process `p`.
#[no_mangle]
pub extern "C" fn tn_set_group_leader(_c: *mut Ctx, p: Term, gl: Term) -> Term {
    if !is_pid(p) {
        return atom(a::ERROR);
    }
    let mut m = sched::group_leaders().lock();
    if is_pid(gl) {
        m.insert(pid_id(p), pid_id(gl));
    } else {
        m.remove(&pid_id(p));
    }
    TRUE
}

/// erlang:alias/0: a new reference that routes messages to this process.
#[no_mangle]
pub extern "C" fn tn_alias(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let refid = sched::next_ref_id();
    sched::alias_add(refid, ctx.pid(), false);
    sched::mk_ref_term(ctx, refid)
}

/// Make an existing (monitor) reference an alias of this process.
/// `on_demonitor`: deactivate with the monitor.
#[no_mangle]
pub extern "C" fn tn_alias_ref(c: *mut Ctx, r: Term, on_demonitor: Term) -> Term {
    let ctx = cx(c);
    if !is_ref(r) {
        return ctx.badarg("argument error");
    }
    sched::alias_add(word(r, 1), ctx.pid(), on_demonitor == TRUE);
    r
}

#[no_mangle]
pub extern "C" fn tn_unalias(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    if !is_ref(r) {
        return ctx.badarg_args(&[(1, "not a reference")]);
    }
    boolean(sched::alias_remove(word(r, 1), ctx.pid()))
}

/// spawn_opt with link and/or monitor set up atomically: pid or {pid, ref}.
#[no_mangle]
pub extern "C" fn tn_spawn_opt(c: *mut Ctx, f: Term, link: Term, monitor: Term) -> Term {
    let ctx = cx(c);
    let refid = if monitor == TRUE { Some(sched::next_ref_id()) } else { None };
    match do_spawn(ctx, f, link == TRUE, refid) {
        Ok(p) => match refid {
            Some(r) => {
                let rt = sched::mk_ref_term(ctx, r);
                ctx.tuple(&[pid(p), rt])
            }
            None => pid(p),
        },
        Err(e) => e,
    }
}

/// process_info items for the current process (`ctx`), as a list of
/// `{item, value}`. `running`: whether the process is the caller (status).
pub fn info_items(ctx: &mut Ctx, items: Term, running: bool) -> Term {
    let me = ctx.pid();
    let base = ctx.roots.len();
    let mut n = 0;
    let mut l = items;
    let mut list: Vec<Term> = Vec::new();
    while is_cons(l) {
        list.push(head(l));
        l = tail(l);
    }
    for it in list {
        if !is_atom(it) {
            continue;
        }
        let name = atoms::name(atom_idx(it));
        let v: Term = match name {
            "registered_name" => match sched::proc_state(me, |s| s.name).flatten() {
                Some(a) => atom(a),
                None => NIL_LIST,
            },
            "dictionary" => {
                let d = ctx.dict;
                if d == 0 || d == NIL {
                    NIL_LIST
                } else {
                    map::to_list(ctx, d)
                }
            }
            "messages" => {
                let b2 = ctx.roots.len();
                let k = ctx.saveq.len();
                for i in 0..k {
                    let m = ctx.saveq[i];
                    ctx.roots.push(m);
                }
                let r = ctx.list_from_roots(b2, k, NIL_LIST);
                ctx.roots.truncate(b2);
                r
            }
            "message_queue_len" => small((ctx.saveq.len() + sched::queue_len(me)) as i64),
            "links" => {
                let ls = sched::links_of(me);
                let mut v: Vec<Term> = ls.iter().map(|p| pid(*p)).collect();
                ctx.list_from_vec(&mut v)
            }
            "monitors" => {
                let ms = sched::monitors_of(me);
                let b2 = ctx.roots.len();
                for p in &ms {
                    let t = ctx.tuple(&[atom(a::PROCESS), pid(*p)]);
                    ctx.roots.push(t);
                }
                let r = ctx.list_from_roots(b2, ms.len(), NIL_LIST);
                ctx.roots.truncate(b2);
                r
            }
            "monitored_by" => {
                let ws = sched::proc_state(me, |s| s.monitored_by.iter().map(|(_, w)| *w).collect::<Vec<u64>>()).unwrap_or_default();
                let mut v: Vec<Term> = ws.iter().map(|p| pid(*p)).collect();
                ctx.list_from_vec(&mut v)
            }
            "trap_exit" => boolean(sched::proc_state(me, |s| s.trap_exit).unwrap_or(false)),
            "priority" => atom(sched::proc_state(me, |s| s.priority).unwrap_or(atoms::intern("normal"))),
            "status" => atom(atoms::intern(if running { "running" } else { "waiting" })),
            "initial_call" => {
                let (m, f, ar) = sched::proc_state(me, |s| s.initial_call).unwrap_or((0, 0, 0));
                ctx.tuple(&[atom(m), atom(f), small(ar as i64)])
            }
            "parent" => match sched::proc_state(me, |s| s.parent).flatten() {
                Some(p) => pid(p),
                None => atom(a::UNDEFINED),
            },
            "group_leader" => match sched::group_leaders().lock().get(&me).copied() {
                Some(g) => pid(g),
                None => NIL,
            },
            "heap_size" | "total_heap_size" => small((ctx.heap_words as i64).max(233)),
            "stack_size" => small(ctx.last_stack_words.max(1) as i64),
            "reductions" => small(1000),
            "memory" => small((ctx.heap_words * 8 + 2000) as i64),
            "min_heap_size" => small(233),
            "min_bin_vheap_size" => small(46422),
            "fullsweep_after" => small(65535),
            "garbage_collection" => NIL_LIST,
            "error_handler" => atom(atoms::intern("error_handler")),
            "suspending" => NIL_LIST,
            "current_stacktrace" => ctx.capture_stacktrace_skip(4),
            "current_function" => {
                let st = ctx.capture_stacktrace_skip(4);
                if is_cons(st) {
                    let top = head(st);
                    ctx.tuple(&[tuple_get(top, 0), tuple_get(top, 1), tuple_get(top, 2)])
                } else {
                    ctx.tuple(&[atom(atoms::intern("erlang")), atom(atoms::intern("apply")), small(2)])
                }
            }
            "label" => atom(a::UNDEFINED),
            "message_queue_data" => atom(atoms::intern("on_heap")),
            "catchlevel" => small(0),
            "sequential_trace_token" => NIL_LIST,
            _ => {
                continue;
            }
        };
        ctx.roots.push(v);
        let t = ctx.tuple(&[it, *ctx.roots.last().unwrap()]);
        *ctx.roots.last_mut().unwrap() = t;
        n += 1;
    }
    let r = ctx.list_from_roots(base, n, NIL_LIST);
    ctx.roots.truncate(base);
    r
}

#[no_mangle]
pub extern "C" fn tn_process_info_self(c: *mut Ctx, items: Term) -> Term {
    let ctx = cx(c);
    let ii = ctx.push(items);
    if !ctx.drain() {
        ctx.truncate(ii);
        return NONE;
    }
    let items = ctx.get(ii);
    ctx.truncate(ii);
    info_items(ctx, items, true)
}

/// Ask another process for its info; it replies {:"$tonic_info", ref, info}.
/// Returns the ref, or :undefined if the process is dead.
#[no_mangle]
pub extern "C" fn tn_info_request(c: *mut Ctx, p: Term, items: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(p) {
        return ctx.badarg_args(&[(1, "not a pid")]);
    }
    let target = pid_id(p);
    if !sched::is_alive(target) {
        return atom(a::UNDEFINED);
    }
    let refid = sched::next_ref_id();
    let f = ctx.to_fragment(items);
    sched::send_signal(target, Signal::Info { from: ctx.pid(), refid, items: f });
    sched::mk_ref_term(ctx, refid)
}

#[no_mangle]
pub extern "C" fn tn_set_initial_call(c: *mut Ctx, m: Term, f: Term, a: Term) -> Term {
    let ctx = cx(c);
    if is_atom(m) && is_atom(f) && is_small(a) {
        sched::set_initial_call(ctx.pid(), atom_idx(m), atom_idx(f), small_val(a) as u64);
    }
    TRUE
}

#[no_mangle]
pub extern "C" fn tn_alias_reply(_c: *mut Ctx, r: Term) -> Term {
    if is_ref(r) {
        sched::alias_mark_reply(word(r, 1));
    }
    r
}

/// spawn(m, f, args) with optional link/monitor: the process starts directly
/// in m.f(args), with that initial call. Returns pid or {pid, ref}.
#[no_mangle]
pub extern "C" fn tn_spawn_mfa(c: *mut Ctx, m: Term, f: Term, args: Term, link: Term, monitor: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(m) || !is_atom(f) || !is_list(args) {
        return ctx.badarg("argument error");
    }
    let mut n = 0u64;
    let mut l = args;
    while is_cons(l) {
        n += 1;
        l = tail(l);
    }
    let t = ctx.tuple(&[m, f, args]);
    let frag = ctx.to_fragment(t);
    let refid = if monitor == TRUE { Some(sched::next_ref_id()) } else { None };
    match do_spawn_entry(ctx, Entry::Mfa(frag), link == TRUE, refid) {
        Ok(p) => {
            sched::set_initial_call(p, atom_idx(m), atom_idx(f), n);
            match refid {
                Some(r) => {
                    let rt = sched::mk_ref_term(ctx, r);
                    ctx.tuple(&[pid(p), rt])
                }
                None => pid(p),
            }
        }
        Err(e) => e,
    }
}

#[no_mangle]
pub extern "C" fn tn_set_priority_of(_c: *mut Ctx, p: Term, v: Term) -> Term {
    if is_pid(p) && is_atom(v) {
        sched::set_priority(pid_id(p), atom_idx(v));
    }
    TRUE
}
