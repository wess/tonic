//! Socket options shared by TCP listeners and accepted streams.

use super::{a, active_of, ip_of, list_items, option, Active};
use crate::term::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

#[derive(Clone)]
pub(super) struct Options {
    pub active: Active,
    pub binary: bool,
    pub packet: super::packet::Packet,
    pub header: usize,
    pub packet_size: usize,
    pub buffer: usize,
    pub nodelay: bool,
    pub keepalive: bool,
    pub keep_open: bool,
    pub reuseaddr: bool,
    pub recbuf: Option<usize>,
    pub sndbuf: Option<usize>,
    pub send_timeout: Option<Duration>,
    pub send_timeout_close: bool,
    pub ip: Option<IpAddr>,
    pub inet6: bool,
    pub port: u16,
    pub backlog: i32,
    pub settings: Vec<crate::udp::options::Setting>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            active: Active::On,
            binary: false,
            packet: super::packet::Packet::Raw,
            header: 0,
            packet_size: 0,
            buffer: 65536,
            nodelay: false,
            keepalive: false,
            keep_open: false,
            reuseaddr: false,
            recbuf: None,
            sndbuf: None,
            send_timeout: None,
            send_timeout_close: false,
            ip: None,
            inet6: false,
            port: 0,
            backlog: 128,
            settings: Vec::new(),
        }
    }
}

fn positive(v: Term) -> Option<usize> {
    (is_small(v) && small_val(v) > 0).then(|| small_val(v) as usize)
}
fn flag(v: Term) -> Option<bool> {
    if v == TRUE {
        Some(true)
    } else if v == FALSE {
        Some(false)
    } else {
        None
    }
}

pub(super) fn apply(opts: &mut Options, list: Term) -> Option<()> {
    if !super::proper_list(list) {
        return None;
    }
    let mut symbolic = None;
    for item in list_items(list) {
        match option(item)? {
            ("active", value) => opts.active = active_of(value)?,
            ("binary", _) => opts.binary = true,
            ("list", _) => opts.binary = false,
            ("mode", value) if value == a("binary") || value == a("list") => {
                opts.binary = value == a("binary")
            }
            ("packet", value) => opts.packet = super::packet::of(value)?,
            ("header", value) if is_small(value) && small_val(value) >= 0 => {
                opts.header = small_val(value) as usize
            }
            ("packet_size", value) if is_small(value) && small_val(value) >= 0 => {
                opts.packet_size = small_val(value) as usize
            }
            ("buffer", value) => opts.buffer = positive(value)?,
            ("nodelay", value) => opts.nodelay = flag(value)?,
            ("keepalive", value) => opts.keepalive = flag(value)?,
            ("exit_on_close", value) => opts.keep_open = !flag(value)?,
            ("reuseaddr", value) => opts.reuseaddr = flag(value)?,
            ("recbuf", value) => opts.recbuf = Some(positive(value)?),
            ("sndbuf", value) => opts.sndbuf = Some(positive(value)?),
            ("send_timeout", value) if value == a("infinity") => opts.send_timeout = None,
            ("send_timeout", value) if is_small(value) && small_val(value) >= 0 => {
                opts.send_timeout = Some(Duration::from_millis(small_val(value) as u64))
            }
            ("send_timeout_close", value) => opts.send_timeout_close = flag(value)?,
            ("ip" | "ifaddr", value) if value == a("loopback") || value == a("any") => {
                symbolic = Some(value == a("loopback"))
            }
            ("ip" | "ifaddr", value) => {
                opts.ip = Some(ip_of(value)?);
                symbolic = None;
            }
            ("inet", _) => opts.inet6 = false,
            ("inet6", _) => opts.inet6 = true,
            ("port", value) if is_small(value) && (0..=65535).contains(&small_val(value)) => {
                opts.port = small_val(value) as u16
            }
            ("backlog", value)
                if is_small(value) && (0..=i32::MAX as i64).contains(&small_val(value)) =>
            {
                opts.backlog = small_val(value) as i32
            }
            (name, value) => opts.settings.push(crate::udp::options::parse(name, value)?),
        }
    }
    if let Some(loopback) = symbolic {
        opts.ip = Some(match (opts.inet6, loopback) {
            (false, false) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            (false, true) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            (true, false) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            (true, true) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        });
    }
    Some(())
}

pub(super) fn transport(
    socket: &socket2::Socket,
    opts: &Options,
    ipv6: bool,
) -> std::io::Result<()> {
    socket.set_tcp_nodelay(opts.nodelay)?;
    socket.set_keepalive(opts.keepalive)?;
    socket.set_reuse_address(opts.reuseaddr)?;
    if let Some(size) = opts.recbuf {
        socket.set_recv_buffer_size(size)?;
    }
    if let Some(size) = opts.sndbuf {
        socket.set_send_buffer_size(size)?;
    }
    for setting in &opts.settings {
        setting.apply(socket, ipv6)?;
    }
    Ok(())
}
