//! Socket and TLS options, read out of the caller's terms into owned data.
//! `lib/ssl.ex` normalizes the Erlang shapes first (charlists to binaries,
//! `{:cert, der, _}` records to DER, the default for `:verify`).

use tonic_core::net::{active_of, ip_of, list_items, option, Active};
use tonic_core::term::*;
use crate::packet::{self, Packet};
use std::net::IpAddr;

#[derive(Clone)]
pub struct Opts {
    pub active: Active,
    pub binary: bool,
    pub packet: Packet,
    pub nodelay: bool,
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
            active: Active::On,
            binary: false,
            packet: Packet::Raw,
            nodelay: false,
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

fn binaries(v: Term) -> Vec<Vec<u8>> {
    if is_binary(v) {
        return vec![bin_bytes(v).to_vec()];
    }
    list_items(v).into_iter().filter(|t| is_binary(*t)).map(|t| bin_bytes(t).to_vec()).collect()
}

fn atom_name(v: Term) -> Option<&'static str> {
    is_atom(v).then(|| tonic_core::atoms::name(atom_idx(v)))
}

/// Applies an option list on top of `self`; the error names the bad option.
pub fn apply(opts: &mut Opts, list: Term) -> Result<(), &'static str> {
    for item in list_items(list) {
        let Some((name, v)) = option(item) else { continue };
        match name {
            "active" => opts.active = active_of(v).ok_or("active")?,
            "binary" => opts.binary = true,
            "list" => opts.binary = false,
            "mode" => opts.binary = atom_name(v) == Some("binary"),
            "packet" => opts.packet = packet::of(v).ok_or("packet")?,
            "nodelay" => opts.nodelay = v == TRUE,
            "ip" | "ifaddr" => opts.ip = Some(ip_of(v).ok_or("ip")?),
            "inet6" => opts.inet6 = true,
            "inet" => opts.inet6 = false,
            "verify" => opts.verify = atom_name(v) == Some("verify_peer"),
            "cacerts" => opts.cacerts = binaries(v),
            "cacertfile" => opts.cacertfile = text(v),
            "cert" => opts.certs = binaries(v),
            "certfile" => opts.certfile = text(v),
            "key" => opts.key = is_binary(v).then(|| bin_bytes(v).to_vec()),
            "keyfile" => opts.keyfile = text(v),
            "server_name_indication" => match atom_name(v) {
                Some("disable") => opts.sni_disabled = true,
                _ => opts.sni = text(v),
            },
            "versions" => {
                let all: Vec<&'static str> = list_items(v).into_iter().filter_map(atom_name).collect();
                opts.versions = all.into_iter().filter(|n| matches!(*n, "tlsv1.2" | "tlsv1.3")).collect();
                if opts.versions.is_empty() {
                    return Err("versions");
                }
            }
            "alpn_advertised_protocols" => opts.alpn = binaries(v),
            "alpn_preferred_protocols" => opts.alpn_preferred = binaries(v),
            "fail_if_no_peer_cert" => opts.fail_if_no_peer_cert = v == TRUE,
            "sni_hosts" => {
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
            _ => {}
        }
    }
    Ok(())
}
