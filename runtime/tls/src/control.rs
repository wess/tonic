//! send/recv, closing, options and ownership.

use crate::alert::{self, Failure};
use crate::io::{self, Read};
use crate::{build, data_term, id_of, lookup, packet, reply, Conn, Socket};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::net::{
    a, active_of, ctx_result, list_items, ok_with, operation, option, timeout_of, with_timeout,
    Active,
};
use tonic_core::term::*;

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
    let Some(conn) = conn_of(socket) else {
        return closed(ctx);
    };
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
            Some(Read::Closed) => {
                reply(pid, op, |env| alert::error(env, &Failure::Posix("closed")))
            }
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
    if !tonic_core::net::proper_list(opts) {
        return ctx_result(ctx, None, Some("einval"));
    }
    let target = lookup(socket);
    let Some(items) = list_items(opts)
        .into_iter()
        .map(option)
        .collect::<Option<Vec<_>>>()
    else {
        return ctx_result(ctx, None, Some("einval"));
    };
    let Some(target) = target else {
        return closed(ctx);
    };
    match target {
        Socket::Conn(conn) => {
            for (name, v) in items {
                let mut st = conn.state.lock();
                match name {
                    "active" => {
                        let Some(active) = active_of(v) else {
                            return ctx_result(ctx, None, Some("einval"));
                        };
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
                    "mode" if v == a("binary") || v == a("list") => st.binary = v == a("binary"),
                    "packet" => match packet::of(v) {
                        Some(p) => st.packet = p,
                        None => return ctx_result(ctx, None, Some("einval")),
                    },
                    "buffer" if is_small(v) && small_val(v) > 0 => {
                        st.read_buffer = small_val(v) as usize
                    }
                    "packet_size" if is_small(v) && small_val(v) >= 0 => {
                        st.packet_size = small_val(v) as usize
                    }
                    "header" if v == small(0) => {}
                    "nodelay" | "keepalive" | "reuseaddr" | "recbuf" | "sndbuf" => {
                        if let Err(e) = crate::tcp::set(&conn, name, v) {
                            return ctx_result(ctx, None, Some(tonic_core::net::posix(&e)));
                        }
                    }
                    _ => return ctx_result(ctx, None, Some("einval")),
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
                    if let Err(error) = crate::tcp::apply(&l.io, &sub) {
                        return ctx_result(ctx, None, Some(tonic_core::net::posix(&error)));
                    }
                    *l.opts.lock() = sub;
                    atom(a::OK)
                }
                Err(_) => ctx_result(ctx, None, Some("einval")),
            }
        }
        Socket::Pending(_) => ctx_result(ctx, None, Some("einval")),
    }
}

/// `{name, value}` pairs for the socket options asked for, in order.
#[no_mangle]
pub extern "C" fn tn_ssl_getopts(c: *mut Ctx, socket: Term, names: Term) -> Term {
    let ctx = cx(c);
    if !tonic_core::net::proper_list(names) {
        return ctx_result(ctx, None, Some("einval"));
    }
    let target = lookup(socket);
    let Some(names) = list_items(names)
        .into_iter()
        .map(|term| is_atom(term).then(|| tonic_core::atoms::name(atom_idx(term))))
        .collect::<Option<Vec<_>>>()
    else {
        return ctx_result(ctx, None, Some("einval"));
    };
    let (active, binary, packet, buffer, packet_size) = match &target {
        Some(Socket::Conn(conn)) => {
            let st = conn.state.lock();
            (
                st.active,
                st.binary,
                st.packet,
                st.read_buffer,
                st.packet_size,
            )
        }
        Some(Socket::Listener(l)) => {
            let o = l.opts.lock();
            (o.active, o.binary, o.packet, o.buffer, o.packet_size)
        }
        Some(Socket::Pending(p)) => (
            p.opts.active,
            p.opts.binary,
            p.opts.packet,
            p.opts.buffer,
            p.opts.packet_size,
        ),
        None => return closed(ctx),
    };
    let mut values = Vec::new();
    for name in names {
        let value = match name {
            "active" => match active {
                Active::Passive => FALSE,
                Active::On => TRUE,
                Active::Once => a("once"),
                Active::Count(n) => small(n),
            },
            "mode" => a(if binary { "binary" } else { "list" }),
            "packet" => packet::term(packet),
            "header" => small(0),
            "buffer" => small(buffer as i64),
            "packet_size" => small(packet_size as i64),
            "nodelay" | "keepalive" | "reuseaddr" | "sndbuf" | "recbuf" => match &target {
                Some(Socket::Conn(conn)) => match crate::tcp::get(conn, name) {
                    Ok(value) => value,
                    Err(e) => return ctx_result(ctx, None, Some(tonic_core::net::posix(&e))),
                },
                Some(Socket::Listener(listener)) => match crate::tcp::get_io(&listener.io, name) {
                    Ok(value) => value,
                    Err(error) => {
                        return ctx_result(ctx, None, Some(tonic_core::net::posix(&error)))
                    }
                },
                _ => return ctx_result(ctx, None, Some("einval")),
            },
            _ => return ctx_result(ctx, None, Some("einval")),
        };
        values.push((name, value));
    }
    build(ctx, |env| {
        let pairs: Vec<Term> = values
            .iter()
            .map(|(name, value)| env.tuple(&[a(name), *value]))
            .collect();
        let list = pairs
            .iter()
            .rev()
            .fold(NIL_LIST, |tail, p| env.cons(*p, tail));
        ok_with(env, list)
    })
}
