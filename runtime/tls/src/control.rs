//! send/recv, closing, options and ownership.

use crate::alert::{self, Failure};
use crate::io::{self, Read};
use crate::{build, data_term, id_of, lookup, packet, reply, Conn, Socket};
use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::net::{a, active_of, ctx_result, list_items, ok_with, operation, option, timeout_of, with_timeout, Active};
use tonic_core::term::*;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub fn conn_of(socket: Term) -> Option<Arc<Conn>> {
    match lookup(socket) {
        Some(Socket::Conn(c)) => Some(c),
        _ => None,
    }
}

pub fn closed(ctx: &mut Ctx) -> Term {
    ctx_result(ctx, None, Some("closed"))
}

#[no_mangle]
pub extern "C" fn tn_ssl_send(c: *mut Ctx, socket: Term, data: Term) -> Term {
    let ctx = cx(c);
    let Some(conn) = conn_of(socket) else { return closed(ctx) };
    let mut bytes = Vec::new();
    if !tonic_core::nif::flatten_iolist(data, &mut bytes) {
        return ctx.badarg("argument error");
    }
    let header = match packet::header(conn.state.lock().packet, bytes.len()) {
        Ok(h) => h,
        Err(reason) => return ctx_result(ctx, None, Some(reason)),
    };
    if !header.is_empty() {
        bytes.splice(0..0, header);
    }
    let (r, op, pid) = operation(ctx);
    tonic_core::sched::runtime().spawn(async move {
        match io::write(&conn, &bytes).await {
            Ok(()) => reply(pid, op, |_| atom(a::OK)),
            Err(f) => reply(pid, op, |env| alert::error(env, &f)),
        }
    });
    r
}

#[no_mangle]
pub extern "C" fn tn_ssl_recv(c: *mut Ctx, socket: Term, length: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let conn = conn_of(socket);
    let length = (is_small(length) && small_val(length) >= 0).then(|| small_val(length) as usize);
    let timeout = timeout_of(timeout);
    let (r, op, pid) = operation(ctx);
    let fail = |f: Failure| reply(pid, op, |env| alert::error(env, &f));
    let Some(conn) = conn else {
        fail(Failure::Posix("closed"));
        return r;
    };
    let Some(length) = length else {
        fail(Failure::Posix("einval"));
        return r;
    };
    if conn.state.lock().active != Active::Passive {
        fail(Failure::Posix("einval"));
        return r;
    }
    tonic_core::sched::runtime().spawn(async move {
        match with_timeout(timeout, io::read(&conn, length)).await {
            Some(Read::Data(data)) => {
                let binary = conn.state.lock().binary;
                reply(pid, op, |env| {
                    let d = data_term(env, &data, binary);
                    ok_with(env, d)
                })
            }
            Some(Read::Closed) => reply(pid, op, |env| alert::error(env, &Failure::Posix("closed"))),
            Some(Read::Failed(f)) => reply(pid, op, |env| alert::error(env, &f)),
            None => reply(pid, op, |env| alert::error(env, &Failure::Posix("timeout"))),
        }
    });
    r
}

#[no_mangle]
pub extern "C" fn tn_ssl_close(_c: *mut Ctx, socket: Term) -> Term {
    if let Some(s) = id_of(socket).and_then(crate::remove) {
        crate::close_socket(s);
    }
    atom(a::OK)
}

/// `:ssl.shutdown/2`; answers by message once close_notify is out.
#[no_mangle]
pub extern "C" fn tn_ssl_shutdown(c: *mut Ctx, socket: Term, how: Term) -> Term {
    let ctx = cx(c);
    let conn = conn_of(socket);
    let how = is_atom(how).then(|| tonic_core::atoms::name(atom_idx(how)));
    if !matches!(how, Some("read" | "write" | "read_write")) {
        return ctx.badarg("argument error");
    }
    let Some(conn) = conn else { return closed(ctx) };
    if how == Some("read") {
        return atom(a::OK);
    }
    let (r, op, pid) = operation(ctx);
    tonic_core::sched::runtime().spawn(async move {
        let mut writer = conn.writer.lock().await;
        let _ = writer.shutdown().await;
        reply(pid, op, |_| atom(a::OK));
    });
    r
}

#[no_mangle]
pub extern "C" fn tn_ssl_controlling_process(c: *mut Ctx, socket: Term, owner: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(owner) {
        return ctx.badarg("argument error");
    }
    let (me, new) = (ctx.pid(), pid_id(owner));
    let slot = |current: &mut u64| {
        if *current != me {
            return Some("not_owner");
        }
        *current = new;
        None
    };
    let failed = match lookup(socket) {
        Some(Socket::Conn(c)) => slot(&mut c.state.lock().owner),
        Some(Socket::Listener(l)) => slot(&mut l.owner.lock()),
        Some(Socket::Pending(p)) => slot(&mut p.owner.lock()),
        None => Some("closed"),
    };
    ctx_result(ctx, None, failed)
}

#[no_mangle]
pub extern "C" fn tn_ssl_setopts(c: *mut Ctx, socket: Term, opts: Term) -> Term {
    let ctx = cx(c);
    let target = lookup(socket);
    let items: Vec<(&'static str, Term)> = list_items(opts).into_iter().filter_map(option).collect();
    let Some(target) = target else { return closed(ctx) };
    match target {
        Socket::Conn(conn) => {
            for (name, v) in items {
                let mut st = conn.state.lock();
                match name {
                    "active" => {
                        let Some(active) = active_of(v) else { return ctx_result(ctx, None, Some("einval")) };
                        st.active = match (st.active, active) {
                            (Active::Count(n), Active::Count(m)) => Active::Count(n + m),
                            (_, other) => other,
                        };
                        if let Active::Count(n) = st.active {
                            if n <= 0 {
                                st.active = Active::Passive;
                                drop(st);
                                let id = id_of(socket).unwrap_or(0);
                                let owner = conn.state.lock().owner;
                                tonic_core::net::deliver(owner, |env| {
                                    let s = crate::sock_term(env, id);
                                    env.tuple(&[a("ssl_passive"), s])
                                });
                                continue;
                            }
                        }
                    }
                    "binary" => st.binary = true,
                    "list" => st.binary = false,
                    "mode" => st.binary = v == a("binary"),
                    "packet" => match packet::of(v) {
                        Some(p) => st.packet = p,
                        None => return ctx_result(ctx, None, Some("einval")),
                    },
                    "nodelay" => {}
                    _ => {}
                }
            }
            if let Some(id) = id_of(socket) {
                io::start_reader(id, conn);
            }
            atom(a::OK)
        }
        Socket::Listener(l) => {
            let mut sub = l.opts.lock().clone();
            let applied = crate::options::apply(&mut sub, opts);
            match applied {
                Ok(()) => {
                    *l.opts.lock() = sub;
                    atom(a::OK)
                }
                Err(_) => ctx_result(ctx, None, Some("einval")),
            }
        }
        Socket::Pending(_) => atom(a::OK),
    }
}

/// `{name, value}` pairs for the socket options asked for, in order.
#[no_mangle]
pub extern "C" fn tn_ssl_getopts(c: *mut Ctx, socket: Term, names: Term) -> Term {
    let ctx = cx(c);
    let target = lookup(socket);
    let names: Vec<&'static str> =
        list_items(names).into_iter().filter(|t| is_atom(*t)).map(|t| tonic_core::atoms::name(atom_idx(t))).collect();
    let (active, binary, packet) = match &target {
        Some(Socket::Conn(conn)) => {
            let st = conn.state.lock();
            (st.active, st.binary, st.packet)
        }
        Some(Socket::Listener(l)) => {
            let o = l.opts.lock();
            (o.active, o.binary, o.packet)
        }
        Some(Socket::Pending(p)) => (p.opts.active, p.opts.binary, p.opts.packet),
        None => return closed(ctx),
    };
    build(ctx, |env| {
        let pairs: Vec<Term> = names
            .iter()
            .filter_map(|name| {
                let value = match *name {
                    "active" => match active {
                        Active::Passive => FALSE,
                        Active::On => TRUE,
                        Active::Once => a("once"),
                        Active::Count(n) => small(n),
                    },
                    "mode" => a(if binary { "binary" } else { "list" }),
                    "packet" => packet::term(packet),
                    "nodelay" | "keepalive" | "reuseaddr" => FALSE,
                    "header" | "packet_size" => small(0),
                    _ => return None,
                };
                Some(value).map(|v| (name, v))
            })
            .map(|(name, v)| env.tuple(&[a(name), v]))
            .collect();
        let list = pairs.iter().rev().fold(NIL_LIST, |tail, p| env.cons(*p, tail));
        ok_with(env, list)
    })
}
