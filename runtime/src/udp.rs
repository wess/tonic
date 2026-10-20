//! UDP sockets for `:gen_udp`.
//!
//! Same shape as the TCP sockets in net.rs: references index a registry,
//! waiting operations run as tokio tasks and answer with
//! `{:"$tonic_udp", ref, result}`. Active mode delivers
//! `{:udp, socket, ip, port, data}` and `{:udp_passive, socket}` to the owner.

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::net::{
    a, active_of, ctx_result, deliver, error_with, host_of, ip_of, ip_term, list_items, ok_with, operation, option, posix,
    socket_term, timeout_of, with_timeout, Active,
};
use crate::nif::Env;
use crate::term::*;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;

struct State {
    owner: u64,
    active: Active,
    list: bool,
    reading: bool,
    closed: bool,
    // A datagram the active reader took after the socket turned passive.
    pending: Option<(SocketAddr, Vec<u8>)>,
    recv_oct: u64,
    recv_cnt: u64,
    send_oct: u64,
    send_cnt: u64,
}

pub(crate) struct Udp {
    io: UdpSocket,
    state: parking_lot::Mutex<State>,
    reads: tokio::sync::Mutex<()>,
    closed: tokio::sync::Notify,
}

static SOCKETS: parking_lot::Mutex<Option<HashMap<u64, Arc<Udp>>>> = parking_lot::const_mutex(None);

pub(crate) fn lookup(t: Term) -> Option<Arc<Udp>> {
    if !is_ref(t) {
        return None;
    }
    SOCKETS.lock().as_ref().and_then(|m| m.get(&word(t, 1)).cloned())
}

/// `{:"$tonic_udp", ref, result}` for the operation `op`.
fn reply(pid: u64, op: u64, result: impl FnOnce(&mut Env) -> Term) {
    deliver(pid, |env| {
        let r = socket_term(env, op);
        let value = result(env);
        env.tuple(&[a("$tonic_udp"), r, value])
    })
}

fn packet_term(env: &mut Env, data: &[u8], list: bool) -> Term {
    if list {
        data.iter().rev().fold(NIL_LIST, |t, b| env.cons(small(*b as i64), t))
    } else {
        env.bytes(data)
    }
}

// ---------------------------------------------------------------------------
// Opening
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Config {
    ip: Option<IpAddr>,
    inet6: bool,
    active: Option<Active>,
    binary: bool,
    reuseaddr: bool,
    reuseport: bool,
    broadcast: bool,
    recbuf: Option<usize>,
    sndbuf: Option<usize>,
}

fn address_option(v: Term) -> Option<IpAddr> {
    match is_atom(v).then(|| crate::atoms::name(atom_idx(v))) {
        Some("loopback") => Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        Some("any") => Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
        _ => ip_of(v),
    }
}

fn size_option(v: Term) -> Option<usize> {
    (is_small(v) && small_val(v) > 0).then(|| small_val(v) as usize)
}

/// None for an option OTP rejects with `badarg`.
fn configure(opts: Term) -> Option<Config> {
    let mut config = Config::default();
    for item in list_items(opts) {
        match option(item)? {
            ("binary", _) => config.binary = true,
            ("list", _) => config.binary = false,
            ("mode", v) => config.binary = v == a("binary"),
            ("inet", _) => config.inet6 = false,
            ("inet6", _) => config.inet6 = true,
            ("ip" | "ifaddr", v) => config.ip = Some(address_option(v)?),
            ("active", v) => config.active = Some(active_of(v)?),
            ("reuseaddr", v) => config.reuseaddr = v == TRUE,
            ("reuseport" | "reuseport_lb", v) => config.reuseport = v == TRUE,
            ("broadcast", v) => config.broadcast = v == TRUE,
            ("recbuf", v) => config.recbuf = Some(size_option(v)?),
            ("sndbuf", v) => config.sndbuf = Some(size_option(v)?),
            // Accepted for compatibility; the runtime has no use for them.
            ("buffer" | "read_packets" | "deliver" | "dontroute" | "priority" | "tos" | "ttl" | "recvtos" | "recvttl"
            | "high_msgq_watermark" | "low_msgq_watermark" | "multicast_ttl" | "multicast_loop" | "multicast_if"
            | "exclusiveaddruse" | "debug", _) => {}
            _ => return None,
        }
    }
    Some(config)
}

fn bind(config: &Config, port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let ip = config.ip.unwrap_or(if config.inet6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    });
    let addr = SocketAddr::new(ip, port);
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(config.reuseaddr)?;
    if config.reuseport {
        socket.set_reuse_port(true)?;
    }
    if config.broadcast {
        socket.set_broadcast(true)?;
    }
    if let Some(n) = config.recbuf {
        socket.set_recv_buffer_size(n)?;
    }
    if let Some(n) = config.sndbuf {
        socket.set_send_buffer_size(n)?;
    }
    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    let _guard = crate::sched::runtime().enter();
    UdpSocket::from_std(socket.into())
}

/// `:badarg` (for the wrapper to raise) on a bad port or option.
#[no_mangle]
pub extern "C" fn tn_udp_open(c: *mut Ctx, port: Term, opts: Term) -> Term {
    let ctx = cx(c);
    if !is_small(port) || !(0..=65535).contains(&small_val(port)) {
        return a("badarg");
    }
    let Some(config) = configure(opts) else {
        return a("badarg");
    };
    let io = match bind(&config, small_val(port) as u16) {
        Ok(io) => io,
        Err(e) => return ctx_result(ctx, None, Some(posix(&e))),
    };
    let active = config.active.unwrap_or(Active::On);
    let socket = Arc::new(Udp {
        io,
        state: parking_lot::Mutex::new(State {
            owner: ctx.pid(),
            active,
            list: !config.binary,
            reading: false,
            closed: false,
            pending: None,
            recv_oct: 0,
            recv_cnt: 0,
            send_oct: 0,
            send_cnt: 0,
        }),
        reads: tokio::sync::Mutex::new(()),
        closed: tokio::sync::Notify::new(),
    });
    let id = crate::sched::next_ref_id();
    SOCKETS.lock().get_or_insert_with(HashMap::new).insert(id, socket.clone());
    start_reader(id, socket);
    let t = crate::sched::mk_ref_term(ctx, id);
    ctx_result(ctx, Some(t), None)
}

// ---------------------------------------------------------------------------
// Receiving
// ---------------------------------------------------------------------------

enum Received {
    Packet(SocketAddr, Vec<u8>),
    Closed,
    Failed(&'static str),
}

/// One datagram. The caller holds `reads`, so a stashed datagram cannot be
/// overtaken by a later one.
async fn receive(socket: &Udp) -> Received {
    if let Some((from, data)) = socket.state.lock().pending.take() {
        return Received::Packet(from, data);
    }
    let closed = socket.closed.notified();
    tokio::pin!(closed);
    // Registered before the flag check so a close in between still wakes us.
    closed.as_mut().enable();
    if socket.state.lock().closed {
        return Received::Closed;
    }
    let mut buffer = vec![0u8; 65536];
    tokio::select! {
        r = socket.io.recv_from(&mut buffer) => match r {
            Ok((n, from)) => {
                buffer.truncate(n);
                let mut st = socket.state.lock();
                st.recv_oct += n as u64;
                st.recv_cnt += 1;
                Received::Packet(from, buffer)
            }
            Err(e) => Received::Failed(posix(&e)),
        },
        _ = &mut closed => Received::Closed,
    }
}

fn received_term(env: &mut Env, from: SocketAddr, data: &[u8], list: bool) -> Term {
    let ip = ip_term(env, from.ip());
    let packet = packet_term(env, data, list);
    let triple = env.tuple(&[ip, small(from.port() as i64), packet]);
    ok_with(env, triple)
}

#[no_mangle]
pub extern "C" fn tn_udp_recv(c: *mut Ctx, socket: Term, _length: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    // Read the arguments before allocating the reference: allocation can move them.
    let udp = lookup(socket);
    let timeout = timeout_of(timeout);
    let (r, op, pid) = operation(ctx);
    let Some(udp) = udp else {
        reply(pid, op, |env| error_with(env, "closed"));
        return r;
    };
    if udp.state.lock().active != Active::Passive {
        reply(pid, op, |env| error_with(env, "einval"));
        return r;
    }
    crate::sched::runtime().spawn(async move {
        let got = with_timeout(timeout, async {
            let _guard = udp.reads.lock().await;
            receive(&udp).await
        })
        .await;
        match got {
            Some(Received::Packet(from, data)) => {
                let list = udp.state.lock().list;
                reply(pid, op, |env| received_term(env, from, &data, list))
            }
            Some(Received::Closed) => reply(pid, op, |env| error_with(env, "closed")),
            Some(Received::Failed(reason)) => reply(pid, op, |env| error_with(env, reason)),
            None => reply(pid, op, |env| error_with(env, "timeout")),
        }
    });
    r
}

fn passive_message(owner: u64, id: u64) {
    deliver(owner, |env| {
        let s = socket_term(env, id);
        env.tuple(&[a("udp_passive"), s])
    });
}

/// Active mode: delivers datagrams to the owner until the socket turns
/// passive. Delivery happens under the state lock so a passive switch or an
/// owner change never races a message to the old owner.
fn start_reader(id: u64, socket: Arc<Udp>) {
    {
        let mut st = socket.state.lock();
        if st.reading || st.closed || st.active == Active::Passive {
            return;
        }
        st.reading = true;
    }
    crate::sched::runtime().spawn(async move {
        loop {
            let _guard = socket.reads.lock().await;
            let got = receive(&socket).await;
            let mut st = socket.state.lock();
            match got {
                Received::Packet(from, data) => {
                    if st.active == Active::Passive {
                        st.pending = Some((from, data));
                        st.reading = false;
                        return;
                    }
                    let mut exhausted = false;
                    st.active = match st.active {
                        Active::Once => Active::Passive,
                        Active::Count(n) if n <= 1 => {
                            exhausted = true;
                            Active::Passive
                        }
                        Active::Count(n) => Active::Count(n - 1),
                        other => other,
                    };
                    let (owner, list) = (st.owner, st.list);
                    deliver(owner, |env| {
                        let s = socket_term(env, id);
                        let ip = ip_term(env, from.ip());
                        let packet = packet_term(env, &data, list);
                        env.tuple(&[a("udp"), s, ip, small(from.port() as i64), packet])
                    });
                    if exhausted {
                        passive_message(owner, id);
                    }
                    if st.active == Active::Passive {
                        st.reading = false;
                        return;
                    }
                }
                Received::Closed => {
                    st.reading = false;
                    return;
                }
                Received::Failed(reason) => {
                    st.reading = false;
                    let owner = st.owner;
                    deliver(owner, |env| {
                        let s = socket_term(env, id);
                        env.tuple(&[a("udp_error"), s, a(reason)])
                    });
                    return;
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Sending
// ---------------------------------------------------------------------------

enum Destination {
    Address(SocketAddr),
    Host(String, u16),
}

fn destination(address: Term, port: u16) -> Option<Destination> {
    if is_tuple(address) {
        return ip_of(address).map(|ip| Destination::Address(SocketAddr::new(ip, port)));
    }
    let host = host_of(address)?;
    Some(match host.parse::<IpAddr>() {
        Ok(ip) => Destination::Address(SocketAddr::new(ip, port)),
        Err(_) => Destination::Host(host, port),
    })
}

/// Resolves a host name in the socket's address family.
async fn resolve(udp: &Udp, host: &str, port: u16) -> Result<SocketAddr, &'static str> {
    let v6 = udp.io.local_addr().map(|l| l.is_ipv6()).unwrap_or(false);
    let found = tokio::net::lookup_host((host, port)).await.map_err(|_| "nxdomain")?;
    found.into_iter().find(|addr| addr.is_ipv6() == v6).ok_or("nxdomain")
}

fn sent(udp: &Udp, n: usize) {
    let mut st = udp.state.lock();
    st.send_oct += n as u64;
    st.send_cnt += 1;
}

/// Sends at once when the address is known and the socket is writable;
/// otherwise returns a reference the caller waits on.
#[no_mangle]
pub extern "C" fn tn_udp_send(c: *mut Ctx, socket: Term, address: Term, port: Term, data: Term) -> Term {
    let ctx = cx(c);
    if !is_small(port) || !(0..=65535).contains(&small_val(port)) {
        return ctx.badarg("argument error");
    }
    let mut bytes = Vec::new();
    if !crate::nif::flatten_iolist(data, &mut bytes) {
        return ctx.badarg("argument error");
    }
    let Some(target) = destination(address, small_val(port) as u16) else {
        return ctx_result(ctx, None, Some("einval"));
    };
    let Some(udp) = lookup(socket) else {
        return ctx_result(ctx, None, Some("closed"));
    };
    if let Destination::Address(addr) = target {
        match udp.io.try_send_to(&bytes, addr) {
            Ok(n) => {
                sent(&udp, n);
                return atom(a::OK);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return ctx_result(ctx, None, Some(posix(&e))),
        }
    }
    let (r, op, pid) = operation(ctx);
    crate::sched::runtime().spawn(async move {
        let addr = match target {
            Destination::Address(addr) => addr,
            Destination::Host(host, port) => match resolve(&udp, &host, port).await {
                Ok(addr) => addr,
                Err(reason) => return reply(pid, op, |env| error_with(env, reason)),
            },
        };
        let result = tokio::select! {
            r = udp.io.send_to(&bytes, addr) => r.map_err(|e| posix(&e)),
            _ = udp.closed.notified() => Err("closed"),
        };
        match result {
            Ok(n) => {
                sent(&udp, n);
                reply(pid, op, |_| atom(a::OK))
            }
            Err(reason) => reply(pid, op, |env| error_with(env, reason)),
        }
    });
    r
}

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

fn close(udp: &Udp) {
    udp.state.lock().closed = true;
    udp.closed.notify_waiters();
}

/// True when `socket` was a UDP socket (now closed).
pub(crate) fn remove(socket: Term) -> bool {
    if !is_ref(socket) {
        return false;
    }
    let removed = SOCKETS.lock().as_mut().and_then(|m| m.remove(&word(socket, 1)));
    removed.map(|udp| close(&udp)).is_some()
}

#[no_mangle]
pub extern "C" fn tn_udp_close(_c: *mut Ctx, socket: Term) -> Term {
    remove(socket);
    atom(a::OK)
}

/// Sockets close with their controlling process, as ports do on the BEAM.
pub fn owner_exited(pid: u64) {
    let owned: Vec<Arc<Udp>> = {
        let mut sockets = SOCKETS.lock();
        let Some(m) = sockets.as_mut() else { return };
        let ids: Vec<u64> = m.iter().filter(|(_, s)| s.state.lock().owner == pid).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| m.remove(&id)).collect()
    };
    owned.iter().for_each(|udp| close(udp));
}

/// Whether the caller may hand `socket` to `owner`: `:ok`, `:same` when
/// `owner` already has it, or an error as OTP gives.
#[no_mangle]
pub extern "C" fn tn_udp_owner_check(c: *mut Ctx, socket: Term, owner: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(owner) {
        return ctx.badarg("argument error");
    }
    let Some(udp) = lookup(socket) else {
        return ctx_result(ctx, None, Some("closed"));
    };
    let current = udp.state.lock().owner;
    if current == pid_id(owner) {
        a("same")
    } else if current == ctx.pid() {
        atom(a::OK)
    } else {
        ctx_result(ctx, None, Some("not_owner"))
    }
}

#[no_mangle]
pub extern "C" fn tn_udp_controlling_process(c: *mut Ctx, socket: Term, owner: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(owner) {
        return ctx.badarg("argument error");
    }
    match lookup(socket) {
        Some(udp) => {
            udp.state.lock().owner = pid_id(owner);
            atom(a::OK)
        }
        None => ctx_result(ctx, None, Some("closed")),
    }
}

/// `:inet.setopts/2` for a UDP socket.
pub(crate) fn setopts(ctx: &mut Ctx, udp: Arc<Udp>, socket: Term, opts: Term) -> Term {
    let id = word(socket, 1);
    let sock = socket2::SockRef::from(&udp.io);
    for item in list_items(opts) {
        let applied = match option(item) {
            Some(("active", v)) => {
                let Some(active) = active_of(v) else {
                    return ctx_result(ctx, None, Some("einval"));
                };
                let mut st = udp.state.lock();
                st.active = match (st.active, active) {
                    (Active::Count(n), Active::Count(m)) => Active::Count(n + m),
                    (_, other) => other,
                };
                if let Active::Count(n) = st.active {
                    if n <= 0 {
                        st.active = Active::Passive;
                        passive_message(st.owner, id);
                    }
                }
                Ok(())
            }
            Some(("binary", _)) => Ok(udp.state.lock().list = false),
            Some(("list", _)) => Ok(udp.state.lock().list = true),
            Some(("mode", v)) => Ok(udp.state.lock().list = v != a("binary")),
            Some(("reuseaddr", v)) => sock.set_reuse_address(v == TRUE),
            Some(("broadcast", v)) => sock.set_broadcast(v == TRUE),
            Some(("recbuf", v)) => match size_option(v) {
                Some(n) => sock.set_recv_buffer_size(n),
                None => return ctx_result(ctx, None, Some("einval")),
            },
            Some(("sndbuf", v)) => match size_option(v) {
                Some(n) => sock.set_send_buffer_size(n),
                None => return ctx_result(ctx, None, Some("einval")),
            },
            _ => Ok(()),
        };
        if let Err(e) = applied {
            return ctx_result(ctx, None, Some(posix(&e)));
        }
    }
    start_reader(id, udp);
    atom(a::OK)
}

/// `:inet.getopts/2` for a UDP socket.
pub(crate) fn getopts(ctx: &mut Ctx, udp: Arc<Udp>, opts: Term) -> Term {
    let sock = socket2::SockRef::from(&udp.io);
    let (active, list) = {
        let st = udp.state.lock();
        (st.active, st.list)
    };
    let mut found: Vec<(&'static str, Term)> = Vec::new();
    for item in list_items(opts) {
        if !is_atom(item) {
            continue;
        }
        let name = crate::atoms::name(atom_idx(item));
        let value = match name {
            "active" => match active {
                Active::Passive => FALSE,
                Active::On => TRUE,
                Active::Once => a("once"),
                Active::Count(n) => small(n),
            },
            "mode" => a(if list { "list" } else { "binary" }),
            "reuseaddr" => boolean(sock.reuse_address().unwrap_or(false)),
            "reuseport" => boolean(sock.reuse_port().unwrap_or(false)),
            "broadcast" => boolean(sock.broadcast().unwrap_or(false)),
            "recbuf" => small(sock.recv_buffer_size().unwrap_or(0) as i64),
            "sndbuf" => small(sock.send_buffer_size().unwrap_or(0) as i64),
            _ => continue,
        };
        found.push((name, value));
    }
    let base = ctx.roots.len();
    for (name, value) in &found {
        let t = ctx.tuple(&[a(name), *value]);
        ctx.roots.push(t);
    }
    let list = ctx.list_from_roots(base, found.len(), NIL_LIST);
    ctx.roots.truncate(base);
    ctx_result(ctx, Some(list), None)
}

/// Local address; UDP sockets here are never connected, so no peer.
pub(crate) fn endpoint(ctx: &mut Ctx, udp: Arc<Udp>, peer: bool) -> Term {
    if peer {
        return ctx_result(ctx, None, Some("enotconn"));
    }
    match udp.io.local_addr() {
        Ok(addr) => {
            let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
            let ip = ip_term(&mut env, addr.ip());
            let t = env.tuple(&[ip, small(addr.port() as i64)]);
            let t = ctx.copy_in_by(t, &|p| env.owns_address(p));
            ctx_result(ctx, Some(t), None)
        }
        Err(e) => ctx_result(ctx, None, Some(posix(&e))),
    }
}

pub(crate) fn stats(udp: &Udp) -> [u64; 4] {
    let st = udp.state.lock();
    [st.recv_oct, st.recv_cnt, st.send_oct, st.send_cnt]
}
