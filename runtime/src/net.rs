//! TCP sockets for `:gen_tcp` and `:inet`.
//!
//! Sockets are references whose ids index a registry. Operations that wait
//! (accept, recv, send, connect) run as tokio tasks and answer the calling
//! process with `{:"$tonic_tcp", ref, result}`, so a waiting process never
//! blocks a scheduler thread. Active mode delivers `{:tcp, socket, data}`,
//! `{:tcp_closed, socket}` and `{:tcp_error, socket, reason}` to the owner.

mod config;
pub mod packet;

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::nif::Env;
use crate::term::*;
use config::Options;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket, TcpStream};

#[derive(Clone, Copy, PartialEq)]
pub enum Active {
    Passive,
    Once,
    On,
    Count(i64),
}

struct State {
    options: Options,
    pending: Option<Vec<u8>>,
    owner: u64,
    active: Active,
    buffer: Vec<u8>,
    reading: bool,
    closed: bool,
    recv_oct: u64,
    recv_cnt: u64,
    send_oct: u64,
    send_cnt: u64,
    keep_open: bool,
    /// closed by the read that found the peer gone (OTP's exit_on_close)
    shut: bool,
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
    SOCKETS
        .lock()
        .get_or_insert_with(HashMap::new)
        .insert(id, socket);
    id
}

fn lookup(t: Term) -> Option<Socket> {
    if !is_ref(t) {
        return None;
    }
    SOCKETS
        .lock()
        .as_ref()
        .and_then(|m| m.get(&word(t, 1)).cloned())
}

pub fn a(name: &str) -> Term {
    atom(crate::atoms::intern(name))
}

pub fn posix(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.raw_os_error() {
        Some(libc::EMSGSIZE) => return "emsgsize",
        Some(libc::ENETUNREACH) => return "enetunreach",
        Some(libc::EHOSTUNREACH) => return "ehostunreach",
        _ => {}
    }
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

pub fn socket_term(env: &mut Env, id: u64) -> Term {
    let p = env.alloc(2);
    unsafe {
        *p = header(T_REF, 1);
        *p.add(1) = id;
    }
    p as Term
}

pub fn deliver(pid: u64, build: impl FnOnce(&mut Env) -> Term) {
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

pub fn ok_with(env: &mut Env, value: Term) -> Term {
    env.tuple(&[atom(a::OK), value])
}

pub fn error_with(env: &mut Env, reason: &str) -> Term {
    env.tuple(&[atom(a::ERROR), a(reason)])
}

pub fn ip_term(env: &mut Env, ip: IpAddr) -> Term {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            env.tuple(&[
                small(o[0] as i64),
                small(o[1] as i64),
                small(o[2] as i64),
                small(o[3] as i64),
            ])
        }
        IpAddr::V6(v6) => {
            let s: Vec<Term> = v6.segments().iter().map(|x| small(*x as i64)).collect();
            env.tuple(&s)
        }
    }
}

fn address_term(env: &mut Env, addr: SocketAddr) -> Term {
    let ip = ip_term(env, addr.ip());
    env.tuple(&[ip, small(addr.port() as i64)])
}

// ---------------------------------------------------------------------------
// Option parsing (terms of the calling process)
// ---------------------------------------------------------------------------

pub fn proper_list(mut list: Term) -> bool {
    while is_cons(list) {
        list = tail(list);
    }
    list == NIL_LIST
}

pub fn list_items(mut l: Term) -> Vec<Term> {
    let mut items = Vec::new();
    while is_cons(l) {
        items.push(head(l));
        l = tail(l);
    }
    items
}

pub fn option(t: Term) -> Option<(&'static str, Term)> {
    if is_atom(t) {
        return Some((crate::atoms::name(atom_idx(t)), TRUE));
    }
    if is_tuple(t) && tuple_size(t) == 2 && is_atom(tuple_get(t, 0)) {
        return Some((
            crate::atoms::name(atom_idx(tuple_get(t, 0))),
            tuple_get(t, 1),
        ));
    }
    None
}

pub fn active_of(v: Term) -> Option<Active> {
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

pub fn ip_of(v: Term) -> Option<IpAddr> {
    if !is_tuple(v) {
        return None;
    }
    let parts: Vec<i64> = (0..tuple_size(v))
        .map(|i| tuple_get(v, i))
        .filter(|x| is_small(*x))
        .map(small_val)
        .collect();
    match parts.len() {
        4 if tuple_size(v) == 4 => Some(IpAddr::V4(Ipv4Addr::new(
            parts[0] as u8,
            parts[1] as u8,
            parts[2] as u8,
            parts[3] as u8,
        ))),
        8 if tuple_size(v) == 8 => {
            let s: Vec<u16> = parts.iter().map(|x| *x as u16).collect();
            Some(IpAddr::V6(Ipv6Addr::new(
                s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
            )))
        }
        _ => None,
    }
}

pub fn timeout_of(t: Term) -> Option<Duration> {
    if is_small(t) && small_val(t) >= 0 {
        Some(Duration::from_millis(small_val(t) as u64))
    } else {
        None
    }
}

pub fn ctx_result(ctx: &mut Ctx, ok: Option<Term>, error: Option<&str>) -> Term {
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
    let mut options = Options::default();
    if config::apply(&mut options, opts).is_none() {
        return ctx_result(ctx, None, Some("einval"));
    }
    let ip = options.ip.unwrap_or(if options.inet6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    });
    let bound = (|| {
        let address = SocketAddr::new(ip, small_val(port) as u16);
        let socket = socket2::Socket::new(
            socket2::Domain::for_address(address),
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        config::transport(&socket, &options, address.is_ipv6())?;
        socket.bind(&address.into())?;
        socket.listen(options.backlog)?;
        socket.set_nonblocking(true)?;
        let _guard = crate::sched::runtime().enter();
        TcpListener::from_std(socket.into())
    })();
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

fn adopt(io: TcpStream, owner: u64, options: &Options) -> Result<u64, &'static str> {
    let socket = socket2::SockRef::from(&io);
    config::transport(&socket, options, io.local_addr().is_ok_and(|a| a.is_ipv6()))
        .map_err(|e| posix(&e))?;
    let stream = Arc::new(Stream {
        io,
        state: parking_lot::Mutex::new(State {
            options: options.clone(),
            pending: None,
            owner,
            active: options.active,
            buffer: Vec::new(),
            reading: false,
            closed: false,
            recv_oct: 0,
            recv_cnt: 0,
            send_oct: 0,
            send_cnt: 0,
            keep_open: options.keep_open,
            shut: false,
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
    Ok(id)
}

/// Starts an operation: returns its reference to the caller, who waits for
/// the reply message.
pub fn operation(ctx: &mut Ctx) -> (Term, u64, u64) {
    let op = crate::sched::next_ref_id();
    (crate::sched::mk_ref_term(ctx, op), op, ctx.pid())
}

pub async fn with_timeout<T>(
    timeout: Option<Duration>,
    f: impl std::future::Future<Output = T>,
) -> Option<T> {
    match timeout {
        Some(d) => tokio::time::timeout(d, f).await.ok(),
        None => Some(f.await),
    }
}

#[no_mangle]
pub extern "C" fn tn_tcp_accept(c: *mut Ctx, socket: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    // Arguments are read before the reference is allocated, which can move them.
    let listener = lookup(socket);
    let timeout = timeout_of(timeout);
    let (r, op, pid) = operation(ctx);
    let listener = match listener {
        Some(Socket::Listener(l)) => l,
        _ => {
            reply(pid, op, |env| error_with(env, "closed"));
            return r;
        }
    };
    crate::sched::runtime().spawn(async move {
        let accepted = tokio::select! {
            r = with_timeout(timeout, listener.io.accept()) => r,
            _ = listener.closed.notified() => Some(Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))),
        };
        match accepted {
            Some(Ok((io, _))) => {
                let id = {
                    let options = listener.options.lock();
                    let options = options.clone();
                    adopt(io, pid, &options)
                };
                reply(pid, op, |env| match id {
                    Ok(id) => { let s = socket_term(env, id); ok_with(env, s) }
                    Err(reason) => error_with(env, reason),
                })
            }
            Some(Err(e)) => reply(pid, op, |env| error_with(env, posix(&e))),
            None => reply(pid, op, |env| error_with(env, "timeout")),
        }
    });
    r
}

/// An address tuple, or a host name as an atom, charlist or binary.
pub fn host_of(address: Term) -> Option<String> {
    if is_tuple(address) {
        ip_of(address).map(|ip| ip.to_string())
    } else if is_atom(address) {
        Some(crate::atoms::name(atom_idx(address)).to_string())
    } else if is_list(address) {
        Some(
            list_items(address)
                .iter()
                .filter(|x| is_small(**x))
                .map(|x| small_val(*x) as u8 as char)
                .collect(),
        )
    } else if is_binary(address) {
        Some(String::from_utf8_lossy(bin_bytes(address)).into_owned())
    } else {
        None
    }
}

#[no_mangle]
pub extern "C" fn tn_tcp_connect(
    c: *mut Ctx,
    address: Term,
    port: Term,
    opts: Term,
    timeout: Term,
) -> Term {
    let ctx = cx(c);
    let target = match (host_of(address), is_small(port)) {
        (Some(h), true) => Some((h, small_val(port) as u16)),
        _ => None,
    };
    let mut options = Options::default();
    let configured = config::apply(&mut options, opts).is_some();
    let timeout = timeout_of(timeout);
    let (r, op, pid) = operation(ctx);
    let Some((host, port)) = target.filter(|_| configured) else {
        reply(pid, op, |env| error_with(env, "einval"));
        return r;
    };
    crate::sched::runtime().spawn(async move {
        let connect = async {
            let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
                vec![SocketAddr::new(ip, port)]
            } else {
                tokio::net::lookup_host((host.as_str(), port))
                    .await
                    .map_err(|_| "nxdomain")?
                    .filter(|address| address.is_ipv6() == options.inet6)
                    .collect()
            };
            let mut last = "nxdomain";
            for address in addresses {
                let socket = if address.is_ipv6() {
                    TcpSocket::new_v6()
                } else {
                    TcpSocket::new_v4()
                }
                .map_err(|e| posix(&e))?;
                config::transport(
                    &socket2::SockRef::from(&socket),
                    &options,
                    address.is_ipv6(),
                )
                .map_err(|e| posix(&e))?;
                if options.ip.is_some() || options.port != 0 {
                    let ip = options.ip.unwrap_or(if address.is_ipv6() {
                        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
                    } else {
                        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
                    });
                    socket
                        .bind(SocketAddr::new(ip, options.port))
                        .map_err(|e| posix(&e))?;
                }
                match socket.connect(address).await {
                    Ok(io) => return Ok(io),
                    Err(error) => last = posix(&error),
                }
            }
            Err(last)
        };
        match with_timeout(timeout, connect).await {
            Some(Ok(io)) => {
                let id = adopt(io, pid, &options);
                reply(pid, op, |env| match id {
                    Ok(id) => {
                        let s = socket_term(env, id);
                        ok_with(env, s)
                    }
                    Err(reason) => error_with(env, reason),
                })
            }
            Some(Err(reason)) => reply(pid, op, |env| error_with(env, reason)),
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
            if let Some(data) = st.pending.take() {
                return Read::Data(data);
            }
            if packet::too_large(st.options.packet, &st.buffer, st.options.packet_size) {
                return Read::Failed("emsgsize");
            }
            if let Some(frame) = packet::frame(st.options.packet, &st.buffer, length) {
                let data = st.buffer[frame.skip..frame.skip + frame.len].to_vec();
                st.buffer.drain(..frame.skip + frame.len);
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
        let mut chunk = vec![0u8; stream.state.lock().options.buffer];
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
    let stream = lookup(socket);
    let length = (is_small(length) && small_val(length) >= 0).then(|| small_val(length) as usize);
    let timeout = timeout_of(timeout);
    let (r, op, pid) = operation(ctx);
    let stream = match stream {
        Some(Socket::Stream(s)) => s,
        _ => {
            reply(pid, op, |env| error_with(env, "closed"));
            return r;
        }
    };
    let Some(length) = length else {
        reply(pid, op, |env| error_with(env, "einval"));
        return r;
    };
    if stream.state.lock().active != Active::Passive {
        reply(pid, op, |env| error_with(env, "einval"));
        return r;
    }
    if stream.state.lock().shut {
        reply(pid, op, |env| error_with(env, "enotconn"));
        return r;
    }
    crate::sched::runtime().spawn(async move {
        match with_timeout(timeout, read(&stream, length)).await {
            Some(Read::Data(data)) => reply(pid, op, |env| {
                let options = &stream.state.lock().options;
                let b = data_term(env, &data, options.binary, options.header);
                ok_with(env, b)
            }),
            Some(Read::Closed) => {
                {
                    let mut st = stream.state.lock();
                    st.shut = !st.keep_open;
                }
                reply(pid, op, |env| error_with(env, "closed"))
            }
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
        if st.reading || st.closed || st.active == Active::Passive {
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
                        st.pending = Some(data);
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
                    let (binary, header) = (st.options.binary, st.options.header);
                    drop(st);
                    deliver(owner, |env| {
                        let s = socket_term(env, id);
                        let b = data_term(env, &data, binary, header);
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

fn data_term(env: &mut Env, data: &[u8], binary: bool, header: usize) -> Term {
    if !binary {
        return data
            .iter()
            .rev()
            .fold(NIL_LIST, |tail, byte| env.cons(small(*byte as i64), tail));
    }
    let count = header.min(data.len());
    let tail = env.bytes(&data[count..]);
    data[..count]
        .iter()
        .rev()
        .fold(tail, |tail, byte| env.cons(small(*byte as i64), tail))
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
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                ) =>
            {
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
        Some(Socket::Stream(s)) if !s.state.lock().shut => s,
        _ => return ctx_result(ctx, None, Some("closed")),
    };
    let mut bytes = Vec::new();
    if !crate::nif::flatten_iolist(data, &mut bytes) {
        return ctx.badarg("argument error");
    }
    let header = match packet::header(stream.state.lock().options.packet, bytes.len()) {
        Ok(header) => header,
        Err(reason) => return ctx_result(ctx, None, Some(reason)),
    };
    bytes.splice(0..0, header);
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
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    return ctx_result(ctx, None, Some("closed"))
                }
                Err(e) => return ctx_result(ctx, None, Some(posix(&e))),
            }
        }
        bytes.drain(..written);
    }
    let socket_id = word(socket, 1);
    let (r, op, pid) = operation(ctx);
    crate::sched::runtime().spawn(async move {
        let _guard = stream.writes.lock().await;
        let (timeout, close_on_timeout) = {
            let st = stream.state.lock();
            (st.options.send_timeout, st.options.send_timeout_close)
        };
        match with_timeout(timeout, write_all(&stream, &bytes)).await {
            Some(Ok(())) => reply(pid, op, |_| atom(a::OK)),
            Some(Err(reason)) => reply(pid, op, |env| error_with(env, reason)),
            None => {
                if close_on_timeout {
                    SOCKETS
                        .lock()
                        .as_mut()
                        .and_then(|sockets| sockets.remove(&socket_id));
                    close(Socket::Stream(stream.clone()));
                }
                reply(pid, op, |env| error_with(env, "timeout"));
            }
        }
    });
    r
}

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn tn_tcp_close(_c: *mut Ctx, socket: Term) -> Term {
    if !is_ref(socket) || crate::udp::remove(socket) {
        return atom(a::OK);
    }
    let removed = SOCKETS
        .lock()
        .as_mut()
        .and_then(|m| m.remove(&word(socket, 1)));
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

/// Hands a connected stream to another layer (`:ssl.connect/3`): the registry
/// forgets it without shutting the connection down.
pub fn detach(socket: Term) -> Option<std::net::TcpStream> {
    use std::os::fd::{AsRawFd, FromRawFd};
    if !is_ref(socket) {
        return None;
    }
    let mut sockets = SOCKETS.lock();
    let m = sockets.as_mut()?;
    let Some(Socket::Stream(s)) = m.get(&word(socket, 1)).cloned() else {
        return None;
    };
    let fd = unsafe { libc::dup(s.io.as_raw_fd()) };
    if fd < 0 {
        return None;
    }
    m.remove(&word(socket, 1));
    drop(sockets);
    {
        // a reader still waiting must not tell the old owner it closed
        let mut st = s.state.lock();
        st.owner = 0;
        st.closed = true;
    }
    s.closed.notify_waiters();
    Some(unsafe { std::net::TcpStream::from_raw_fd(fd) })
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
        let ids: Vec<u64> = m
            .iter()
            .filter(|(_, s)| owner_of(s) == pid)
            .map(|(id, _)| *id)
            .collect();
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
    if let Some(udp) = crate::udp::lookup(socket) {
        return crate::udp::setopts(ctx, udp, socket, opts);
    }
    if !proper_list(opts) {
        return ctx_result(ctx, None, Some("einval"));
    }
    let socket_value = lookup(socket);
    let Some(socket_value) = socket_value else {
        return ctx_result(ctx, None, Some("einval"));
    };
    let items = list_items(opts);
    if items.iter().any(|item| {
        matches!(
            option(*item),
            Some(("ip" | "ifaddr" | "inet" | "inet6" | "port" | "backlog", _))
        )
    }) {
        return ctx_result(ctx, None, Some("einval"));
    }
    match socket_value {
        Socket::Stream(stream) => {
            let mut next = stream.state.lock().options.clone();
            next.settings.clear();
            if config::apply(&mut next, opts).is_none() {
                return ctx_result(ctx, None, Some("einval"));
            }
            if let Err(error) = config::transport(
                &socket2::SockRef::from(&stream.io),
                &next,
                stream.io.local_addr().is_ok_and(|a| a.is_ipv6()),
            ) {
                return ctx_result(ctx, None, Some(posix(&error)));
            }
            {
                let mut st = stream.state.lock();
                if items
                    .iter()
                    .any(|item| matches!(option(*item), Some(("active", _))))
                {
                    st.active = match (st.active, next.active) {
                        (Active::Count(n), Active::Count(m)) => Active::Count(n + m),
                        (_, other) => other,
                    };
                    if matches!(st.active, Active::Count(n) if n <= 0) {
                        st.active = Active::Passive;
                        deliver(st.owner, |env| {
                            let socket = socket_term(env, word(socket, 1));
                            env.tuple(&[a("tcp_passive"), socket])
                        });
                    }
                }
                next.active = st.active;
                st.keep_open = next.keep_open;
                st.options = next;
            }
            start_reader(word(socket, 1), stream);
        }
        Socket::Listener(listener) => {
            let mut next = listener.options.lock().clone();
            next.settings.clear();
            if config::apply(&mut next, opts).is_none() {
                return ctx_result(ctx, None, Some("einval"));
            }
            if let Err(error) = config::transport(
                &socket2::SockRef::from(&listener.io),
                &next,
                listener.io.local_addr().is_ok_and(|a| a.is_ipv6()),
            ) {
                return ctx_result(ctx, None, Some(posix(&error)));
            }
            *listener.options.lock() = next;
        }
    }
    atom(a::OK)
}

#[no_mangle]
pub extern "C" fn tn_inet_getopts(c: *mut Ctx, socket: Term, opts: Term) -> Term {
    let ctx = cx(c);
    if !proper_list(opts) {
        return ctx_result(ctx, None, Some("einval"));
    }
    if let Some(udp) = crate::udp::lookup(socket) {
        return crate::udp::getopts(ctx, udp, opts);
    }
    let Some(socket_value) = lookup(socket) else {
        return ctx_result(ctx, None, Some("einval"));
    };
    let (options, active) = match &socket_value {
        Socket::Stream(stream) => {
            let st = stream.state.lock();
            (st.options.clone(), st.active)
        }
        Socket::Listener(listener) => {
            let options = listener.options.lock().clone();
            let active = options.active;
            (options, active)
        }
    };
    let socket = match &socket_value {
        Socket::Stream(stream) => socket2::SockRef::from(&stream.io),
        Socket::Listener(listener) => socket2::SockRef::from(&listener.io),
    };
    let ipv6 = socket.local_addr().is_ok_and(|a| a.is_ipv6());
    let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
    let mut found = Vec::new();
    for item in list_items(opts) {
        if !is_atom(item) {
            return ctx_result(ctx, None, Some("einval"));
        }
        let name = crate::atoms::name(atom_idx(item));
        let value = match name {
            "active" => match active {
                Active::Passive => FALSE,
                Active::On => TRUE,
                Active::Once => a("once"),
                Active::Count(n) => small(n),
            },
            "mode" => a(if options.binary { "binary" } else { "list" }),
            "packet" => packet::term(options.packet),
            "header" => small(options.header as i64),
            "packet_size" => small(options.packet_size as i64),
            "buffer" => small(options.buffer as i64),
            "send_timeout" => options
                .send_timeout
                .map(|duration| small(duration.as_millis() as i64))
                .unwrap_or(a("infinity")),
            "send_timeout_close" => boolean(options.send_timeout_close),
            "exit_on_close" => boolean(!options.keep_open),
            name => {
                let value = match name {
                    "nodelay" => socket.tcp_nodelay().map(boolean),
                    "keepalive" => socket.keepalive().map(boolean),
                    "reuseaddr" => socket.reuse_address().map(boolean),
                    "sndbuf" => socket.send_buffer_size().map(|n| small(n as i64)),
                    "recbuf" => socket.recv_buffer_size().map(|n| small(n as i64)),
                    _ => crate::udp::options::get(&socket, ipv6, name, &mut env),
                };
                match value {
                    Ok(value) => value,
                    Err(error) => return ctx_result(ctx, None, Some(posix(&error))),
                }
            }
        };
        found.push(env.tuple(&[a(name), value]));
    }
    let list = found
        .iter()
        .rev()
        .fold(NIL_LIST, |tail, pair| env.cons(*pair, tail));
    let list = ctx.copy_in_by(list, &|p| env.owns_address(p));
    ctx_result(ctx, Some(list), None)
}

fn endpoint(c: *mut Ctx, socket: Term, peer: bool) -> Term {
    let ctx = cx(c);
    if let Some(udp) = crate::udp::lookup(socket) {
        return crate::udp::endpoint(ctx, udp, peer);
    }
    let addr = match lookup(socket) {
        Some(Socket::Stream(s)) if s.state.lock().shut => {
            return ctx_result(ctx, None, Some(if peer { "enotconn" } else { "ebadf" }))
        }
        Some(Socket::Stream(s)) => {
            if peer {
                s.io.peer_addr()
            } else {
                s.io.local_addr()
            }
        }
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
    let udp = crate::udp::lookup(socket);
    let stats = match lookup(socket) {
        _ if udp.is_some() => udp.map(|u| crate::udp::stats(&u)).unwrap_or_default(),
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
