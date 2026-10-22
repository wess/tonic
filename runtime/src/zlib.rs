//! `:zlib` streams: the native half of OTP's zlib NIF (erts zlib_nif.c),
//! so the bundled lib/erl/zlib.erl runs unchanged on top of it.
//!
//! zlib itself is the stock C library built by libz-sys, which keeps the
//! compressed bytes identical to the BEAM's. A stream is named by the id of
//! the reference returned from `open`; closing it or collecting its final
//! reference releases the codec state.

use crate::bif_bin::flatten_io;
use crate::bif_core::cx;
use crate::heap::{Ctx, Fragment};
use crate::term::*;
use libz_sys as z;
use parking_lot::Mutex;
use std::os::raw::{c_int, c_void};

const DICT_SIZE: usize = 32768;

#[derive(PartialEq)]
enum Mode {
    None,
    Deflate,
    Inflate,
}

struct Stream {
    // zlib keeps a pointer back to the z_stream, so it must never move.
    s: Box<z::z_stream>,
    mode: Mode,
    eos: i64,
    eos_seen: bool,
    raw: bool,
    owner: u64,
    // Queued input; the codec has consumed everything before `at`.
    input: Vec<u8>,
    at: usize,
    stash: Option<Fragment>,
    closed: bool,
}

// The raw pointers in z_stream only refer to zlib's own allocations.
unsafe impl Send for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        end(self);
    }
}

unsafe extern "C" fn zalloc(_: *mut c_void, items: z::uInt, size: z::uInt) -> *mut c_void {
    libc::calloc(items as usize, size as usize)
}

unsafe extern "C" fn zfree(_: *mut c_void, p: *mut c_void) {
    libc::free(p)
}

extern "C" {
    // Not declared by libz-sys, but part of the zlib it builds.
    fn inflateGetDictionary(strm: z::z_streamp, dict: *mut u8, len: *mut z::uInt) -> c_int;
}

fn ok() -> Term {
    atom(crate::atoms::intern("ok"))
}

fn sym(s: &str) -> Term {
    atom(crate::atoms::intern(s))
}

/// A zlib return code as the NIF reports it: errors on the stream or data
/// raise, the rest come back as atoms.
fn code(ctx: &mut Ctx, res: c_int) -> Term {
    match res {
        z::Z_OK => ok(),
        z::Z_STREAM_END => sym("stream_end"),
        z::Z_ERRNO => small(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as i64),
        z::Z_STREAM_ERROR => ctx.raise_error(sym("stream_error")),
        z::Z_DATA_ERROR => ctx.raise_error(sym("data_error")),
        z::Z_MEM_ERROR => sym("mem_error"),
        z::Z_BUF_ERROR => sym("buf_error"),
        z::Z_VERSION_ERROR => sym("version_error"),
        _ => sym("unknown_error"),
    }
}

fn end(st: &mut Stream) {
    unsafe {
        match st.mode {
            Mode::Deflate => {
                z::deflateEnd(&mut *st.s);
            }
            Mode::Inflate => {
                z::inflateEnd(&mut *st.s);
            }
            Mode::None => {}
        }
    }
    st.mode = Mode::None;
}

fn reset_input(st: &mut Stream) {
    st.input = Vec::new();
    st.at = 0;
    st.stash = None;
}

fn pending(st: &Stream) -> usize {
    st.input.len() - st.at
}

fn int(t: Term) -> Option<c_int> {
    if is_small(t) {
        c_int::try_from(small_val(t)).ok()
    } else {
        None
    }
}

/// Run `f` on the stream named by `r` after OTP's checks: an unknown handle
/// is badarg, and only the controlling process may use a stream.
fn with<F>(ctx: &mut Ctx, r: Term, f: F) -> Term
where
    F: FnOnce(&mut Ctx, &mut Stream) -> Term,
{
    let found = crate::resource::get::<Mutex<Stream>>(r);
    let Some(shared) = found else {
        return ctx.badarg("argument error");
    };
    let mut st = shared.lock();
    if st.closed {
        return ctx.badarg("argument error");
    }
    if st.owner != ctx.pid() {
        return ctx.raise_error(sym("not_on_controlling_process"));
    }
    f(ctx, &mut st)
}

/// Like `with`, for operations on an initialised stream of one direction.
fn with_mode<F>(ctx: &mut Ctx, r: Term, mode: Mode, f: F) -> Term
where
    F: FnOnce(&mut Ctx, &mut Stream) -> Term,
{
    with(ctx, r, |ctx, st| if st.mode != mode { ctx.raise_error(sym("not_initialized")) } else { f(ctx, st) })
}

#[no_mangle]
pub extern "C" fn tn_zlib_open(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let s = Box::new(z::z_stream {
        next_in: std::ptr::null_mut(),
        avail_in: 0,
        total_in: 0,
        next_out: std::ptr::null_mut(),
        avail_out: 0,
        total_out: 0,
        msg: std::ptr::null_mut(),
        state: std::ptr::null_mut(),
        zalloc,
        zfree,
        opaque: std::ptr::null_mut(),
        data_type: z::Z_BINARY,
        adler: 0,
        reserved: 0,
    });
    let st = Stream { s, mode: Mode::None, eos: 2, eos_seen: false, raw: false, owner: ctx.pid(), input: Vec::new(), at: 0, stash: None, closed: false };
    ctx.managed_ref(Mutex::new(st))
}

#[no_mangle]
pub extern "C" fn tn_zlib_close(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    with(ctx, r, |_, st| {
        end(st);
        reset_input(st);
        st.closed = true;
        ok()
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_set_controller(c: *mut Ctx, r: Term, p: Term) -> Term {
    let ctx = cx(c);
    if !is_pid(p) {
        return ctx.badarg("argument error");
    }
    with(ctx, r, |_, st| {
        st.owner = pid_id(p);
        ok()
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_deflate_init(c: *mut Ctx, r: Term, level: Term, method: Term, bits: Term, mem: Term, strategy: Term) -> Term {
    let ctx = cx(c);
    let (Some(level), Some(method), Some(bits), Some(mem), Some(strategy)) = (int(level), int(method), int(bits), int(mem), int(strategy)) else {
        return ctx.badarg("argument error");
    };
    with(ctx, r, |ctx, st| {
        if st.mode != Mode::None {
            return ctx.raise_error(sym("already_initialized"));
        }
        let res = unsafe {
            z::deflateInit2_(&mut *st.s, level, method, bits, mem, strategy, z::zlibVersion(), std::mem::size_of::<z::z_stream>() as c_int)
        };
        if res == z::Z_OK {
            st.mode = Mode::Deflate;
            st.eos_seen = false;
            st.raw = bits < 0;
        }
        code(ctx, res)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_inflate_init(c: *mut Ctx, r: Term, bits: Term, eos: Term) -> Term {
    let ctx = cx(c);
    let (Some(bits), Some(eos)) = (int(bits), int(eos)) else {
        return ctx.badarg("argument error");
    };
    with(ctx, r, |ctx, st| {
        if st.mode != Mode::None {
            return ctx.raise_error(sym("already_initialized"));
        }
        let res = unsafe { z::inflateInit2_(&mut *st.s, bits, z::zlibVersion(), std::mem::size_of::<z::z_stream>() as c_int) };
        if res == z::Z_OK {
            st.mode = Mode::Inflate;
            st.eos = eos as i64;
            st.eos_seen = false;
            st.raw = bits < 0;
        }
        code(ctx, res)
    })
}

/// Shared by deflateSetDictionary and inflateSetDictionary.
fn set_dictionary(ctx: &mut Ctx, r: Term, dict: Term, mode: Mode) -> Term {
    let mut bytes = Vec::new();
    if !flatten_io(dict, &mut bytes, false) {
        return ctx.badarg("argument error");
    }
    with_mode(ctx, r, mode, |ctx, st| unsafe {
        if st.mode == Mode::Inflate {
            let res = z::inflateSetDictionary(&mut *st.s, bytes.as_ptr(), bytes.len() as z::uInt);
            return code(ctx, res);
        }
        let res = z::deflateSetDictionary(&mut *st.s, bytes.as_ptr(), bytes.len() as z::uInt);
        if res != z::Z_OK {
            return code(ctx, res);
        }
        // A raw stream does not track the checksum, but callers still get one.
        let sum = if st.raw { z::adler32(0, bytes.as_ptr(), bytes.len() as z::uInt) } else { st.s.adler };
        small(sum as i64)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_deflate_set_dictionary(c: *mut Ctx, r: Term, dict: Term) -> Term {
    set_dictionary(cx(c), r, dict, Mode::Deflate)
}

#[no_mangle]
pub extern "C" fn tn_zlib_inflate_set_dictionary(c: *mut Ctx, r: Term, dict: Term) -> Term {
    set_dictionary(cx(c), r, dict, Mode::Inflate)
}

#[no_mangle]
pub extern "C" fn tn_zlib_inflate_get_dictionary(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    let mut out = vec![0u8; DICT_SIZE];
    let mut len: z::uInt = 0;
    let res = with_mode(ctx, r, Mode::Inflate, |ctx, st| {
        let res = unsafe { inflateGetDictionary(&mut *st.s, out.as_mut_ptr(), &mut len) };
        if res < 0 {
            code(ctx, res)
        } else {
            ok()
        }
    });
    if res != ok() {
        return res;
    }
    ctx.binary(&out[..len as usize])
}

#[no_mangle]
pub extern "C" fn tn_zlib_deflate_reset(c: *mut Ctx, r: Term) -> Term {
    with_mode(cx(c), r, Mode::Deflate, |ctx, st| {
        let res = unsafe { z::deflateReset(&mut *st.s) };
        st.eos_seen = false;
        reset_input(st);
        code(ctx, res)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_inflate_reset(c: *mut Ctx, r: Term) -> Term {
    with_mode(cx(c), r, Mode::Inflate, |ctx, st| {
        let res = unsafe { z::inflateReset(&mut *st.s) };
        st.eos_seen = false;
        reset_input(st);
        code(ctx, res)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_deflate_end(c: *mut Ctx, r: Term) -> Term {
    with_mode(cx(c), r, Mode::Deflate, |ctx, st| {
        let mut res = unsafe { z::deflateEnd(&mut *st.s) };
        if res == z::Z_OK && pending(st) > 0 {
            res = z::Z_DATA_ERROR;
        }
        reset_input(st);
        st.mode = Mode::None;
        code(ctx, res)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_inflate_end(c: *mut Ctx, r: Term) -> Term {
    with_mode(cx(c), r, Mode::Inflate, |ctx, st| {
        let mut res = unsafe { z::inflateEnd(&mut *st.s) };
        if res == z::Z_OK && (!st.eos_seen || pending(st) > 0) {
            res = z::Z_DATA_ERROR;
        }
        reset_input(st);
        st.mode = Mode::None;
        code(ctx, res)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_deflate_params(c: *mut Ctx, r: Term, level: Term, strategy: Term) -> Term {
    let ctx = cx(c);
    let (Some(level), Some(strategy)) = (int(level), int(strategy)) else {
        return ctx.badarg("argument error");
    };
    with_mode(ctx, r, Mode::Deflate, |ctx, st| {
        // zlib.erl flushes before this call, so nothing should be pending;
        // the one-byte buffer catches the case where something still is.
        let mut dummy = 0u8;
        st.s.next_out = &mut dummy;
        st.s.avail_out = 1;
        let res = unsafe { z::deflateParams(&mut *st.s, level, strategy) };
        if st.s.avail_out == 0 {
            return code(ctx, z::Z_STREAM_ERROR);
        }
        code(ctx, res)
    })
}

/// Queue input for the next codec calls. OTP takes an iovec here and can
/// hand back an unqueued tail; flattening the whole of it is equivalent.
#[no_mangle]
pub extern "C" fn tn_zlib_enqueue(c: *mut Ctx, r: Term, data: Term) -> Term {
    let ctx = cx(c);
    let mut bytes = Vec::new();
    let valid = flatten_io(data, &mut bytes, false);
    with(ctx, r, |ctx, st| {
        if st.mode == Mode::None {
            return ctx.raise_error(sym("not_initialized"));
        }
        if !valid {
            return ctx.badarg("argument error");
        }
        st.input.drain(..st.at);
        st.at = 0;
        st.input.extend_from_slice(&bytes);
        ok()
    })
}

enum Outcome {
    Raise(c_int),
    Finished(Vec<u8>),
    Continue(Vec<u8>),
    NeedDict(u64, Vec<u8>),
}

/// One step of zlib_codec: feed up to `limit` queued bytes, flushing with
/// `flush` once the queue is drained, into at most `size` output bytes.
fn step(st: &mut Stream, inflate: bool, limit: usize, size: usize, flush: c_int) -> Outcome {
    let codec = |s: &mut z::z_stream, f: c_int| unsafe { if inflate { z::inflate(s, f) } else { z::deflate(s, f) } };
    let mut out = vec![0u8; size];
    st.s.next_out = out.as_mut_ptr();
    st.s.avail_out = size as z::uInt;
    let mut res = z::Z_OK;
    let mut consumed = 0;
    if pending(st) > 0 && limit > 0 {
        let block = pending(st).min(limit);
        st.s.next_in = st.input[st.at..].as_mut_ptr();
        st.s.avail_in = block as z::uInt;
        res = codec(&mut st.s, z::Z_NO_FLUSH);
        consumed = block - st.s.avail_in as usize;
    }
    st.at += consumed;
    let remaining = pending(st);
    if res == z::Z_OK && remaining == 0 && st.s.avail_out > 0 {
        st.s.next_in = std::ptr::null_mut();
        st.s.avail_in = 0;
        res = codec(&mut st.s, flush);
    }
    let produced = size - st.s.avail_out as usize;
    st.s.next_out = std::ptr::null_mut();
    st.s.next_in = std::ptr::null_mut();
    if res < 0 && res != z::Z_BUF_ERROR {
        return Outcome::Raise(res);
    }
    if res == z::Z_STREAM_END {
        st.eos_seen = true;
    }
    if consumed == 0 && produced == 0 && remaining != 0 {
        return Outcome::Raise(z::Z_DATA_ERROR);
    }
    out.truncate(produced);
    if remaining == 0 && produced < size {
        Outcome::Finished(out)
    } else if res != z::Z_NEED_DICT {
        Outcome::Continue(out)
    } else {
        Outcome::NeedDict(st.s.adler, out)
    }
}

fn outcome(ctx: &mut Ctx, o: Outcome) -> Term {
    let (tag, adler, out) = match o {
        Outcome::Raise(res) => return code(ctx, res),
        Outcome::Finished(out) => ("finished", None, out),
        Outcome::Continue(out) => ("continue", None, out),
        Outcome::NeedDict(a, out) => ("need_dictionary", Some(a), out),
    };
    let list = if out.is_empty() {
        NIL_LIST
    } else {
        let b = ctx.binary(&out);
        ctx.cons(b, NIL_LIST)
    };
    match adler {
        Some(a) => ctx.tuple(&[sym(tag), small(a as i64), list]),
        None => ctx.tuple(&[sym(tag), list]),
    }
}

fn sizes(limit: Term, size: Term, flush: Term) -> Option<(usize, usize, c_int)> {
    let l = usize::try_from(int(limit)?).ok()?;
    let s = usize::try_from(int(size)?).ok()?;
    Some((l, s, int(flush)?))
}

#[no_mangle]
pub extern "C" fn tn_zlib_deflate(c: *mut Ctx, r: Term, limit: Term, size: Term, flush: Term) -> Term {
    let ctx = cx(c);
    let Some((limit, size, flush)) = sizes(limit, size, flush) else {
        return ctx.badarg("argument error");
    };
    with_mode(ctx, r, Mode::Deflate, |ctx, st| {
        let o = step(st, false, limit, size, flush);
        outcome(ctx, o)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_inflate(c: *mut Ctx, r: Term, limit: Term, size: Term, flush: Term) -> Term {
    let ctx = cx(c);
    let Some((limit, size, flush)) = sizes(limit, size, flush) else {
        return ctx.badarg("argument error");
    };
    with_mode(ctx, r, Mode::Inflate, |ctx, st| {
        // Input queued past the end of a stream: an error, the start of
        // another stream (gunzip of concatenated members), or discarded.
        if st.eos_seen && pending(st) > 0 {
            match st.eos {
                0 => return code(ctx, z::Z_DATA_ERROR),
                1 => {
                    let res = unsafe { z::inflateReset(&mut *st.s) };
                    if res != z::Z_OK {
                        return code(ctx, res);
                    }
                    st.eos_seen = false;
                }
                _ => reset_input(st),
            }
        }
        let o = step(st, true, limit, size, flush);
        outcome(ctx, o)
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_get_stash(c: *mut Ctx, r: Term) -> Term {
    with(cx(c), r, |ctx, st| match &st.stash {
        Some(f) => {
            let t = ctx.from_fragment(f);
            ctx.tuple(&[ok(), t])
        }
        None => sym("empty"),
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_clear_stash(c: *mut Ctx, r: Term) -> Term {
    with(cx(c), r, |ctx, st| {
        if st.stash.take().is_none() {
            return ctx.raise_error(sym("error"));
        }
        ok()
    })
}

#[no_mangle]
pub extern "C" fn tn_zlib_set_stash(c: *mut Ctx, r: Term, t: Term) -> Term {
    with(cx(c), r, |ctx, st| {
        if st.stash.is_some() {
            return ctx.raise_error(sym("error"));
        }
        st.stash = Some(ctx.to_fragment(t));
        ok()
    })
}
