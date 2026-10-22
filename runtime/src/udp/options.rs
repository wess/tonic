//! OS-backed datagram options, shared by open and setopts.

use crate::net::{ip_of, ip_term};
use crate::nif::Env;
use crate::term::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::AsRawFd;

#[derive(Clone)]
pub(crate) enum Value {
    Number(u32),
    Flag(bool),
    Interface(Ipv4Addr),
    Index(u32),
    Membership6(Ipv6Addr, u32),
    Membership(IpAddr, IpAddr),
}

#[derive(Clone)]
pub(crate) struct Setting(pub &'static str, pub Value);

pub(crate) fn parse(name: &'static str, value: Term) -> Option<Setting> {
    let value = match name {
        "ttl" | "tos" | "tclass" | "multicast_ttl" => {
            if !is_small(value) || !(0..=255).contains(&small_val(value)) {
                return None;
            }
            Value::Number(small_val(value) as u32)
        }
        "dontroute" | "multicast_loop" | "ipv6_v6only" => {
            if value != TRUE && value != FALSE {
                return None;
            }
            Value::Flag(value == TRUE)
        }
        "multicast_if" if is_small(value) && small_val(value) >= 0 => {
            Value::Index(small_val(value) as u32)
        }
        "multicast_if" => match ip_of(value)? {
            IpAddr::V4(ip) => Value::Interface(ip),
            _ => return None,
        },
        "add_membership" | "drop_membership" => {
            if !is_tuple(value) || tuple_size(value) != 2 {
                return None;
            }
            let group = ip_of(tuple_get(value, 0))?;
            let interface = tuple_get(value, 1);
            match group {
                IpAddr::V6(group) if is_small(interface) && small_val(interface) >= 0 => {
                    Value::Membership6(group, small_val(interface) as u32)
                }
                group => Value::Membership(group, ip_of(interface)?),
            }
        }
        _ => return None,
    };
    Some(Setting(name, value))
}

fn route(sock: &socket2::Socket, value: Option<bool>) -> std::io::Result<bool> {
    let mut flag: libc::c_int = value.unwrap_or(false) as libc::c_int;
    let size = std::mem::size_of_val(&flag) as libc::socklen_t;
    let result = unsafe {
        match value {
            Some(_) => libc::setsockopt(
                sock.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_DONTROUTE,
                (&flag as *const libc::c_int).cast(),
                size,
            ),
            None => {
                let mut len = size;
                libc::getsockopt(
                    sock.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_DONTROUTE,
                    (&mut flag as *mut libc::c_int).cast(),
                    &mut len,
                )
            }
        }
    };
    if result < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(flag != 0)
    }
}

impl Setting {
    pub(crate) fn apply(&self, sock: &socket2::Socket, ipv6: bool) -> std::io::Result<()> {
        use Value::*;
        match (&self.0, &self.1) {
            (&"ttl", Number(n)) if ipv6 => sock.set_unicast_hops_v6(*n),
            (&"ttl", Number(n)) => sock.set_ttl_v4(*n),
            (&"tos", Number(n)) => sock.set_tos_v4(*n),
            (&"tclass", Number(n)) => sock.set_tclass_v6(*n),
            (&"dontroute", Flag(v)) => route(sock, Some(*v)).map(|_| ()),
            (&"ipv6_v6only", Flag(v)) => sock.set_only_v6(*v),
            (&"multicast_ttl", Number(n)) if ipv6 => sock.set_multicast_hops_v6(*n),
            (&"multicast_ttl", Number(n)) => sock.set_multicast_ttl_v4(*n),
            (&"multicast_loop", Flag(v)) if ipv6 => sock.set_multicast_loop_v6(*v),
            (&"multicast_loop", Flag(v)) => sock.set_multicast_loop_v4(*v),
            (&"multicast_if", Interface(ip)) => sock.set_multicast_if_v4(ip),
            (&"multicast_if", Index(index)) => sock.set_multicast_if_v6(*index),
            (&"add_membership", Membership6(group, interface)) => {
                sock.join_multicast_v6(group, *interface)
            }
            (&"drop_membership", Membership6(group, interface)) => {
                sock.leave_multicast_v6(group, *interface)
            }
            (&"add_membership", Membership(IpAddr::V4(group), IpAddr::V4(interface))) => {
                sock.join_multicast_v4(group, interface)
            }
            (&"drop_membership", Membership(IpAddr::V4(group), IpAddr::V4(interface))) => {
                sock.leave_multicast_v4(group, interface)
            }
            _ => Err(std::io::Error::from_raw_os_error(libc::EINVAL)),
        }
    }
}

pub(crate) fn get(
    sock: &socket2::Socket,
    ipv6: bool,
    name: &str,
    env: &mut Env,
) -> std::io::Result<Term> {
    let value = match name {
        "ttl" if ipv6 => small(sock.unicast_hops_v6()? as i64),
        "ttl" => small(sock.ttl_v4()? as i64),
        "tos" => small(sock.tos_v4()? as i64),
        "tclass" => small(sock.tclass_v6()? as i64),
        "dontroute" => boolean(route(sock, None)?),
        "ipv6_v6only" => boolean(sock.only_v6()?),
        "multicast_ttl" if ipv6 => small(sock.multicast_hops_v6()? as i64),
        "multicast_ttl" => small(sock.multicast_ttl_v4()? as i64),
        "multicast_loop" if ipv6 => boolean(sock.multicast_loop_v6()?),
        "multicast_loop" => boolean(sock.multicast_loop_v4()?),
        "multicast_if" if ipv6 => small(sock.multicast_if_v6()? as i64),
        "multicast_if" => ip_term(env, IpAddr::V4(sock.multicast_if_v4()?)),
        _ => return Err(std::io::Error::from_raw_os_error(libc::EINVAL)),
    };
    Ok(value)
}
