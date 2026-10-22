//! Connecting, upgrading gen_tcp sockets, listening and server handshakes.

use crate::alert::{self, Failure};
use crate::options::{apply, Opts};
use crate::{config, io, lookup, register, reply, sock_term, Listener, Pending, Socket, Tls};
use rustls::pki_types::ServerName;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::{TcpSocket, TcpStream};
use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::net::{ctx_result, host_of, ok_with, operation, posix, timeout_of, with_timeout};
use tonic_core::term::*;

fn options(list: Term, base: Opts) -> Result<Opts, Failure> {
    let mut opts = base;
    apply(&mut opts, list).map_err(|name| Failure::Options(name, String::new()))?;
    Ok(opts)
}

fn server_name(host: &str) -> Result<ServerName<'static>, Failure> {
    match host.parse::<IpAddr>() {
        Ok(ip) => Ok(ServerName::IpAddress(ip.into())),
        Err(_) => ServerName::try_from(host.to_string())
            .map_err(|_| Failure::Options("server_name_indication", host.to_string())),
    }
}

fn answer(pid: u64, op: u64, result: Result<u64, Failure>) {
    match result {
        Ok(id) => reply(pid, op, |env| {
            let s = sock_term(env, id);
            ok_with(env, s)
        }),
        Err(f) => reply(pid, op, |env| alert::error(env, &f)),
    }
}

/// Finishes `operation` with an error right away.
fn refuse(ctx: &mut Ctx, failure: Failure) -> Term {
    let (r, op, pid) = operation(ctx);
    answer(pid, op, Err(failure));
    r
}

async fn handshake_client(
    config: Arc<rustls::ClientConfig>,
    name: ServerName<'static>,
    tcp: TcpStream,
) -> Result<Tls, Failure> {
    tokio_rustls::TlsConnector::from(config)
        .connect(name, tcp)
        .await
        .map(Tls::Client)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                Failure::Posix("closed")
            } else {
                alert::from_io(&e, false)
            }
        })
}

async fn resolve(host: &str, port: u16, inet6: bool) -> Result<Vec<SocketAddr>, Failure> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let found = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| Failure::Posix("nxdomain"))?;
    // gen_tcp connects over IPv4 unless asked for :inet6
    let addrs: Vec<SocketAddr> = found.filter(|a| a.is_ipv6() == inet6).collect();
    if addrs.is_empty() {
        return Err(Failure::Posix("nxdomain"));
    }
    Ok(addrs)
}

#[no_mangle]
pub extern "C" fn tn_ssl_connect(
    c: *mut Ctx,
    address: Term,
    port: Term,
    opts: Term,
    timeout: Term,
) -> Term {
    let ctx = cx(c);
    let host = host_of(address);
    let port =
        (is_small(port) && (0..=65535).contains(&small_val(port))).then(|| small_val(port) as u16);
    let parsed = options(opts, Opts::default());
    let timeout = timeout_of(timeout);
    let (Some(host), Some(port)) = (host, port) else {
        return refuse(ctx, Failure::Posix("einval"));
    };
    let mut opts = match parsed {
        Ok(o) => o,
        Err(f) => return refuse(ctx, f),
    };
    let name = server_name(opts.sni.as_deref().unwrap_or(&host));
    let (r, op, pid) = operation(ctx);
    opts.callback_owner = pid;
    opts.callback_operation = op;
    opts.callback_deadline = timeout.map(|duration| std::time::Instant::now() + duration);
    let config = config::client(&opts);
    let (config, name) = match (config, name) {
        (Ok(c), Ok(n)) => (c, n),
        (Err(f), _) | (_, Err(f)) => {
            answer(pid, op, Err(f));
            return r;
        }
    };
    tonic_core::sched::runtime().spawn(async move {
        let attempt = async {
            let addrs = resolve(&host, port, opts.inet6).await?;
            let mut connected = None;
            let mut last = Failure::Posix("econnrefused");
            for address in addrs {
                let socket = if address.is_ipv6() {
                    TcpSocket::new_v6()
                } else {
                    TcpSocket::new_v4()
                }
                .map_err(|e| Failure::Posix(posix(&e)))?;
                if opts.ip.is_some() || opts.local_port != 0 {
                    let ip = opts.ip.unwrap_or(if address.is_ipv6() {
                        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
                    } else {
                        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
                    });
                    socket
                        .bind(SocketAddr::new(ip, opts.local_port))
                        .map_err(|e| Failure::Posix(posix(&e)))?;
                }
                match socket.connect(address).await {
                    Ok(tcp) => {
                        connected = Some(tcp);
                        break;
                    }
                    Err(e) => last = Failure::Posix(posix(&e)),
                }
            }
            let tcp = connected.ok_or(last)?;
            crate::tcp::apply(&tcp, &opts).map_err(|e| Failure::Posix(posix(&e)))?;
            handshake_client(config, name, tcp).await
        };
        let result = match with_timeout(timeout, attempt).await {
            Some(Ok(tls)) => {
                let id = tonic_core::sched::next_ref_id();
                io::adopt(id, tls, pid, &opts);
                Ok(id)
            }
            Some(Err(_))
                if opts
                    .callback_timed_out
                    .load(std::sync::atomic::Ordering::Acquire) =>
            {
                Err(Failure::Posix("timeout"))
            }
            Some(Err(f)) => Err(f),
            None => Err(Failure::Posix("timeout")),
        };
        answer(pid, op, result);
    });
    r
}

/// `:ssl.connect/3` on a connected gen_tcp socket.
#[no_mangle]
pub extern "C" fn tn_ssl_upgrade(c: *mut Ctx, socket: Term, opts: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let parsed = options(opts, Opts::default());
    let timeout = timeout_of(timeout);
    let mut opts = match parsed {
        Ok(o) => o,
        Err(f) => return refuse(ctx, f),
    };
    let Some(tcp) = tonic_core::net::detach(socket) else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let peer = tcp
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    let name = server_name(opts.sni.as_deref().unwrap_or(&peer));
    let (r, op, pid) = operation(ctx);
    opts.callback_owner = pid;
    opts.callback_operation = op;
    opts.callback_deadline = timeout.map(|duration| std::time::Instant::now() + duration);
    let config = config::client(&opts);
    let (config, name) = match (config, name) {
        (Ok(c), Ok(n)) => (c, n),
        (Err(f), _) | (_, Err(f)) => {
            answer(pid, op, Err(f));
            return r;
        }
    };
    tonic_core::sched::runtime().spawn(async move {
        let tcp = match tcp
            .set_nonblocking(true)
            .and_then(|()| TcpStream::from_std(tcp))
        {
            Ok(t) => t,
            Err(e) => return answer(pid, op, Err(Failure::Posix(posix(&e)))),
        };
        if let Err(e) = crate::tcp::apply(&tcp, &opts) {
            return answer(pid, op, Err(Failure::Posix(posix(&e))));
        }
        let result = match with_timeout(timeout, handshake_client(config, name, tcp)).await {
            Some(Ok(tls)) => {
                let id = tonic_core::sched::next_ref_id();
                io::adopt(id, tls, pid, &opts);
                Ok(id)
            }
            Some(Err(_))
                if opts
                    .callback_timed_out
                    .load(std::sync::atomic::Ordering::Acquire) =>
            {
                Err(Failure::Posix("timeout"))
            }
            Some(Err(f)) => Err(f),
            None => Err(Failure::Posix("timeout")),
        };
        answer(pid, op, result);
    });
    r
}

#[no_mangle]
pub extern "C" fn tn_ssl_listen(c: *mut Ctx, port: Term, opts: Term) -> Term {
    let ctx = cx(c);
    let port =
        (is_small(port) && (0..=65535).contains(&small_val(port))).then(|| small_val(port) as u16);
    let parsed = options(opts, Opts::default());
    let Some(port) = port else {
        return ctx.badarg("argument error");
    };
    let fail = |ctx: &mut Ctx, f: Failure| crate::build(ctx, |env| alert::error(env, &f));
    let opts = match parsed {
        Ok(o) => o,
        Err(f) => return fail(ctx, f),
    };
    let unspecified = if opts.inet6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };
    let ip = opts.ip.unwrap_or(unspecified);
    let bound = (|| {
        let address = SocketAddr::new(ip, port);
        let socket = socket2::Socket::new(
            socket2::Domain::for_address(address),
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        socket.set_reuse_address(opts.reuseaddr)?;
        socket.bind(&address.into())?;
        socket.listen(opts.backlog)?;
        socket.set_nonblocking(true)?;
        let _guard = tonic_core::sched::runtime().enter();
        tokio::net::TcpListener::from_std(socket.into())
    })();
    match bound {
        Ok(io) => {
            let id = tonic_core::sched::next_ref_id();
            register(
                id,
                Socket::Listener(Arc::new(Listener {
                    io,
                    owner: parking_lot::Mutex::new(ctx.pid()),
                    opts: parking_lot::Mutex::new(opts),
                    closed: tokio::sync::watch::channel(false).0,
                })),
            );
            crate::build(ctx, |env| {
                let s = sock_term(env, id);
                ok_with(env, s)
            })
        }
        Err(e) => ctx_result(ctx, None, Some(posix(&e))),
    }
}

/// `:ssl.transport_accept/2`: a TCP connection, not yet handshaken.
#[no_mangle]
pub extern "C" fn tn_ssl_accept(c: *mut Ctx, socket: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let listener = lookup(socket);
    let timeout = timeout_of(timeout);
    let Some(Socket::Listener(listener)) = listener else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let (r, op, pid) = operation(ctx);
    tonic_core::sched::runtime().spawn(async move {
        let mut closed = listener.closed.subscribe();
        let accepted = tokio::select! {
            r = with_timeout(timeout, listener.io.accept()) => r,
            _ = closed.wait_for(|c| *c) => Some(Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))),
        };
        let result = match accepted {
            Some(Ok((tcp, _))) => {
                let id = tonic_core::sched::next_ref_id();
                let mut opts = listener.opts.lock().clone();
                opts.callback_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
                opts.callback_timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
                register(
                    id,
                    Socket::Pending(Arc::new(Pending {
                        closed: tokio::sync::watch::channel(false).0,
                        io: parking_lot::Mutex::new(Some(tcp)),
                        owner: parking_lot::Mutex::new(pid),
                        opts,
                        listener: listener.clone(),
                    })),
                );
                Ok(id)
            }
            Some(Err(e)) => Err(Failure::Posix(posix(&e))),
            None => Err(Failure::Posix("timeout")),
        };
        answer(pid, op, result);
    });
    r
}

/// `:ssl.handshake/2,3` on a transport-accepted socket; `opts` may add to
/// the listener's options.
#[no_mangle]
pub extern "C" fn tn_ssl_handshake(c: *mut Ctx, socket: Term, opts: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let id = crate::id_of(socket);
    let pending = lookup(socket);
    let timeout = timeout_of(timeout);
    let (Some(id), Some(Socket::Pending(pending))) = (id, pending) else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let parsed = options(opts, pending.opts.clone());
    let mut opts = match parsed {
        Ok(o) => o,
        Err(f) => return refuse(ctx, f),
    };
    let Some(tcp) = pending.io.lock().take() else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let (r, op, pid) = operation(ctx);
    opts.callback_owner = pid;
    opts.callback_operation = op;
    opts.callback_deadline = timeout.map(|duration| std::time::Instant::now() + duration);
    tonic_core::sched::runtime().spawn(async move {
        let accept = async {
            crate::tcp::apply(&tcp, &opts)?;
            let start = tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), tcp).await?;
            let mut selected = opts.clone();
            if opts.sni_callback {
                let name = start.client_hello().server_name().unwrap_or("").to_string();
                match crate::callback::call(&opts, |env| {
                    let hostname = env.bytes(name.as_bytes());
                    env.tuple(&[tonic_core::net::a("sni"), hostname])
                }) {
                    Some(crate::callback::Answer::Options(extra)) => selected = extra,
                    _ => return Err(std::io::Error::other("sni callback failed")),
                }
            }
            let config = config::server(&selected).map_err(|_| std::io::Error::other("invalid SNI options"))?;
            start.into_stream(config).await
        };
        let mut closed = pending.closed.subscribe();
        let accepted = tokio::select! {
            result = with_timeout(timeout, accept) => result,
            _ = closed.wait_for(|closed| *closed) => Some(Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))),
        };
        let result = match accepted {
            Some(Ok(tls)) => {
                let still_open = matches!(crate::remove(id), Some(Socket::Pending(_)));
                if still_open {
                    let owner = *pending.owner.lock();
                    io::adopt(id, Tls::Server(tls), owner, &opts);
                    Ok(id)
                } else {
                    Err(Failure::Posix("closed"))
                }
            }
            Some(Err(e)) => {
                crate::remove(id);
                Err(if opts.callback_timed_out.load(std::sync::atomic::Ordering::Acquire) {
                    Failure::Posix("timeout")
                } else if e.kind() == std::io::ErrorKind::UnexpectedEof { Failure::Posix("closed") } else { alert::from_io(&e, true) })
            }
            None => {
                crate::remove(id);
                Err(Failure::Posix("timeout"))
            }
        };
        answer(pid, op, result);
    });
    r
}
