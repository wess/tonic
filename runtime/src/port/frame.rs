//! Splits what a port program writes into the messages its owner receives:
//! raw chunks, `{:line, n}` lines, or `{:packet, n}` length-prefixed packets.

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Stream,
    Line(usize),
    Packet(usize),
}

pub enum Piece {
    Data(Vec<u8>),
    Eol(Vec<u8>),
    Noeol(Vec<u8>),
}

pub struct Framer {
    mode: Mode,
    buf: Vec<u8>,
}

impl Framer {
    pub fn new(mode: Mode) -> Framer {
        Framer { mode, buf: Vec::new() }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<Piece> {
        match self.mode {
            Mode::Stream => vec![Piece::Data(bytes.to_vec())],
            Mode::Line(max) => {
                self.buf.extend_from_slice(bytes);
                self.lines(max)
            }
            Mode::Packet(size) => {
                self.buf.extend_from_slice(bytes);
                self.packets(size)
            }
        }
    }

    /// At end of input a partial line still goes out; a partial packet is lost.
    pub fn finish(&mut self) -> Vec<Piece> {
        let rest = std::mem::take(&mut self.buf);
        match self.mode {
            Mode::Line(_) if !rest.is_empty() => vec![Piece::Noeol(rest)],
            _ => Vec::new(),
        }
    }

    /// A newline within `max` bytes ends a line; longer lines go out in
    /// `max`-byte `noeol` pieces.
    fn lines(&mut self, max: usize) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut start = 0;
        loop {
            let rest = &self.buf[start..];
            let window = &rest[..rest.len().min(max + 1)];
            if let Some(i) = window.iter().position(|b| *b == b'\n') {
                out.push(Piece::Eol(rest[..i].to_vec()));
                start += i + 1;
            } else if rest.len() > max {
                out.push(Piece::Noeol(rest[..max].to_vec()));
                start += max;
            } else {
                break;
            }
        }
        self.buf.drain(..start);
        out
    }

    fn packets(&mut self, size: usize) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut start = 0;
        while self.buf.len() - start >= size {
            let len = self.buf[start..start + size].iter().fold(0usize, |acc, b| acc << 8 | *b as usize);
            if self.buf.len() - start - size < len {
                break;
            }
            out.push(Piece::Data(self.buf[start + size..start + size + len].to_vec()));
            start += size + len;
        }
        self.buf.drain(..start);
        out
    }
}

/// The length header written before each command in packet mode; None when
/// the data does not fit it.
pub fn header(mode: Mode, len: usize) -> Option<Vec<u8>> {
    match mode {
        Mode::Packet(size) => {
            if size < 8 && len >> (size * 8) != 0 {
                return None;
            }
            Some((0..size).rev().map(|i| (len >> (i * 8)) as u8).collect())
        }
        _ => Some(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(pieces: Vec<Piece>) -> Vec<String> {
        pieces
            .into_iter()
            .map(|p| match p {
                Piece::Data(d) => format!("data:{}", String::from_utf8_lossy(&d)),
                Piece::Eol(d) => format!("eol:{}", String::from_utf8_lossy(&d)),
                Piece::Noeol(d) => format!("noeol:{}", String::from_utf8_lossy(&d)),
            })
            .collect()
    }

    #[test]
    fn lines_split_at_the_limit() {
        let mut f = Framer::new(Mode::Line(3));
        assert_eq!(show(f.push(b"a\nbbbbbb\ncc")), ["eol:a", "noeol:bbb", "eol:bbb"]);
        assert_eq!(show(f.push(b"c")), Vec::<String>::new());
        assert_eq!(show(f.finish()), ["noeol:ccc"]);
    }

    #[test]
    fn packets_wait_for_their_body() {
        let mut f = Framer::new(Mode::Packet(2));
        assert_eq!(show(f.push(b"\0\x03ab")), Vec::<String>::new());
        assert_eq!(show(f.push(b"c\0\x01z\0")), ["data:abc", "data:z"]);
        assert_eq!(show(f.finish()), Vec::<String>::new());
        assert_eq!(header(Mode::Packet(1), 256), None);
        assert_eq!(header(Mode::Packet(2), 258), Some(vec![1, 2]));
    }
}
