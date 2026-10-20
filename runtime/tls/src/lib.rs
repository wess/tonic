//! TLS sockets for `:ssl`, on rustls with the ring provider.
//!
//! The design follows `net.rs`: sockets live in a registry keyed by a
//! reference id, waiting operations run as tokio tasks and answer with
//! `{:"$tonic_ssl", ref, result}`, and active mode delivers `{:ssl, s, data}`,
//! `{:ssl_closed, s}`, `{:ssl_error, s, reason}` and `{:ssl_passive, s}`.
//! A socket term is `{:sslsocket, ref, :tls}`.
//!
//! A crate of its own, called only from `:ssl` and `:public_key` code, so
//! programs that never use them link none of it (ring's assembly cannot be
//! dead-stripped); the owner-exit hook is installed on first use.

pub mod alert;
pub mod config;
pub mod control;
pub mod info;
pub mod io;
pub mod open;
pub mod options;
pub mod packet;
pub mod roots;

use tonic_core::heap::Ctx;
use tonic_core::net::{a, deliver, socket_term};
use tonic_core::nif::Env;
use tonic_core::term::*;
use options::Opts;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::TlsStream;

pub type Tls = TlsStream<TcpStream>;

/// What the handshake settled, kept for `connection_information` and friends.
pub struct Info {
    pub server: bool,
    pub protocol: &'static str,
    pub suite: String,
    pub alpn: Option<Vec<u8>>,
    pub sni: Option<String>,
    pub peercerts: Vec<Vec<u8>>,
}

pub struct State {
    pub owner: u64,
    pub active: tonic_core::net::Active,
    pub binary: bool,
    pub packet: packet::Packet,
    pub buffer: Vec<u8>,
    pub reading: bool,
    pub eof: bool,
    pub stats: [u64; 4],
}

pub struct Conn {
    pub reader: tokio::sync::Mutex<ReadHalf<Tls>>,
    pub writer: tokio::sync::Mutex<WriteHalf<Tls>>,
    pub state: parking_lot::Mutex<State>,
    pub closed: tokio::sync::watch::Sender<bool>,
    pub info: Info,
    pub local: Option<SocketAddr>,
    pub peer: Option<SocketAddr>,
}

pub struct Listener {
    pub io: tokio::net::TcpListener,
    pub owner: parking_lot::Mutex<u64>,
    pub opts: parking_lot::Mutex<Opts>,
    /// Built at listen; OTP reports certificate problems at handshake.
    pub config: Result<Arc<rustls::ServerConfig>, alert::Failure>,
    pub closed: tokio::sync::watch::Sender<bool>,
}

/// Accepted by `transport_accept`, waiting for `handshake`.
pub struct Pending {
    pub io: parking_lot::Mutex<Option<TcpStream>>,
    pub owner: parking_lot::Mutex<u64>,
    pub opts: Opts,
    pub listener: Arc<Listener>,
}

#[derive(Clone)]
pub enum Socket {
    Listener(Arc<Listener>),
    Pending(Arc<Pending>),
    Conn(Arc<Conn>),
}

static SOCKETS: parking_lot::Mutex<Option<HashMap<u64, Socket>>> = parking_lot::const_mutex(None);

pub fn register(id: u64, socket: Socket) {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| tonic_core::sched::on_exit(owner_exited));
    SOCKETS.lock().get_or_insert_with(HashMap::new).insert(id, socket);
}

/// The registry id inside `{:sslsocket, ref, _}`.
pub fn id_of(t: Term) -> Option<u64> {
    if !is_tuple(t) || tuple_size(t) != 3 || !is_ref(tuple_get(t, 1)) {
        return None;
    }
    Some(word(tuple_get(t, 1), 1))
}

pub fn lookup(t: Term) -> Option<Socket> {
    let id = id_of(t)?;
    SOCKETS.lock().as_ref().and_then(|m| m.get(&id).cloned())
}

pub fn remove(id: u64) -> Option<Socket> {
    SOCKETS.lock().as_mut().and_then(|m| m.remove(&id))
}

pub fn sock_term(env: &mut Env, id: u64) -> Term {
    let r = socket_term(env, id);
    env.tuple(&[a("sslsocket"), r, a("tls")])
}

/// `{:"$tonic_ssl", ref, result}` for the operation `op`.
pub fn reply(pid: u64, op: u64, result: impl FnOnce(&mut Env) -> Term) {
    deliver(pid, |env| {
        let r = socket_term(env, op);
        let value = result(env);
        env.tuple(&[a("$tonic_ssl"), r, value])
    })
}

/// Builds a term outside the heap and copies it into the calling process.
pub fn build(ctx: &mut Ctx, f: impl FnOnce(&mut Env) -> Term) -> Term {
    let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
    let t = f(&mut env);
    ctx.copy_in_by(t, &|p| env.owns_address(p))
}

pub fn charlist(env: &mut Env, s: &str) -> Term {
    s.bytes().rev().fold(NIL_LIST, |tail, b| env.cons(small(b as i64), tail))
}

pub fn data_term(env: &mut Env, data: &[u8], binary: bool) -> Term {
    if binary {
        env.bytes(data)
    } else {
        data.iter().rev().fold(NIL_LIST, |tail, b| env.cons(small(*b as i64), tail))
    }
}

pub fn close_socket(socket: Socket) {
    match socket {
        Socket::Conn(c) => io::close(c),
        Socket::Listener(l) => {
            let _ = l.closed.send(true);
        }
        Socket::Pending(p) => drop(p.io.lock().take()),
    }
}

fn owner_of(socket: &Socket) -> u64 {
    match socket {
        Socket::Conn(c) => c.state.lock().owner,
        Socket::Listener(l) => *l.owner.lock(),
        Socket::Pending(p) => *p.owner.lock(),
    }
}

/// Sockets close with their controlling process, as on the BEAM.
fn owner_exited(pid: u64) {
    let owned: Vec<Socket> = {
        let mut sockets = SOCKETS.lock();
        let Some(m) = sockets.as_mut() else { return };
        let ids: Vec<u64> = m.iter().filter(|(_, s)| owner_of(s) == pid).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| m.remove(&id)).collect()
    };
    owned.into_iter().for_each(close_socket);
}
