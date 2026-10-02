//! Standard IO with buffering that is safe to use from any process/thread.

use parking_lot::Mutex;
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

struct Out {
    buf: Vec<u8>,
    tty: bool,
}

fn out() -> &'static Mutex<Out> {
    static O: OnceLock<Mutex<Out>> = OnceLock::new();
    O.get_or_init(|| {
        Mutex::new(Out {
            buf: Vec::with_capacity(1 << 16),
            tty: std::io::stdout().is_terminal(),
        })
    })
}

pub fn init() {
    let _ = out();
}

pub fn stdout_write(bytes: &[u8]) {
    let mut o = out().lock();
    o.buf.extend_from_slice(bytes);
    if o.tty || o.buf.len() > (1 << 15) {
        let b = std::mem::take(&mut o.buf);
        let mut so = std::io::stdout().lock();
        let _ = so.write_all(&b);
        let _ = so.flush();
        o.buf = b;
        o.buf.clear();
    }
}

pub fn stdout_flush() {
    let mut o = out().lock();
    if !o.buf.is_empty() {
        let mut so = std::io::stdout().lock();
        let _ = so.write_all(&o.buf);
        let _ = so.flush();
        o.buf.clear();
    }
}

pub fn stderr_write(bytes: &[u8]) {
    stdout_flush();
    let mut e = std::io::stderr().lock();
    let _ = e.write_all(bytes);
    let _ = e.flush();
}
