//! `{:packet, _}` framing over decrypted bytes, as gen_tcp does it.

use tonic_core::term::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Packet {
    Raw,
    Prefix(usize),
    Line,
}

/// A complete packet: skip `skip` header bytes, then take `len`.
pub struct Frame {
    pub skip: usize,
    pub len: usize,
}

pub fn of(v: Term) -> Option<Packet> {
    if is_small(v) {
        return match small_val(v) {
            0 => Some(Packet::Raw),
            n @ (1 | 2 | 4) => Some(Packet::Prefix(n as usize)),
            _ => None,
        };
    }
    match is_atom(v).then(|| tonic_core::atoms::name(atom_idx(v))) {
        Some("raw") => Some(Packet::Raw),
        Some("line") => Some(Packet::Line),
        _ => None,
    }
}

pub fn term(packet: Packet) -> Term {
    match packet {
        Packet::Raw => small(0),
        Packet::Prefix(n) => small(n as i64),
        Packet::Line => tonic_core::net::a("line"),
    }
}

/// The next packet in `buf`; `want` is recv's length, used only by raw mode.
pub fn frame(packet: Packet, buf: &[u8], want: usize) -> Option<Frame> {
    match packet {
        Packet::Raw if want == 0 => (!buf.is_empty()).then_some(Frame { skip: 0, len: buf.len() }),
        Packet::Raw => (buf.len() >= want).then_some(Frame { skip: 0, len: want }),
        Packet::Prefix(n) => {
            if buf.len() < n {
                return None;
            }
            let len = buf[..n].iter().fold(0usize, |acc, b| (acc << 8) | *b as usize);
            (buf.len() >= n + len).then_some(Frame { skip: n, len })
        }
        Packet::Line => buf.iter().position(|b| *b == b'\n').map(|i| Frame { skip: 0, len: i + 1 }),
    }
}

/// The length header `send` puts in front of a packet.
pub fn header(packet: Packet, len: usize) -> Result<Vec<u8>, &'static str> {
    match packet {
        Packet::Prefix(n) => {
            if n < 8 && len >> (n * 8) != 0 {
                return Err("emsgsize");
            }
            Ok((0..n).rev().map(|i| (len >> (i * 8)) as u8).collect())
        }
        _ => Ok(Vec::new()),
    }
}
