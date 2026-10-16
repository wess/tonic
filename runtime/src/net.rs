//! TCP sockets for `:gen_tcp` and `:inet`.
//!
//! Sockets are references whose ids index a registry. Operations that wait
//! (accept, recv, send, connect) run as tokio tasks and answer the calling
//! process with `{:"$tonic_tcp", ref, result}`, so a waiting process never
//! blocks a scheduler thread. Active mode delivers `{:tcp, socket, data}`,
//! `{:tcp_closed, socket}` and `{:tcp_error, socket, reason}` to the owner.

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::nif::Env;
use crate::term::*;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy, PartialEq)]
enum Active {
    Passive,
    Once,
    On,
    Count(i64),
}

struct Options {
    active: Active,
    nodelay: bool,
}

struct State {
    owner: u64,
    active: Active,
    buffer: Vec<u8>,
    reading: bool,
    closed: bool,
    recv_oct: u64,
    recv_cnt: u64,
    send_oct: u64,
    send_cnt: u64,
}

struct Stream {
    io: TcpStream,
    state: parking_lot::Mutex<State>,
    reads: tokio::sync::Mutex<()>,
    writes: tokio::sync::Mutex<()>,
    closed: tokio::sync::Notify,
}

struct Listener {
    io: TcpListener,
    owner: parking_lot::Mutex<u64>,
    options: parking_lot::Mutex<Options>,
    closed: tokio::sync::Notify,
}

#[derive(Clone)]
enum Socket {
    Listener(Arc<Listener>),
    Stream(Arc<Stream>),
}

static SOCKETS: parking_lot::Mutex<Option<HashMap<u64, Socket>>> = parking_lot::const_mutex(None);

fn register(socket: Socket) -> u64 {
    let id = crate::sched::next_ref_id();
    SOCKETS.lock().get_or_insert_with(HashMap::new).insert(id, socket);
    id
}

fn lookup(t: Term) -> Option<Socket> {
    if !is_ref(t) {
        return None;
    }
    SOCKETS.lock().as_ref().and_then(|m| m.get(&word(t, 1)).cloned())
}

fn a(name: &str) -> Term {
    atom(crate::atoms::intern(name))
}

fn posix(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.kind() {
        AddrInUse => "eaddrinuse",
        AddrNotAvailable => "eaddrnotavail",
        PermissionDenied => "eacces",
        ConnectionRefused => "econnrefused",
        ConnectionReset => "econnreset",
        ConnectionAborted => "econnaborted",
        BrokenPipe => "epipe",
        NotConnected => "enotconn",
        TimedOut => "timeout",
        UnexpectedEof => "closed",
        _ => "einval",
    }
}

// ---------------------------------------------------------------------------
// Messages built outside any process heap
// ---------------------------------------------------------------------------

fn socket_term(env: &mut Env, id: u64) -> Term {
    let p = env.alloc(2);
    unsafe {
        *p = header(T_REF, 1);
        *p.add(1) = id;
    }
    p as Term
}

fn deliver(pid: u64, build: impl FnOnce(&mut Env) -> Term) {
    let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
    let message = build(&mut env);
    crate::sched::send_fragment(pid, env.export(message));
}

/// `{:"$tonic_tcp", ref, result}` for the operation `op`.
fn reply(pid: u64, op: u64, result: impl FnOnce(&mut Env) -> Term) {
    deliver(pid, |env| {
        let r = socket_term(env, op);
        let value = result(env);
        env.tuple(&[a("$tonic_tcp"), r, value])
    })
}

fn ok_with(env: &mut Env, value: Term) -> Term {
    env.tuple(&[atom(a::OK), value])
}

fn error_with(env: &mut Env, reason: &str) -> Term {
    env.tuple(&[atom(a::ERROR), a(reason)])
}

fn address_term(env: &mut Env, addr: SocketAddr) -> Term {
    let ip = match addr.ip() {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            env.tuple(&[small(o[0] as i64), small(o[1] as i64), small(o[2] as i64), small(o[3] as i64)])
        }
        IpAddr::V6(v6) => {
            let s: Vec<Term> = v6.segments().iter().map(|x| small(*x as i64)).collect();
            env.tuple(&s)
        }
    };
    env.tuple(&[ip, small(addr.port() as i64)])
}

// ---------------------------------------------------------------------------
// Option parsing (terms of the calling process)
// ---------------------------------------------------------------------------

fn list_items(mut l: Term) -> Vec<Term> {
    let mut items = Vec::new();
    while is_cons(l) {
        items.push(head(l));
        l = tail(l);
    }
    items
}

fn option(t: Term) -> Option<(&'static str, Term)> {
    if is_atom(t) {
        return Some((crate::atoms::name(atom_idx(t)), TRUE));
    }
    if is_tuple(t) && tuple_size(t) == 2 && is_atom(tuple_get(t, 0)) {
        return Some((crate::atoms::name(atom_idx(tuple_get(t, 0))), tuple_get(t, 1)));
    }
    None
}

fn active_of(v: Term) -> Option<Active> {
    if v == TRUE {
        Some(Active::On)
    } else if v == FALSE {
        Some(Active::Passive)
    } else if is_atom(v) && crate::atoms::name(atom_idx(v)) == "once" {
        Some(Active::Once)
    } else if is_small(v) {
        Some(Active::Count(small_val(v)))
    } else {
        None
    }
}

fn ip_of(v: Term) -> Option<IpAddr> {
    if !is_tuple(v) {
        return None;
    }
    let parts: Vec<i64> = (0..tuple_size(v)).map(|i| tuple_get(v, i)).filter(|x| is_small(*x)).map(small_val).collect();
    match parts.len() {
        4 if tuple_size(v) == 4 => Some(IpAddr::V4(Ipv4Addr::new(parts[0] as u8, parts[1] as u8, parts[2] as u8, parts[3] as u8))),
        8 if tuple_size(v) == 8 => {
            let s: Vec<u16> = parts.iter().map(|x| *x as u16).collect();
            Some(IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7])))
        }
        _ => None,
    }
}

fn timeout_of(t: Term) -> Option<Duration> {
    if is_small(t) && small_val(t) >= 0 {
        Some(Duration::from_millis(small_val(t) as u64))
    } else {
        None
    }
}

fn ctx_result(ctx: &mut Ctx, ok: Option<Term>, error: Option<&str>) -> Term {
    match (ok, error) {
        (_, Some(reason)) => {
            let r = a(reason);
            ctx.tuple(&[atom(a::ERROR), r])
        }
        (Some(v), None) => ctx.tuple(&[atom(a::OK), v]),
        (None, None) => atom(a::OK),
    }
}

// ---------------------------------------------------------------------------
// Listening, accepting, connecting
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn tn_tcp_listen(c: *mut Ctx, port: Term, opts: Term) -> Term {
    let ctx = cx(c);
    if !is_small(port) || !(0..=65535).contains(&small_val(port)) {
        return ctx.badarg("argument error");
    }
    let mut ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let mut options = Options { active: Active::On, nodelay: false };
    for item in list_items(opts) {
        match option(item) {
            Some(("ip" | "ifaddr", v)) => match ip_of(v) {
                Some(addr) => ip = addr,
                None => return ctx_result(ctx, None, Some("einval")),
            },
            Some(("inet6", _)) => ip = IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            Some(("active", v)) => options.active = active_of(v).unwrap_or(Active::On),
            Some(("nodelay", v)) => options.nodelay = v == TRUE,
            _ => {}
        }
    }
    let bound = std::net::TcpListener::bind(SocketAddr::new(ip, small_val(port) as u16)).and_then(|l| {
        l.set_nonblocking(true)?;
        let _guard = crate::sched::runtime().enter();
        TcpListener::from_std(l)
    });
    match bound {
        Ok(io) => {
            let id = register(Socket::Listener(Arc::new(Listener {
                io,
                owner: parking_lot::Mutex::new(ctx.pid()),
                options: parking_lot::Mutex::new(options),
                closed: tokio::sync::Notify::new(),
            })));
            let t = crate::sched::mk_ref_term(ctx, id);
            ctx_result(ctx, Some(t), None)
        }
        Err(e) => ctx_result(ctx, None, Some(posix(&e))),
    }
}

fn adopt(io: TcpStream, owner: u64, options: &Options) -> u64 {
    let _ = io.set_nodelay(options.nodelay);
    let stream = Arc::new(Stream {
        io,
        state: parking_lot::Mutex::new(State {
            owner,
            active: options.active,
            buffer: Vec::new(),
            reading: false,
            closed: false,
            recv_oct: 0,
            recv_cnt: 0,
            send_oct: 0,
            send_cnt: 0,
        }),
        reads: tokio::sync::Mutex::new(()),
        writes: tokio::sync::Mutex::new(()),
        closed: tokio::sync::Notify::new(),
    });
    let active = options.active;
    let id = register(Socket::Stream(stream.clone()));
    if active != Active::Passive {
        start_reader(id, stream);
    }
    id
}

/// Starts an operation: returns its reference to the caller, who waits for
/// the reply message.
fn operation(ctx: &mut Ctx) -> (Term, u64, u64) {
    let op = crate::sched::next_ref_id();
    (crate::sched::mk_ref_term(ctx, op), op, ctx.pid())
}

async fn with_timeout<T>(timeout: Option<Duration>, f: impl std::future::Future<Output = T>) -> Option<T> {
    match timeout {
        Some(d) => tokio::time::timeout(d, f).await.ok(),
        None => Some(f.await),
    }
}

#[no_mangle]
pub extern "C" fn tn_tcp_accept(c: *mut Ctx, socket: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let (r, op, pid) = operation(ctx);
    let listener = match lookup(socket) {
        Some(Socket::Listener(l)) => l,
        _ => {
            reply(pid, op, |env| error_with(env, "closed"));
            return r;
        }
    };
    let timeout = timeout_of(timeout);
    crate::sched::runtime().spawn(async move {
        let accepted = tokio::select! {
            r = with_timeout(timeout, listener.io.accept()) => r,
            _ = listener.closed.notified() => Some(Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))),
        };
        match accepted {
            Some(Ok((io, _))) => {
                let id = {
                    let options = listener.options.lock();
                    let options = Options { active: options.active, nodelay: options.nodelay };
                    adopt(io, pid, &options)
                };
                reply(pid, op, |env| {
                    let s = socket_term(env, id);
                    ok_with(env, s)
                })
            }
            Some(Err(e)) => reply(pid, op, |env| error_with(env, posix(&e))),
            None => reply(pid, op, |env| error_with(env, "timeout")),
        }
    });
    r
}

#[no_mangle]
pub extern "C" fn tn_tcp_connect(c: *mut Ctx, address: Term, port: Term, opts: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let (r, op, pid) = operation(ctx);
    let host = if is_tuple(address) {
        ip_of(address).map(|ip| ip.to_string())
    } else if is_atom(address) {
        Some(crate::atoms::name(atom_idx(address)).to_string())
    } else if is_list(address) {
        Some(list_items(address).iter().filter(|x| is_small(**x)).map(|x| small_val(*x) as u8 as char).collect())
    } else if is_binary(address) {
        Some(String::from_utf8_lossy(bin_bytes(address)).into_owned())
    } else {
        None
    };
    let (host, port) = match (host, is_small(port)) {
        (Some(h), true) => (h, small_val(port) as u16),
        _ => {
            reply(pid, op, |env| error_with(env, "einval"));
            return r;
        }
    };
    let mut options = Options { active: Active::On, nodelay: false };
    for item in list_items(opts) {
        match option(item) {
            Some(("active", v)) => options.active = active_of(v).unwrap_or(Active::On),
            Some(("nodelay", v)) => options.nodelay = v == TRUE,
            _ => {}
        }
    }
    let timeout = timeout_of(timeout);
    crate::sched::runtime().spawn(async move {
        match with_timeout(timeout, TcpStream::connect((host.as_str(), port))).await {
            Some(Ok(io)) => {
                let id = adopt(io, pid, &options);
                reply(pid, op, |env| {
                    let s = socket_term(env, id);
                    ok_with(env, s)
                })
            }
            Some(Err(e)) => reply(pid, op, |env| error_with(env, posix(&e))),
            None => reply(pid, op, |env| error_with(env, "timeout")),
        }
    });
    r
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

enum Read {
    Data(Vec<u8>),
    Closed,
    Failed(&'static str),
}

/// Reads at least one byte (or exactly `length` bytes) into the buffer and
/// takes what was asked for. Data read before a timeout stays buffered.
async fn read(stream: &Stream, length: usize) -> Read {
    let _guard = stream.reads.lock().await;
    loop {
        {
            let mut st = stream.state.lock();
            let have = st.buffer.len();
            if (length == 0 && have > 0) || (length > 0 && have >= length) {
                let n = if length == 0 { have } else { length };
                let data: Vec<u8> = st.buffer.drain(..n).collect();
                return Read::Data(data);
            }
            if st.closed {
                return Read::Closed;
            }
        }
        let ready = tokio::select! {
            r = stream.io.readable() => r,
            _ = stream.closed.notified() => return Read::Closed,
        };
        if let Err(e) = ready {
            return Read::Failed(posix(&e));
        }
        let mut chunk = vec![0u8; 65536];
        match stream.io.try_read(&mut chunk) {
            Ok(0) => {
                let mut st = stream.state.lock();
                st.closed = true;
                if length > 0 || st.buffer.is_empty() {
                    return Read::Closed;
                }
            }
            Ok(n) => {
                let mut st = stream.state.lock();
                st.buffer.extend_from_slice(&chunk[..n]);
                st.recv_oct += n as u64;
                st.recv_cnt += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Read::Failed(posix(&e)),
        }
    }
}

#[no_mangle]
pub extern "C" fn tn_tcp_recv(c: *mut Ctx, socket: Term, length: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let (r, op, pid) = operation(ctx);
    let stream = match lookup(socket) {
        Some(Socket::Stream(s)) => s,
        _ => {
            reply(pid, op, |env| error_with(env, "closed"));
            return r;
        }
    };
    if !is_small(length) || small_val(length) < 0 {
        reply(pid, op, |env| error_with(env, "einval"));
        return r;
    }
    let length = small_val(length) as usize;
    let timeout = timeout_of(timeout);
    crate::sched::runtime().spawn(async move {
        match with_timeout(timeout, read(&stream, length)).await {
            Some(Read::Data(data)) => reply(pid, op, |env| {
                let b = env.bytes(&data);
                ok_with(env, b)
            }),
            Some(Read::Closed) => reply(pid, op, |env| error_with(env, "closed")),
            Some(Read::Failed(reason)) => reply(pid, op, |env| error_with(env, reason)),
            None => reply(pid, op, |env| error_with(env, "timeout")),
        }
    });
    r
}

/// Active mode: delivers data to the owner until the socket turns passive.
fn start_reader(id: u64, stream: Arc<Stream>) {
    {
        let mut st = stream.state.lock();
        if st.reading || st.active == Active::Passive {
            return;
        }
        st.reading = true;
    }
    crate::sched::runtime().spawn(async move {
        loop {
            let result = read(&stream, 0).await;
            let mut st = stream.state.lock();
            let owner = st.owner;
            match result {
                Read::Data(data) => {
                    if st.active == Active::Passive {
                        // Turned passive while waiting: keep the data for recv.
                        let rest = std::mem::take(&mut st.buffer);
                        st.buffer = data;
                        st.buffer.extend(rest);
                        st.reading = false;
                        return;
                    }
                    let mut passive = false;
                    st.active = match st.active {
                        Active::Once => Active::Passive,
                        Active::Count(n) if n <= 1 => {
                            passive = true;
                            Active::Passive
                        }
                        Active::Count(n) => Active::Count(n - 1),
                        other => other,
                    };
                    let done = st.active == Active::Passive;
                    if done {
                        st.reading = false;
                    }
                    drop(st);
                    deliver(owner, |env| {
                        let s = socket_term(env, id);
                        let b = env.bytes(&data);
                        env.tuple(&[a("tcp"), s, b])
                    });
                    if passive {
                        deliver(owner, |env| {
                            let s = socket_term(env, id);
                            env.tuple(&[a("tcp_passive"), s])
                        });
                    }
                    if done {
                        return;
                    }
                }
                Read::Closed => {
                    st.reading = false;
                    drop(st);
                    deliver(owner, |env| {
                        let s = socket_term(env, id);
                        env.tuple(&[a("tcp_closed"), s])
                    });
                    return;
                }
                Read::Failed(reason) => {
                    st.reading = false;
                    drop(st);
                    deliver(owner, |env| {
                        let s = socket_term(env, id);
                        env.tuple(&[a("tcp_error"), s, a(reason)])
                    });
                    return;
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

async fn write_all(stream: &Stream, data: &[u8]) -> Result<(), &'static str> {
    let mut written = 0;
    while written < data.len() {
        let ready = tokio::select! {
            r = stream.io.writable() => r,
            _ = stream.closed.notified() => return Err("closed"),
        };
        if let Err(e) = ready {
            return Err(posix(&e));
        }
        match stream.io.try_write(&data[written..]) {
            Ok(n) => written += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset) => {
                return Err("closed")
            }
            Err(e) => return Err(posix(&e)),
        }
    }
    let mut st = stream.state.lock();
    st.send_oct += data.len() as u64;
    st.send_cnt += 1;
    Ok(())
}

/// Sends what can be written at once; the rest finishes in a task, and the
/// caller waits for its reply.
#[no_mangle]
pub extern "C" fn tn_tcp_send(c: *mut Ctx, socket: Term, data: Term) -> Term {
    let ctx = cx(c);
    let stream = match lookup(socket) {
        Some(Socket::Stream(s)) => s,
        _ => return ctx_result(ctx, None, Some("closed")),
    };
    let mut bytes = Vec::new();
    if !crate::nif::flatten_iolist(data, &mut bytes) {
        return ctx.badarg("argument error");
    }
    if let Ok(_guard) = stream.writes.try_lock() {
        let mut written = 0;
        loop {
            match stream.io.try_write(&bytes[written..]) {
                Ok(n) => {
                    written += n;
                    if written == bytes.len() {
                        let mut st = stream.state.lock();
                        st.send_oct += bytes.len() as u64;
                        st.send_cnt += 1;
                        return atom(a::OK);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset) => {
                    return ctx_result(ctx, None, Some("closed"))
                }
                Err(e) => return ctx_result(ctx, None, Some(posix(&e))),
            }
        }
        bytes.drain(..written);
    }
    let (r, op, pid) = operation(ctx);
    crate::sched::runtime().spawn(async move {
        let _guard = stream.writes.lock().await;
        match write_all(&stream, &bytes).await {
            Ok(()) => reply(pid, op, |_| atom(a::OK)),
            Err(reason) => reply(pid, op, |env| error_with(env, reason)),
        }
    });
    r
}

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn tn_tcp_close(_c: *mut Ctx, socket: Term) -> Term {
    if !is_ref(socket) {
        return atom(a::OK);
    }
    let removed = SOCKETS.lock().as_mut().and_then(|m| m.remove(&word(socket, 1)));
    if let Some(s) = removed {
        close(s);
    }
    atom(a::OK)
}

fn close(socket: Socket) {
    match socket {
        Socket::Stream(s) => {
            s.state.lock().closed = true;
            s.closed.notify_waiters();
            shutdown_fd(&s.io, 2);
        }
        Socket::Listener(l) => l.closed.notify_waiters(),
    }
}

fn owner_of(socket: &Socket) -> u64 {
    match socket {
        Socket::Stream(s) => s.state.lock().owner,
        Socket::Listener(l) => *l.owner.lock(),
    }
}

/// Sockets close with their controlling process, as ports do on the BEAM.
pub fn owner_exited(pid: u64) {
    let owned: Vec<Socket> = {
        let mut sockets = SOCKETS.lock();
        let Some(m) = sockets.as_mut() else { return };
        let ids: Vec<u64> = m.iter().filter(|(_, s)| owner_of(s) == pid).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| m.remove(&id)).collect()
    };
    owned.into_iter().for_each(close);
}

fn shutdown_fd(io: &TcpStream, how: i32) -> bool {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn shutdown(fd: i32, how: i32) -> i32;
    }
    unsafe { shutdown(io.as_raw_fd(), how) == 0 }
}

#[no_mangle]
pub extern "C" fn tn_tcp_shutdown(c: *mut Ctx, socket: Term, how: Term) -> Term {
    let ctx = cx(c);
    let how = match is_atom(how).then(|| crate::atoms::name(atom_idx(how))) {
        Some("read") => 0,
        Some("write") => 1,
        Some("read_write") => 2,
        _ => return ctx.badarg("argument error"),
    };
    match lookup(socket) {
        Some(Socket::Stream(s)) => {
            if shutdown_fd(&s.io, how) {
                atom(a::OK)
            } else {
                ctx_result(ctx, None, Some("enotconn"))
            }
        }
        _ => ctx_result(ctx, None, Some("closed")),
    }
}

#[no_mangle]
pub extern "C" fn tn_tcp_controlling_process(c: *mut Ctx, socket: Term, owner: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(owner) {
        return ctx.badarg("argument error");
    }
    match lookup(socket) {
        Some(Socket::Stream(s)) => {
            s.state.lock().owner = pid_id(owner);
            atom(a::OK)
        }
        Some(Socket::Listener(l)) => {
            *l.owner.lock() = pid_id(owner);
            atom(a::OK)
        }
        None => ctx_result(ctx, None, Some("closed")),
    }
}

#[no_mangle]
pub extern "C" fn tn_inet_setopts(c: *mut Ctx, socket: Term, opts: Term) -> Term {
    let ctx = cx(c);
    let socket_value = lookup(socket);
    let Some(socket_value) = socket_value else {
        return ctx_result(ctx, None, Some("einval"));
    };
    for item in list_items(opts) {
        match (option(item), &socket_value) {
            (Some(("active", v)), Socket::Stream(s)) => {
                let Some(active) = active_of(v) else {
                    return ctx_result(ctx, None, Some("einval"));
                };
                {
                    let mut st = s.state.lock();
                    st.active = match (st.active, active) {
                        (Active::Count(n), Active::Count(m)) => Active::Count(n + m),
                        (_, other) => other,
                    };
                }
                start_reader(word(socket, 1), s.clone());
            }
            (Some(("active", v)), Socket::Listener(l)) => {
                if let Some(active) = active_of(v) {
                    l.options.lock().active = active;
                }
            }
            (Some(("nodelay", v)), Socket::Stream(s)) => {
                let _ = s.io.set_nodelay(v == TRUE);
            }
            (Some(("nodelay", v)), Socket::Listener(l)) => l.options.lock().nodelay = v == TRUE,
            (Some(("packet", v)), _) if v != small(0) && !(is_atom(v) && crate::atoms::name(atom_idx(v)) == "raw") => {
                return ctx_result(ctx, None, Some("einval"))
            }
            _ => {}
        }
    }
    atom(a::OK)
}

#[no_mangle]
pub extern "C" fn tn_inet_getopts(c: *mut Ctx, socket: Term, opts: Term) -> Term {
    let ctx = cx(c);
    let Some(socket_value) = lookup(socket) else {
        return ctx_result(ctx, None, Some("einval"));
    };
    let mut found: Vec<(&'static str, Term)> = Vec::new();
    for item in list_items(opts) {
        if !is_atom(item) {
            continue;
        }
        let name = crate::atoms::name(atom_idx(item));
        let active = match &socket_value {
            Socket::Stream(s) => s.state.lock().active,
            Socket::Listener(l) => l.options.lock().active,
        };
        let value = match name {
            "active" => match active {
                Active::Passive => FALSE,
                Active::On => TRUE,
                Active::Once => a("once"),
                Active::Count(n) => small(n),
            },
            "nodelay" => match &socket_value {
                Socket::Stream(s) => boolean(s.io.nodelay().unwrap_or(false)),
                Socket::Listener(l) => boolean(l.options.lock().nodelay),
            },
            "mode" => a("binary"),
            "packet" => small(0),
            "sndbuf" | "recbuf" | "buffer" => small(65536),
            "keepalive" | "reuseaddr" => FALSE,
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

fn endpoint(c: *mut Ctx, socket: Term, peer: bool) -> Term {
    let ctx = cx(c);
    let addr = match lookup(socket) {
        Some(Socket::Stream(s)) => if peer { s.io.peer_addr() } else { s.io.local_addr() },
        Some(Socket::Listener(l)) if !peer => l.io.local_addr(),
        Some(Socket::Listener(_)) => return ctx_result(ctx, None, Some("enotconn")),
        None => return ctx_result(ctx, None, Some("einval")),
    };
    match addr {
        Ok(addr) => {
            let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
            let t = address_term(&mut env, addr);
            let t = ctx.copy_in_by(t, &|p| env.owns_address(p));
            ctx_result(ctx, Some(t), None)
        }
        Err(e) => ctx_result(ctx, None, Some(posix(&e))),
    }
}

#[no_mangle]
pub extern "C" fn tn_inet_sockname(c: *mut Ctx, socket: Term) -> Term {
    endpoint(c, socket, false)
}

#[no_mangle]
pub extern "C" fn tn_inet_peername(c: *mut Ctx, socket: Term) -> Term {
    endpoint(c, socket, true)
}

#[no_mangle]
pub extern "C" fn tn_inet_getstat(c: *mut Ctx, socket: Term) -> Term {
    let ctx = cx(c);
    let stats = match lookup(socket) {
        Some(Socket::Stream(s)) => {
            let st = s.state.lock();
            [st.recv_oct, st.recv_cnt, st.send_oct, st.send_cnt]
        }
        Some(Socket::Listener(_)) => [0, 0, 0, 0],
        None => return ctx_result(ctx, None, Some("einval")),
    };
    let names = ["recv_oct", "recv_cnt", "send_oct", "send_cnt"];
    let base = ctx.roots.len();
    for (name, value) in names.iter().zip(stats) {
        let t = ctx.tuple(&[a(name), small(value as i64)]);
        ctx.roots.push(t);
    }
    let list = ctx.list_from_roots(base, names.len(), NIL_LIST);
    ctx.roots.truncate(base);
    ctx_result(ctx, Some(list), None)
}

