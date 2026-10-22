//! Socket and TLS options, read out of the caller's terms into owned data.
//! `lib/ssl.ex` normalizes the Erlang shapes first (charlists to binaries,
//! `{:cert, der, _}` records to DER, the default for `:verify`).

use crate::packet::{self, Packet};
use std::net::IpAddr;
use tonic_core::net::{active_of, ip_of, list_items, option, Active};
use tonic_core::term::*;

#[derive(Clone)]
pub struct Opts {
    pub callback_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub callback_timed_out: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub callback_owner: u64,
    pub callback_operation: u64,
    pub callback_deadline: Option<std::time::Instant>,
    pub verify_callback: bool,
    pub partial_chain_callback: bool,
    pub hostname_callback: bool,
    pub sni_callback: bool,
    pub ciphers: Option<Vec<String>>,
    pub honor_cipher_order: bool,
    pub depth: usize,
    pub local_port: u16,
    pub reuse_sessions: bool,
    pub active: Active,
    pub binary: bool,
    pub packet: Packet,
    pub nodelay: bool,
    pub keepalive: bool,
    pub reuseaddr: bool,
    pub recbuf: Option<usize>,
    pub sndbuf: Option<usize>,
    pub buffer: usize,
    pub packet_size: usize,
    pub backlog: i32,
    pub ip: Option<IpAddr>,
    pub inet6: bool,
    pub verify: bool,
    pub cacerts: Vec<Vec<u8>>,
    pub cacertfile: Option<String>,
    pub certs: Vec<Vec<u8>>,
    pub certfile: Option<String>,
    pub key: Option<Vec<u8>>,
    pub keyfile: Option<String>,
    pub sni: Option<String>,
    pub sni_disabled: bool,
    pub versions: Vec<&'static str>,
    pub alpn: Vec<Vec<u8>>,
    pub alpn_preferred: Vec<Vec<u8>>,
    pub fail_if_no_peer_cert: bool,
    pub sni_hosts: Vec<(String, Opts)>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            callback_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            callback_timed_out: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            callback_owner: 0,
            callback_operation: 0,
            callback_deadline: None,
            verify_callback: false,
            partial_chain_callback: false,
            hostname_callback: false,
            sni_callback: false,
            ciphers: None,
            honor_cipher_order: false,
            depth: 10,
            local_port: 0,
            reuse_sessions: true,
            active: Active::On,
            binary: false,
            packet: Packet::Raw,
            nodelay: false,
            keepalive: false,
            reuseaddr: false,
            recbuf: None,
            sndbuf: None,
            buffer: 65536,
            packet_size: 0,
            backlog: 128,
            ip: None,
            inet6: false,
            verify: false,
            cacerts: Vec::new(),
            cacertfile: None,
            certs: Vec::new(),
            certfile: None,
            key: None,
            keyfile: None,
            sni: None,
            sni_disabled: false,
            versions: vec!["tlsv1.3", "tlsv1.2"],
            alpn: Vec::new(),
            alpn_preferred: Vec::new(),
            fail_if_no_peer_cert: false,
            sni_hosts: Vec::new(),
        }
    }
}

fn text(v: Term) -> Option<String> {
    if is_binary(v) {
        Some(String::from_utf8_lossy(bin_bytes(v)).into_owned())
    } else {
        tonic_core::net::host_of(v).filter(|_| is_list(v) || is_atom(v))
    }
}

fn binaries(v: Term) -> Option<Vec<Vec<u8>>> {
    if is_binary(v) {
        return Some(vec![bin_bytes(v).to_vec()]);
    }
    if !tonic_core::net::proper_list(v) {
        return None;
    }
    list_items(v)
        .into_iter()
        .map(|term| is_binary(term).then(|| bin_bytes(term).to_vec()))
        .collect()
}

fn atom_name(v: Term) -> Option<&'static str> {
    is_atom(v).then(|| tonic_core::atoms::name(atom_idx(v)))
}

/// Applies an option list on top of `self`; the error names the bad option.
pub fn apply(opts: &mut Opts, list: Term) -> Result<(), &'static str> {
    if !tonic_core::net::proper_list(list) {
        return Err("options");
    }
    for item in list_items(list) {
        let Some((name, v)) = option(item) else {
            return Err("options");
        };
        match name {
            "active" => opts.active = active_of(v).ok_or("active")?,
            "binary" => opts.binary = true,
            "list" => opts.binary = false,
            "mode" => match atom_name(v) {
                Some("binary") => opts.binary = true,
                Some("list") => opts.binary = false,
                _ => return Err("mode"),
            },
            "packet" => opts.packet = packet::of(v).ok_or("packet")?,
            "nodelay" if v == TRUE || v == FALSE => opts.nodelay = v == TRUE,
            "keepalive" if v == TRUE || v == FALSE => opts.keepalive = v == TRUE,
            "reuseaddr" if v == TRUE || v == FALSE => opts.reuseaddr = v == TRUE,
            "recbuf" if is_small(v) && small_val(v) > 0 => {
                opts.recbuf = Some(small_val(v) as usize)
            }
            "sndbuf" if is_small(v) && small_val(v) > 0 => {
                opts.sndbuf = Some(small_val(v) as usize)
            }
            "buffer" if is_small(v) && small_val(v) > 0 => opts.buffer = small_val(v) as usize,
            "packet_size" if is_small(v) && small_val(v) >= 0 => {
                opts.packet_size = small_val(v) as usize
            }
            "header" if v == small(0) => {}
            "backlog" if is_small(v) && small_val(v) >= 0 => opts.backlog = small_val(v) as i32,
            "ip" | "ifaddr" => opts.ip = Some(ip_of(v).ok_or("ip")?),
            "inet6" => opts.inet6 = true,
            "inet" => opts.inet6 = false,
            "verify" => match atom_name(v) {
                Some("verify_peer") => opts.verify = true,
                Some("verify_none") => opts.verify = false,
                _ => return Err("verify"),
            },
            "tonic_verify_callback" => opts.verify_callback = v == TRUE,
            "tonic_partial_chain_callback" => opts.partial_chain_callback = v == TRUE,
            "secure_renegotiate" if v == TRUE => {}
            "tonic_hostname_callback" => opts.hostname_callback = v == TRUE,
            "tonic_sni_callback" => opts.sni_callback = v == TRUE,
            "ciphers" => {
                if !tonic_core::net::proper_list(v) {
                    return Err("ciphers");
                }
                let values = list_items(v);
                let ciphers = values
                    .into_iter()
                    .map(text)
                    .collect::<Option<Vec<_>>>()
                    .ok_or("ciphers")?;
                if ciphers.is_empty() {
                    return Err("ciphers");
                }
                opts.ciphers = Some(ciphers);
            }
            "honor_cipher_order" if v == TRUE || v == FALSE => opts.honor_cipher_order = v == TRUE,
            "depth" if is_small(v) && small_val(v) >= 0 => opts.depth = small_val(v) as usize,
            "port" if is_small(v) && (0..=65535).contains(&small_val(v)) => {
                opts.local_port = small_val(v) as u16
            }
            "reuse_sessions" if v == TRUE || v == FALSE => opts.reuse_sessions = v == TRUE,
            "cacerts" => opts.cacerts = binaries(v).ok_or(name)?,
            "cacertfile" => opts.cacertfile = Some(text(v).ok_or(name)?),
            "cert" => opts.certs = binaries(v).ok_or(name)?,
            "certfile" => opts.certfile = Some(text(v).ok_or(name)?),
            "key" if is_binary(v) => opts.key = Some(bin_bytes(v).to_vec()),
            "keyfile" => opts.keyfile = Some(text(v).ok_or(name)?),
            "server_name_indication" => match atom_name(v) {
                Some("disable") => opts.sni_disabled = true,
                _ => opts.sni = Some(text(v).ok_or(name)?),
            },
            "versions" => {
                if !tonic_core::net::proper_list(v) {
                    return Err("versions");
                }
                let entries = list_items(v);
                if entries.iter().any(|entry| !is_atom(*entry)) {
                    return Err("versions");
                }
                let all: Vec<&'static str> =
                    list_items(v).into_iter().filter_map(atom_name).collect();
                if all.iter().any(|n| !matches!(*n, "tlsv1.2" | "tlsv1.3")) {
                    return Err("versions");
                }
                opts.versions = all;
                if opts.versions.is_empty() {
                    return Err("versions");
                }
            }
            "alpn_advertised_protocols" => opts.alpn = binaries(v).ok_or(name)?,
            "alpn_preferred_protocols" => opts.alpn_preferred = binaries(v).ok_or(name)?,
            "fail_if_no_peer_cert" if v == TRUE || v == FALSE => {
                opts.fail_if_no_peer_cert = v == TRUE
            }
            "sni_hosts" => {
                if !tonic_core::net::proper_list(v) {
                    return Err("sni_hosts");
                }
                for entry in list_items(v) {
                    if !is_tuple(entry) || tuple_size(entry) != 2 {
                        return Err("sni_hosts");
                    }
                    let host = text(tuple_get(entry, 0)).ok_or("sni_hosts")?;
                    let mut nested = Opts::default();
                    apply(&mut nested, tuple_get(entry, 1))?;
                    opts.sni_hosts.push((host, nested));
                }
            }
            "log_level" => {}
            _ => return Err(name),
        }
    }
    Ok(())
}
