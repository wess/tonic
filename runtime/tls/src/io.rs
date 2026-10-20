//! Reading, writing and closing established TLS connections.

use crate::alert::{self, Failure};
use crate::options::Opts;
use crate::packet;
use crate::{data_term, sock_term, Conn, Info, Socket, State, Tls};
use tonic_core::net::{a, deliver, Active};
use tonic_core::term::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn info(tls: &Tls) -> Info {
    let (common, server, sni): (&rustls::CommonState, bool, Option<String>) = match tls {
        Tls::Client(s) => (s.get_ref().1, false, None),
        Tls::Server(s) => {
            let conn = s.get_ref().1;
            (conn, true, conn.server_name().map(str::to_string))
        }
    };
    let protocol = match common.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => "tlsv1.3",
        _ => "tlsv1.2",
    };
    Info {
        server,
        protocol,
        suite: common.negotiated_cipher_suite().map(|s| format!("{:?}", s.suite())).unwrap_or_default(),
        alpn: common.alpn_protocol().map(<[u8]>::to_vec),
        sni,
        peercerts: common.peer_certificates().map(|cs| cs.iter().map(|c| c.to_vec()).collect()).unwrap_or_default(),
    }
}

/// Registers a finished handshake under `id`, owned by `owner`.
pub fn adopt(id: u64, tls: Tls, owner: u64, opts: &Opts) -> Arc<Conn> {
    let tcp = tls.get_ref().0;
    let _ = tcp.set_nodelay(opts.nodelay);
    let (local, peer) = (tcp.local_addr().ok(), tcp.peer_addr().ok());
    let info = info(&tls);
    let (reader, writer) = tokio::io::split(tls);
    let conn = Arc::new(Conn {
        reader: tokio::sync::Mutex::new(reader),
        writer: tokio::sync::Mutex::new(writer),
        state: parking_lot::Mutex::new(State {
            owner,
            active: opts.active,
            binary: opts.binary,
            packet: opts.packet,
            buffer: Vec::new(),
            reading: false,
            eof: false,
            stats: [0; 4],
        }),
        closed: tokio::sync::watch::channel(false).0,
        info,
        local,
        peer,
    });
    crate::register(id, Socket::Conn(conn.clone()));
    start_reader(id, conn.clone());
    conn
}

pub enum Read {
    Data(Vec<u8>),
    Closed,
    Failed(Failure),
}

/// Waits for the next packet (or `want` bytes in raw mode). Data read before
/// a timeout stays buffered.
pub async fn read(conn: &Arc<Conn>, want: usize) -> Read {
    let mut reader = conn.reader.lock().await;
    let mut closed = conn.closed.subscribe();
    loop {
        {
            let mut st = conn.state.lock();
            if let Some(f) = packet::frame(st.packet, &st.buffer, want) {
                let data = st.buffer[f.skip..f.skip + f.len].to_vec();
                st.buffer.drain(..f.skip + f.len);
                return Read::Data(data);
            }
            if st.eof {
                return Read::Closed;
            }
        }
        if *closed.borrow() {
            return Read::Closed;
        }
        let mut chunk = vec![0u8; 32768];
        let n = tokio::select! {
            r = reader.read(&mut chunk) => r,
            _ = closed.wait_for(|c| *c) => return Read::Closed,
        };
        match n {
            Ok(0) => peer_closed(conn),
            Ok(n) => {
                let mut st = conn.state.lock();
                st.buffer.extend_from_slice(&chunk[..n]);
                st.stats[0] += n as u64;
                st.stats[1] += 1;
            }
            // a peer that drops TCP without close_notify has still closed
            Err(e) if is_closed(&e) => peer_closed(conn),
            Err(e) => return Read::Failed(alert::from_io(&e, conn.info.server)),
        }
    }
}

/// The peer closed. As OTP does, answer with close_notify and refuse further
/// sends rather than keep a half-closed connection.
fn peer_closed(conn: &Arc<Conn>) {
    conn.state.lock().eof = true;
    let conn = conn.clone();
    tonic_core::sched::runtime().spawn(async move {
        let mut writer = conn.writer.lock().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), writer.shutdown()).await;
    });
}

fn is_closed(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(e.kind(), UnexpectedEof | ConnectionReset | BrokenPipe | ConnectionAborted)
}

fn send_to(owner: u64, id: u64, tag: &str, payload: Option<impl FnOnce(&mut tonic_core::nif::Env) -> Term>) {
    deliver(owner, |env| {
        let s = sock_term(env, id);
        match payload {
            Some(f) => {
                let p = f(env);
                env.tuple(&[a(tag), s, p])
            }
            None => env.tuple(&[a(tag), s]),
        }
    });
}

type NoPayload = fn(&mut tonic_core::nif::Env) -> Term;

/// Active mode: delivers packets to the owner until the socket turns passive.
pub fn start_reader(id: u64, conn: Arc<Conn>) {
    {
        let mut st = conn.state.lock();
        if st.reading || st.active == Active::Passive {
            return;
        }
        st.reading = true;
    }
    tonic_core::sched::runtime().spawn(async move {
        loop {
            let result = read(&conn, 0).await;
            if *conn.closed.borrow() {
                // closed on this side: the owner hears nothing more
                return;
            }
            let mut st = conn.state.lock();
            let owner = st.owner;
            match result {
                Read::Data(data) => {
                    if st.active == Active::Passive {
                        // turned passive while waiting: keep the packet for recv
                        let header = packet::header(st.packet, data.len()).unwrap_or_default();
                        let rest = std::mem::take(&mut st.buffer);
                        st.buffer = [header, data, rest].concat();
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
                    let binary = st.binary;
                    drop(st);
                    send_to(owner, id, "ssl", Some(|env: &mut tonic_core::nif::Env| data_term(env, &data, binary)));
                    if passive {
                        send_to(owner, id, "ssl_passive", None::<NoPayload>);
                    }
                    if done {
                        return;
                    }
                }
                Read::Closed => {
                    st.reading = false;
                    drop(st);
                    send_to(owner, id, "ssl_closed", None::<NoPayload>);
                    return;
                }
                Read::Failed(f) => {
                    st.reading = false;
                    drop(st);
                    send_to(owner, id, "ssl_error", Some(|env: &mut tonic_core::nif::Env| alert::term(env, &f)));
                    return;
                }
            }
        }
    });
}

pub async fn write(conn: &Conn, data: &[u8]) -> Result<(), Failure> {
    let mut closed = conn.closed.subscribe();
    if conn.state.lock().eof {
        return Err(Failure::Posix("closed"));
    }
    let mut writer = tokio::select! {
        w = conn.writer.lock() => w,
        _ = closed.wait_for(|c| *c) => return Err(Failure::Posix("closed")),
    };
    let written = tokio::select! {
        r = async {
            writer.write_all(data).await?;
            writer.flush().await
        } => r,
        _ = closed.wait_for(|c| *c) => return Err(Failure::Posix("closed")),
    };
    match written {
        Ok(()) => {
            let mut st = conn.state.lock();
            st.stats[2] += data.len() as u64;
            st.stats[3] += 1;
            Ok(())
        }
        Err(e) if is_closed(&e) => Err(Failure::Posix("closed")),
        Err(e) => Err(alert::from_io(&e, conn.info.server)),
    }
}

/// Sends close_notify (bounded, in the background) and lets the socket go.
pub fn close(conn: Arc<Conn>) {
    conn.closed.send_replace(true);
    tonic_core::sched::runtime().spawn(async move {
        let mut writer = conn.writer.lock().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), writer.shutdown()).await;
    });
}
