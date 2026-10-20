//! Host name lookups for `:inet.getaddr/2`, `getaddrs/2` and
//! `gethostbyname/1`. The lookup runs on the I/O driver and answers by
//! message, like the socket calls, so it never blocks a scheduler thread.

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::net::{deliver, error_with, host_of, ip_term, ok_with, operation, socket_term};
use crate::term::*;
use std::net::IpAddr;

/// Replies `{:ok, [address]}` or `{:error, :nxdomain}` for the family
/// (`:inet` or `:inet6`).
#[no_mangle]
pub extern "C" fn tn_inet_getaddrs(c: *mut Ctx, address: Term, family: Term) -> Term {
    let ctx = cx(c);
    let v6 = is_atom(family) && crate::atoms::name(atom_idx(family)) == "inet6";
    let host = host_of(address);
    let (r, op, pid) = operation(ctx);
    crate::sched::runtime().spawn(async move {
        let found = match host {
            Some(h) => tokio::net::lookup_host((h.as_str(), 0)).await.map(|found| found.collect::<Vec<_>>()).ok(),
            None => None,
        };
        let mut ips: Vec<IpAddr> = Vec::new();
        for ip in found.into_iter().flatten().map(|a| a.ip()) {
            if ip.is_ipv6() == v6 && !ips.contains(&ip) {
                ips.push(ip);
            }
        }
        deliver(pid, |env| {
            let rf = socket_term(env, op);
            let value = if ips.is_empty() {
                error_with(env, "nxdomain")
            } else {
                let mut list = NIL_LIST;
                for ip in ips.iter().rev() {
                    let t = ip_term(env, *ip);
                    list = env.cons(t, list);
                }
                ok_with(env, list)
            };
            env.tuple(&[atom(crate::atoms::intern("$tonic_tcp")), rf, value])
        });
    });
    r
}

/// The host name as the system reports it (a binary).
#[no_mangle]
pub extern "C" fn tn_inet_gethostname(c: *mut Ctx) -> Term {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    let len = if n == 0 { buf.iter().position(|b| *b == 0).unwrap_or(buf.len()) } else { 0 };
    cx(c).binary(&buf[..len])
}
