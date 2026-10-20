//! Addresses, negotiated parameters, certificates and statistics.

use crate::control::{closed, conn_of};
use crate::{build, lookup, Socket};
use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::net::{a, ctx_result, ok_with};
use tonic_core::nif::Env;
use tonic_core::term::*;

fn address(env: &mut Env, addr: std::net::SocketAddr) -> Term {
    let ip = tonic_core::net::ip_term(env, addr.ip());
    env.tuple(&[ip, small(addr.port() as i64)])
}

fn endpoint(c: *mut Ctx, socket: Term, peer: bool) -> Term {
    let ctx = cx(c);
    let addr = match lookup(socket) {
        Some(Socket::Conn(conn)) => if peer { conn.peer } else { conn.local },
        Some(Socket::Listener(l)) if !peer => l.io.local_addr().ok(),
        Some(Socket::Pending(p)) => p.io.lock().as_ref().and_then(|t| if peer { t.peer_addr().ok() } else { t.local_addr().ok() }),
        Some(Socket::Listener(_)) => return ctx_result(ctx, None, Some("enotconn")),
        None => return closed(ctx),
    };
    match addr {
        Some(addr) => build(ctx, |env| {
            let t = address(env, addr);
            ok_with(env, t)
        }),
        None => ctx_result(ctx, None, Some("enotconn")),
    }
}

#[no_mangle]
pub extern "C" fn tn_ssl_sockname(c: *mut Ctx, socket: Term) -> Term {
    endpoint(c, socket, false)
}

#[no_mangle]
pub extern "C" fn tn_ssl_peername(c: *mut Ctx, socket: Term) -> Term {
    endpoint(c, socket, true)
}

/// `{:ok, {protocol, suite_name, alpn | :undefined, sni | :undefined}}`;
/// lib/ssl.ex turns it into OTP's connection information.
#[no_mangle]
pub extern "C" fn tn_ssl_info(c: *mut Ctx, socket: Term) -> Term {
    let ctx = cx(c);
    let Some(conn) = conn_of(socket) else { return closed(ctx) };
    let info = &conn.info;
    build(ctx, |env| {
        let suite = env.bytes(info.suite.as_bytes());
        let alpn = match &info.alpn {
            Some(p) => env.bytes(p),
            None => a("undefined"),
        };
        let sni = match &info.sni {
            Some(h) => env.bytes(h.as_bytes()),
            None => a("undefined"),
        };
        let t = env.tuple(&[a(info.protocol), suite, alpn, sni]);
        ok_with(env, t)
    })
}

/// The peer's certificate chain as DER binaries, leaf first.
#[no_mangle]
pub extern "C" fn tn_ssl_peercerts(c: *mut Ctx, socket: Term) -> Term {
    let ctx = cx(c);
    let Some(conn) = conn_of(socket) else { return closed(ctx) };
    build(ctx, |env| {
        let list = conn.info.peercerts.iter().rev().fold(NIL_LIST, |tail, der| {
            let b = env.bytes(der);
            env.cons(b, tail)
        });
        ok_with(env, list)
    })
}

#[no_mangle]
pub extern "C" fn tn_ssl_getstat(c: *mut Ctx, socket: Term) -> Term {
    let ctx = cx(c);
    let stats = match lookup(socket) {
        Some(Socket::Conn(conn)) => conn.state.lock().stats,
        Some(_) => [0; 4],
        None => return closed(ctx),
    };
    build(ctx, |env| {
        let names = ["recv_oct", "recv_cnt", "send_oct", "send_cnt"];
        let pairs: Vec<Term> = names.iter().zip(stats).map(|(n, v)| env.tuple(&[a(n), small(v as i64)])).collect();
        let list = pairs.iter().rev().fold(NIL_LIST, |tail, p| env.cons(*p, tail));
        ok_with(env, list)
    })
}
