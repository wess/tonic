//! Connecting, upgrading gen_tcp sockets, listening and server handshakes.

use crate::alert::{self, Failure};
use crate::options::{apply, Opts};
use crate::{config, io, lookup, register, reply, sock_term, Listener, Pending, Socket, Tls};
use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::net::{ctx_result, host_of, ok_with, operation, posix, timeout_of, with_timeout};
use tonic_core::term::*;
use rustls::pki_types::ServerName;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpStream;

fn options(list: Term, base: Opts) -> Result<Opts, Failure> {
    let mut opts = base;
    apply(&mut opts, list).map_err(|name| Failure::Options(name, String::new()))?;
    Ok(opts)
}

fn server_name(host: &str) -> Result<ServerName<'static>, Failure> {
    match host.parse::<IpAddr>() {
        Ok(ip) => Ok(ServerName::IpAddress(ip.into())),
        Err(_) => ServerName::try_from(host.to_string()).map_err(|_| Failure::Options("server_name_indication", host.to_string())),
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

async fn handshake_client(config: Arc<rustls::ClientConfig>, name: ServerName<'static>, tcp: TcpStream) -> Result<Tls, Failure> {
    tokio_rustls::TlsConnector::from(config)
        .connect(name, tcp)
        .await
        .map(Tls::Client)
        .map_err(|e| if e.kind() == std::io::ErrorKind::UnexpectedEof { Failure::Posix("closed") } else { alert::from_io(&e, false) })
}

async fn resolve(host: &str, port: u16, inet6: bool) -> Result<Vec<SocketAddr>, Failure> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let found = tokio::net::lookup_host((host, port)).await.map_err(|_| Failure::Posix("nxdomain"))?;
    // gen_tcp connects over IPv4 unless asked for :inet6
    let addrs: Vec<SocketAddr> = found.filter(|a| a.is_ipv6() == inet6).collect();
    if addrs.is_empty() {
        return Err(Failure::Posix("nxdomain"));
    }
    Ok(addrs)
}

#[no_mangle]
pub extern "C" fn tn_ssl_connect(c: *mut Ctx, address: Term, port: Term, opts: Term, timeout: Term) -> Term {
    let ctx = cx(c);
    let host = host_of(address);
    let port = (is_small(port) && (0..=65535).contains(&small_val(port))).then(|| small_val(port) as u16);
    let parsed = options(opts, Opts::default());
    let timeout = timeout_of(timeout);
    let (Some(host), Some(port)) = (host, port) else {
        return refuse(ctx, Failure::Posix("einval"));
    };
    let opts = match parsed {
        Ok(o) => o,
        Err(f) => return refuse(ctx, f),
    };
    let name = server_name(opts.sni.as_deref().unwrap_or(&host));
    let config = config::client(&opts);
    let (r, op, pid) = operation(ctx);
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
            let tcp = TcpStream::connect(&addrs[..]).await.map_err(|e| Failure::Posix(posix(&e)))?;
            handshake_client(config, name, tcp).await
        };
        let result = match with_timeout(timeout, attempt).await {
            Some(Ok(tls)) => {
                let id = tonic_core::sched::next_ref_id();
                io::adopt(id, tls, pid, &opts);
                Ok(id)
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
    let opts = match parsed {
        Ok(o) => o,
        Err(f) => return refuse(ctx, f),
    };
    let Some(tcp) = tonic_core::net::detach(socket) else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let peer = tcp.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    let name = server_name(opts.sni.as_deref().unwrap_or(&peer));
    let config = config::client(&opts);
    let (r, op, pid) = operation(ctx);
    let (config, name) = match (config, name) {
        (Ok(c), Ok(n)) => (c, n),
        (Err(f), _) | (_, Err(f)) => {
            answer(pid, op, Err(f));
            return r;
        }
    };
    tonic_core::sched::runtime().spawn(async move {
        let tcp = match tcp.set_nonblocking(true).and_then(|()| TcpStream::from_std(tcp)) {
            Ok(t) => t,
            Err(e) => return answer(pid, op, Err(Failure::Posix(posix(&e)))),
        };
        let result = match with_timeout(timeout, handshake_client(config, name, tcp)).await {
            Some(Ok(tls)) => {
                let id = tonic_core::sched::next_ref_id();
                io::adopt(id, tls, pid, &opts);
                Ok(id)
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
    let port = (is_small(port) && (0..=65535).contains(&small_val(port))).then(|| small_val(port) as u16);
    let parsed = options(opts, Opts::default());
    let Some(port) = port else {
        return ctx.badarg("argument error");
    };
    let fail = |ctx: &mut Ctx, f: Failure| crate::build(ctx, |env| alert::error(env, &f));
    let opts = match parsed {
        Ok(o) => o,
        Err(f) => return fail(ctx, f),
    };
    let config = config::server(&opts);
    let unspecified = if opts.inet6 { IpAddr::V6(Ipv6Addr::UNSPECIFIED) } else { IpAddr::V4(Ipv4Addr::UNSPECIFIED) };
    let ip = opts.ip.unwrap_or(unspecified);
    let bound = std::net::TcpListener::bind(SocketAddr::new(ip, port)).and_then(|l| {
        l.set_nonblocking(true)?;
        let _guard = tonic_core::sched::runtime().enter();
        tokio::net::TcpListener::from_std(l)
    });
    match bound {
        Ok(io) => {
            let id = tonic_core::sched::next_ref_id();
            register(
                id,
                Socket::Listener(Arc::new(Listener {
                    io,
                    owner: parking_lot::Mutex::new(ctx.pid()),
                    opts: parking_lot::Mutex::new(opts),
                    config,
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
                let opts = listener.opts.lock().clone();
                register(
                    id,
                    Socket::Pending(Arc::new(Pending {
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
    let extra = is_cons(opts);
    let timeout = timeout_of(timeout);
    let (Some(id), Some(Socket::Pending(pending))) = (id, pending) else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let parsed = options(opts, pending.opts.clone());
    let opts = match parsed {
        Ok(o) => o,
        Err(f) => return refuse(ctx, f),
    };
    let config = if extra { config::server(&opts) } else { pending.listener.config.clone() };
    let config = match config {
        Ok(c) => c,
        Err(f) => {
            if let Some(Socket::Pending(p)) = crate::remove(id) {
                drop(p.io.lock().take());
            }
            return refuse(ctx, f);
        }
    };
    let Some(tcp) = pending.io.lock().take() else {
        return refuse(ctx, Failure::Posix("closed"));
    };
    let (r, op, pid) = operation(ctx);
    tonic_core::sched::runtime().spawn(async move {
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(tcp);
        let result = match with_timeout(timeout, accept).await {
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
                Err(if e.kind() == std::io::ErrorKind::UnexpectedEof { Failure::Posix("closed") } else { alert::from_io(&e, true) })
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
