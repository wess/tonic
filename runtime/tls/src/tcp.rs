//! TCP options on the transport under a TLS connection.

use crate::options::Opts;
use std::os::fd::{AsFd, BorrowedFd};
use tonic_core::net::a;
use tonic_core::term::*;

pub fn apply(io: &impl AsFd, opts: &Opts) -> std::io::Result<()> {
    let socket = socket2::SockRef::from(io);
    socket.set_tcp_nodelay(opts.nodelay)?;
    socket.set_keepalive(opts.keepalive)?;
    socket.set_reuse_address(opts.reuseaddr)?;
    if let Some(size) = opts.recbuf {
        socket.set_recv_buffer_size(size)?;
    }
    if let Some(size) = opts.sndbuf {
        socket.set_send_buffer_size(size)?;
    }
    Ok(())
}

pub fn set(conn: &crate::Conn, name: &str, value: Term) -> std::io::Result<()> {
    // the I/O halves live in conn, so its descriptor cannot be reused here
    let fd = unsafe { BorrowedFd::borrow_raw(conn.tcp_fd) };
    let socket = socket2::SockRef::from(&fd);
    match name {
        "nodelay" if value == TRUE || value == FALSE => socket.set_tcp_nodelay(value == TRUE),
        "keepalive" if value == TRUE || value == FALSE => socket.set_keepalive(value == TRUE),
        "reuseaddr" if value == TRUE || value == FALSE => socket.set_reuse_address(value == TRUE),
        "recbuf" if is_small(value) && small_val(value) > 0 => {
            socket.set_recv_buffer_size(small_val(value) as usize)
        }
        "sndbuf" if is_small(value) && small_val(value) > 0 => {
            socket.set_send_buffer_size(small_val(value) as usize)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid socket option",
        )),
    }
}

pub fn get(conn: &crate::Conn, name: &str) -> std::io::Result<Term> {
    let fd = unsafe { BorrowedFd::borrow_raw(conn.tcp_fd) };
    get_io(&fd, name)
}

pub fn get_io(io: &impl AsFd, name: &str) -> std::io::Result<Term> {
    let socket = socket2::SockRef::from(io);
    Ok(match name {
        "nodelay" => boolean(socket.tcp_nodelay()?),
        "keepalive" => boolean(socket.keepalive()?),
        "reuseaddr" => boolean(socket.reuse_address()?),
        "recbuf" => small(socket.recv_buffer_size()? as i64),
        "sndbuf" => small(socket.send_buffer_size()? as i64),
        _ => a("undefined"),
    })
}
