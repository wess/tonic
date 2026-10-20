//! `:erlang` port BIFs, called as `:tonic.port_*` from lib/erlang.ex.

use super::spawn::Bad;
use super::{lookup, OpenError, Port};
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::nif::Env;
use crate::term::*;

fn a(name: &str) -> Term {
    atom(crate::atoms::intern(name))
}

#[no_mangle]
pub extern "C" fn tn_port_open(c: *mut Ctx, name: Term, opts: Term) -> Term {
    let ctx = cx(c);
    match super::open(ctx.pid(), name, opts) {
        Ok(id) => super::term(ctx, id),
        Err(OpenError::Badarg(Bad::Name)) => ctx.badarg_args(&[(1, "invalid port name")]),
        Err(OpenError::Badarg(Bad::Options)) => ctx.badarg_args(&[(2, "invalid option in list")]),
        Err(OpenError::Posix(reason)) => {
            let r = a(reason);
            ctx.raise_error(r)
        }
    }
}

#[no_mangle]
pub extern "C" fn tn_port_command(c: *mut Ctx, port: Term, data: Term) -> Term {
    let ctx = cx(c);
    let mut bytes = Vec::new();
    if !crate::nif::flatten_iolist(data, &mut bytes) {
        if let Some(p) = lookup(port) {
            super::bad_value(&p);
        }
        return ctx.badarg_args(&[(2, "not an iodata term")]);
    }
    match lookup(port) {
        Some(p) if super::command(&p, bytes) => TRUE,
        _ => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_port_close(c: *mut Ctx, port: Term) -> Term {
    let ctx = cx(c);
    let normal = ctx.to_fragment(atom(a::NORMAL));
    match lookup(port) {
        Some(p) if super::close(&p, normal, None) => TRUE,
        _ => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_port_connect(c: *mut Ctx, port: Term, to: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(to) {
        return ctx.badarg_args(&[(2, "not a pid")]);
    }
    match lookup(port) {
        Some(p) if crate::sched::is_alive(pid_id(to)) && super::connect(&p, pid_id(to)) => TRUE,
        _ => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_ports(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let ids = super::all();
    let base = ctx.roots.len();
    for id in &ids {
        let t = super::term(ctx, *id);
        ctx.roots.push(t);
    }
    let l = ctx.list_from_roots(base, ids.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

#[no_mangle]
pub extern "C" fn tn_port_to_string(c: *mut Ctx, port: Term) -> Term {
    let ctx = cx(c);
    if !is_port(port) {
        return ctx.badarg_args(&[(1, "not a port identifier")]);
    }
    let s = format!("#Port<0.{}>", word(port, 1));
    ctx.str(&s)
}

const ITEMS: [&str; 7] = ["name", "links", "id", "connected", "input", "output", "os_pid"];

fn charlist(env: &mut Env, s: &str) -> Term {
    s.chars().rev().fold(NIL_LIST, |acc, ch| env.cons(small(ch as i64), acc))
}

fn pids(env: &mut Env, ids: &[u64]) -> Term {
    ids.iter().rev().fold(NIL_LIST, |acc, p| env.cons(pid(*p), acc))
}

/// One `{item, value}` of port_info/2, built outside the process heap.
fn item(env: &mut Env, port: &Port, name: &str) -> Option<Term> {
    let st = port.st.lock();
    let value = match name {
        "name" => charlist(env, &port.name),
        "links" => {
            let mut ls: Vec<u64> = st.links.iter().copied().collect();
            ls.sort_unstable();
            pids(env, &ls)
        }
        "id" => small(port.id as i64),
        "connected" => pid(st.owner),
        "input" => small(st.input as i64),
        "output" => small(st.output as i64),
        "os_pid" => small(port.os_pid as i64),
        // an unregistered port answers [] rather than a tuple
        "registered_name" => return Some(NIL_LIST),
        "memory" => small(std::mem::size_of::<Port>() as i64),
        "queue_size" => small(0),
        "parallelism" => FALSE,
        "locking" => a("port_level"),
        "monitors" => NIL_LIST,
        "monitored_by" => {
            let ws: Vec<u64> = st.monitors.iter().filter(|(r, _)| crate::sched::monitor_active(*r)).map(|(_, w)| *w).collect();
            pids(env, &ws)
        }
        _ => return None,
    };
    Some(env.tuple(&[a(name), value]))
}

fn into_process(ctx: &mut Ctx, env: &Env, t: Term) -> Term {
    ctx.copy_in_by(t, &|p| env.owns_address(p))
}

/// The registered-name lookup that atoms get on the BEAM finds no ports.
fn not_a_port(ctx: &mut Ctx, port: Term) -> Option<Term> {
    if is_port(port) || is_atom(port) {
        return None;
    }
    Some(ctx.badarg("argument error"))
}

#[no_mangle]
pub extern "C" fn tn_port_info(c: *mut Ctx, port: Term) -> Term {
    let ctx = cx(c);
    if let Some(e) = not_a_port(ctx, port) {
        return e;
    }
    let Some(p) = lookup(port) else { return atom(a::UNDEFINED) };
    let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
    let list = ITEMS.iter().rev().fold(NIL_LIST, |acc, name| {
        let t = item(&mut env, &p, name).unwrap_or(NIL_LIST);
        env.cons(t, acc)
    });
    into_process(ctx, &env, list)
}

#[no_mangle]
pub extern "C" fn tn_port_info_item(c: *mut Ctx, port: Term, name: Term) -> Term {
    let ctx = cx(c);
    if let Some(e) = not_a_port(ctx, port) {
        return e;
    }
    let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
    let p = lookup(port);
    let found = is_atom(name).then(|| crate::atoms::name(atom_idx(name))).and_then(|n| match &p {
        Some(p) => item(&mut env, p, n),
        None => is_known(n).then_some(atom(a::UNDEFINED)),
    });
    match found {
        Some(t) => into_process(ctx, &env, t),
        None => ctx.badarg_args(&[(2, "invalid option in list")]),
    }
}

fn is_known(name: &str) -> bool {
    ITEMS.contains(&name)
        || ["registered_name", "memory", "queue_size", "parallelism", "locking", "monitors", "monitored_by"].contains(&name)
}

/// os:cmd's command as text: a flat or deep character list or an atom.
/// Trailing NULs are dropped as OTP does; any other NUL is an error.
#[no_mangle]
pub extern "C" fn tn_port_cmd_text(c: *mut Ctx, cmd: Term) -> Term {
    let ctx = cx(c);
    let mut chars = Vec::new();
    let ok = if is_atom(cmd) {
        chars.extend(crate::atoms::name(atom_idx(cmd)).chars());
        true
    } else {
        deep_chars(cmd, &mut chars)
    };
    while chars.last() == Some(&'\0') {
        chars.pop();
    }
    if !ok || chars.contains(&'\0') {
        return ctx.badarg_args(&[(1, "not a list of characters")]);
    }
    let s: String = chars.into_iter().collect();
    ctx.str(&s)
}

fn deep_chars(mut l: Term, out: &mut Vec<char>) -> bool {
    while is_cons(l) {
        let h = head(l);
        if is_small(h) {
            match char::from_u32(small_val(h) as u32) {
                Some(ch) if small_val(h) >= 0 => out.push(ch),
                _ => return false,
            }
        } else if !is_list(h) || !deep_chars(h, out) {
            return false;
        }
        l = tail(l);
    }
    l == NIL_LIST
}
