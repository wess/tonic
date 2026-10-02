//! Natives backing the `:binary`, `:file` and `:filename` modules (lib/erlang_io.ex).
//!
//! Binary natives are prefixed `tn_bf_`, file natives `tn_fio_`.

use crate::atoms;
use crate::bif_bin::flatten_io;
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::term::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn sym(name: &str) -> Term {
    atom(atoms::intern(name))
}

fn ok_tuple(ctx: &mut Ctx, v: Term) -> Term {
    let base = ctx.roots.len();
    ctx.roots.push(v);
    let t = ctx.tuple(&[atom(a::OK), ctx.roots[base]]);
    ctx.roots.truncate(base);
    t
}

fn charlist_from_bytes(ctx: &mut Ctx, b: &[u8]) -> Term {
    // File names are decoded as UTF-8 when possible (native name encoding),
    // otherwise kept as raw bytes (like OTP's raw filenames).
    match std::str::from_utf8(b) {
        Ok(s) => {
            let mut v: Vec<Term> = s.chars().map(|c| small(c as i64)).collect();
            ctx.list_from_vec(&mut v)
        }
        Err(_) => ctx.binary(b),
    }
}

// ============================================================== :binary

/// Patterns: a non-empty binary or a non-empty list of non-empty binaries.
fn get_patterns(p: Term) -> Option<Vec<Vec<u8>>> {
    if is_binary(p) {
        let b = bin_bytes(p);
        if b.is_empty() {
            return None;
        }
        return Some(vec![b.to_vec()]);
    }
    if is_tuple(p) && tuple_size(p) == 2 && tuple_get(p, 0) == sym("tonic_cp") {
        return get_patterns(tuple_get(p, 1));
    }
    if !is_cons(p) {
        return None;
    }
    let mut v = Vec::new();
    let mut x = p;
    while is_cons(x) {
        let h = head(x);
        if !is_binary(h) || bin_len(h) == 0 {
            return None;
        }
        v.push(bin_bytes(h).to_vec());
        x = tail(x);
    }
    if x != NIL_LIST {
        return None;
    }
    Some(v)
}

/// Earliest match in h[start..end] (entire needle inside the range); at equal
/// positions the longest needle wins (OTP semantics).
fn find_at(h: &[u8], start: usize, end: usize, pats: &[Vec<u8>]) -> Option<(usize, usize)> {
    if pats.len() == 1 {
        let n = &pats[0];
        if n.len() > end.saturating_sub(start) {
            return None;
        }
        let hay = &h[start..end];
        if n.len() == 1 {
            return hay.iter().position(|&c| c == n[0]).map(|p| (start + p, 1));
        }
        let first = n[0];
        let last = end - n.len();
        let mut i = start;
        while i <= last {
            match h[i..=last].iter().position(|&c| c == first) {
                None => return None,
                Some(off) => {
                    let j = i + off;
                    if &h[j..j + n.len()] == n.as_slice() {
                        return Some((j, n.len()));
                    }
                    i = j + 1;
                }
            }
        }
        return None;
    }
    let mut firsts = [false; 256];
    for p in pats {
        firsts[p[0] as usize] = true;
    }
    let mut i = start;
    while i < end {
        if firsts[h[i] as usize] {
            let mut best: Option<usize> = None;
            for p in pats {
                if i + p.len() <= end && &h[i..i + p.len()] == p.as_slice() {
                    if best.map_or(true, |b| p.len() > b) {
                        best = Some(p.len());
                    }
                }
            }
            if let Some(l) = best {
                return Some((i, l));
            }
        }
        i += 1;
    }
    None
}

fn all_matches(h: &[u8], start: usize, end: usize, pats: &[Vec<u8>]) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut s = start;
    while let Some((p, l)) = find_at(h, s, end, pats) {
        v.push((p, l));
        s = p + l;
    }
    v
}

/// Validate a scope (already normalised to non-negative start/len by caller).
fn scope_of(b: Term, start: Term, len: Term) -> Option<(usize, usize)> {
    if !is_small(start) || !is_small(len) {
        return None;
    }
    let (s, l) = (small_val(start), small_val(len));
    if s < 0 || l < 0 || (s + l) as usize > bin_len(b) {
        return None;
    }
    Some((s as usize, (s + l) as usize))
}

/// match/matches with scope. all = true → list of {pos, len}.
#[no_mangle]
pub extern "C" fn tn_bf_match(c: *mut Ctx, b: Term, p: Term, start: Term, len: Term, all: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    let pats = match get_patterns(p) {
        Some(p) => p,
        None => return ctx.badarg_args(&[(2, "not a valid pattern")]),
    };
    let (s, e) = match scope_of(b, start, len) {
        Some(x) => x,
        None => return ctx.badarg_args(&[(3, "invalid options")]),
    };
    let h = bin_bytes(b);
    if all == TRUE {
        let found = all_matches(h, s, e, &pats);
        let base = ctx.roots.len();
        for &(pos, l) in &found {
            let t = ctx.tuple(&[small(pos as i64), small(l as i64)]);
            ctx.roots.push(t);
        }
        let l = ctx.list_from_roots(base, found.len(), NIL_LIST);
        ctx.roots.truncate(base);
        l
    } else {
        match find_at(h, s, e, &pats) {
            Some((pos, l)) => ctx.tuple(&[small(pos as i64), small(l as i64)]),
            None => sym("nomatch"),
        }
    }
}

/// split(subject, pattern, scope_start, scope_len, flags) flags: 1 global, 2 trim, 4 trim_all
#[no_mangle]
pub extern "C" fn tn_bf_split(c: *mut Ctx, b: Term, p: Term, start: Term, len: Term, flags: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    let pats = match get_patterns(p) {
        Some(p) => p,
        None => return ctx.badarg_args(&[(2, "not a valid pattern")]),
    };
    let (s, e) = match scope_of(b, start, len) {
        Some(x) => x,
        None => return ctx.badarg_args(&[(3, "invalid options")]),
    };
    let fl = small_val(flags);
    let h = bin_bytes(b);
    let total = h.len();
    let matches = if fl & 1 != 0 {
        all_matches(h, s, e, &pats)
    } else {
        find_at(h, s, e, &pats).into_iter().collect()
    };
    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(matches.len() + 1);
    let mut prev = 0;
    for (pos, l) in matches {
        ranges.push((prev, pos));
        prev = pos + l;
    }
    ranges.push((prev, total));
    if fl & 4 != 0 {
        ranges.retain(|&(a, z)| z > a);
    } else if fl & 2 != 0 {
        while let Some(&(a, z)) = ranges.last() {
            if z == a {
                ranges.pop();
            } else {
                break;
            }
        }
    }
    let bi = ctx.push(b);
    let base = ctx.roots.len();
    for &(a, z) in &ranges {
        let piece = ctx.sub_binary(ctx.get(bi), a, z - a);
        ctx.roots.push(piece);
    }
    let l = ctx.list_from_roots(base, ranges.len(), NIL_LIST);
    ctx.truncate(bi);
    l
}

/// replace with a plain binary replacement.
#[no_mangle]
pub extern "C" fn tn_bf_replace(c: *mut Ctx, b: Term, p: Term, r: Term, scope: Term, global: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    let pats = match get_patterns(p) {
        Some(p) => p,
        None => return ctx.badarg_args(&[(2, "not a valid pattern")]),
    };
    if !is_binary(r) {
        return ctx.badarg_args(&[(3, "not a binary")]);
    }
    let (s, e) = if is_tuple(scope) && tuple_size(scope) == 2 {
        match scope_of(b, tuple_get(scope, 0), tuple_get(scope, 1)) {
            Some(x) => x,
            None => return ctx.badarg_args(&[(4, "invalid options")]),
        }
    } else {
        (0, bin_len(b))
    };
    let h = bin_bytes(b);
    let rep = bin_bytes(r).to_vec();
    let matches = if global == TRUE {
        all_matches(h, s, e, &pats)
    } else {
        find_at(h, s, e, &pats).into_iter().collect()
    };
    if matches.is_empty() {
        return b;
    }
    let mut out = Vec::with_capacity(h.len() + matches.len() * rep.len());
    let mut prev = 0;
    for (pos, l) in matches {
        out.extend_from_slice(&h[prev..pos]);
        out.extend_from_slice(&rep);
        prev = pos + l;
    }
    out.extend_from_slice(&h[prev..]);
    ctx.binary(&out)
}

#[no_mangle]
pub extern "C" fn tn_bf_encode_hex(c: *mut Ctx, b: Term, upper: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    let digits: &[u8; 16] = if upper == TRUE { b"0123456789ABCDEF" } else { b"0123456789abcdef" };
    let h = bin_bytes(b);
    let mut out = Vec::with_capacity(h.len() * 2);
    for &x in h {
        out.push(digits[(x >> 4) as usize]);
        out.push(digits[(x & 15) as usize]);
    }
    ctx.binary(&out)
}

#[no_mangle]
pub extern "C" fn tn_bf_decode_hex(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || bin_len(b) % 2 != 0 {
        return ctx.badarg_args(&[(1, "not a valid hex string")]);
    }
    fn un(x: u8) -> Option<u8> {
        match x {
            b'0'..=b'9' => Some(x - b'0'),
            b'a'..=b'f' => Some(x - b'a' + 10),
            b'A'..=b'F' => Some(x - b'A' + 10),
            _ => None,
        }
    }
    let h = bin_bytes(b);
    let mut out = Vec::with_capacity(h.len() / 2);
    for ch in h.chunks(2) {
        match (un(ch[0]), un(ch[1])) {
            (Some(x), Some(y)) => out.push(x << 4 | y),
            _ => return ctx.badarg_args(&[(1, "not a valid hex string")]),
        }
    }
    ctx.binary(&out)
}

/// longest common prefix (suffix = false) / suffix (true) of a list of binaries.
#[no_mangle]
pub extern "C" fn tn_bf_common(c: *mut Ctx, l: Term, suffix: Term) -> Term {
    let ctx = cx(c);
    let mut v: Vec<&[u8]> = Vec::new();
    let mut x = l;
    while is_cons(x) {
        let h = head(x);
        if !is_binary(h) {
            return ctx.badarg_args(&[(1, "not a list of binaries")]);
        }
        v.push(bin_bytes(h));
        x = tail(x);
    }
    if x != NIL_LIST || v.is_empty() {
        return ctx.badarg_args(&[(1, "not a non-empty list of binaries")]);
    }
    let first = v[0];
    let mut n = first.len();
    for s in &v[1..] {
        let mut k = 0;
        let m = n.min(s.len());
        if suffix == TRUE {
            while k < m && first[first.len() - 1 - k] == s[s.len() - 1 - k] {
                k += 1;
            }
        } else {
            while k < m && first[k] == s[k] {
                k += 1;
            }
        }
        n = k;
    }
    small(n as i64)
}

// ============================================================== errors

fn errno_name(code: i32) -> &'static str {
    #[cfg(target_os = "macos")]
    {
        match code {
            35 => return "eagain",
            36 => return "einprogress",
            37 => return "ealready",
            38 => return "enotsock",
            45 => return "enotsup",
            48 => return "eaddrinuse",
            54 => return "econnreset",
            60 => return "etimedout",
            61 => return "econnrefused",
            62 => return "eloop",
            63 => return "enametoolong",
            66 => return "enotempty",
            69 => return "edquot",
            77 => return "enolck",
            78 => return "enosys",
            84 => return "eoverflow",
            _ => {}
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        match code {
            35 => return "edeadlk",
            36 => return "enametoolong",
            37 => return "enolck",
            38 => return "enosys",
            39 => return "enotempty",
            40 => return "eloop",
            61 => return "enodata",
            75 => return "eoverflow",
            95 => return "enotsup",
            98 => return "eaddrinuse",
            104 => return "econnreset",
            110 => return "etimedout",
            111 => return "econnrefused",
            122 => return "edquot",
            _ => {}
        }
    }
    match code {
        1 => "eperm",
        2 => "enoent",
        3 => "esrch",
        4 => "eintr",
        5 => "eio",
        6 => "enxio",
        7 => "e2big",
        8 => "enoexec",
        9 => "ebadf",
        10 => "echild",
        11 => "eagain",
        12 => "enomem",
        13 => "eacces",
        14 => "efault",
        15 => "enotblk",
        16 => "ebusy",
        17 => "eexist",
        18 => "exdev",
        19 => "enodev",
        20 => "enotdir",
        21 => "eisdir",
        22 => "einval",
        23 => "enfile",
        24 => "emfile",
        25 => "enotty",
        26 => "etxtbsy",
        27 => "efbig",
        28 => "enospc",
        29 => "espipe",
        30 => "erofs",
        31 => "emlink",
        32 => "epipe",
        33 => "edom",
        34 => "erange",
        _ => "eio",
    }
}

fn posix_of(e: &std::io::Error) -> &'static str {
    if let Some(code) = e.raw_os_error() {
        return errno_name(code);
    }
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "enoent",
        PermissionDenied => "eacces",
        AlreadyExists => "eexist",
        InvalidInput => "einval",
        UnexpectedEof => "eof",
        _ => "eio",
    }
}

fn err(ctx: &mut Ctx, e: &std::io::Error) -> Term {
    let r = sym(posix_of(e));
    ctx.tuple(&[atom(a::ERROR), r])
}

fn err_atom(ctx: &mut Ctx, name: &str) -> Term {
    let r = sym(name);
    ctx.tuple(&[atom(a::ERROR), r])
}

/// erl_posix_msg:message/1 as a binary (unknown → "unknown POSIX error: x").
#[no_mangle]
pub extern "C" fn tn_fio_posix_msg(c: *mut Ctx, reason: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(reason) {
        return NIL;
    }
    let name = atoms::name(atom_idx(reason));
    let m = match name {
        "e2big" => "argument list too long",
        "eacces" => "permission denied",
        "eaddrinuse" => "address already in use",
        "eaddrnotavail" => "can't assign requested address",
        "eadv" => "advertise error",
        "eafnosupport" => "address family not supported by protocol family",
        "eagain" => "resource temporarily unavailable",
        "ealign" => "EALIGN",
        "ealready" => "operation already in progress",
        "ebade" => "bad exchange descriptor",
        "ebadf" => "bad file number",
        "ebadfd" => "file descriptor in bad state",
        "ebadmsg" => "not a data message",
        "ebadr" => "bad request descriptor",
        "ebadrpc" => "RPC structure is bad",
        "ebadrqc" => "bad request code",
        "ebadslt" => "invalid slot",
        "ebfont" => "bad font file format",
        "ebusy" => "file busy",
        "echild" => "no children",
        "echrng" => "channel number out of range",
        "ecomm" => "communication error on send",
        "econnaborted" => "software caused connection abort",
        "econnrefused" => "connection refused",
        "econnreset" => "connection reset by peer",
        "edeadlk" => "resource deadlock avoided",
        "edeadlock" => "resource deadlock avoided",
        "edestaddrreq" => "destination address required",
        "edirty" => "mounting a dirty fs w/o force",
        "edom" => "math argument out of range",
        "edotdot" => "cross mount point",
        "edquot" => "disk quota exceeded",
        "eduppkg" => "duplicate package name",
        "eexist" => "file already exists",
        "efault" => "bad address in system call argument",
        "efbig" => "file too large",
        "eftype" => "EFTYPE",
        "ehostdown" => "host is down",
        "ehostunreach" => "host is unreachable",
        "eidrm" => "identifier removed",
        "einit" => "initialization error",
        "einprogress" => "operation now in progress",
        "eintr" => "interrupted system call",
        "einval" => "invalid argument",
        "eio" => "I/O error",
        "eisconn" => "socket is already connected",
        "eisdir" => "illegal operation on a directory",
        "eisnam" => "is a name file",
        "elbin" => "ELBIN",
        "el2hlt" => "level 2 halted",
        "el2nsync" => "level 2 not synchronized",
        "el3hlt" => "level 3 halted",
        "el3rst" => "level 3 reset",
        "elibacc" => "cannot access a needed shared library",
        "elibbad" => "accessing a corrupted shared library",
        "elibexec" => "cannot exec a shared library directly",
        "elibmax" => "attempting to link in more shared libraries than system limit",
        "elibscn" => ".lib section in a.out corrupted",
        "elnrng" => "link number out of range",
        "eloop" => "too many levels of symbolic links",
        "emfile" => "too many open files",
        "emlink" => "too many links",
        "emsgsize" => "message too long",
        "emultihop" => "multihop attempted",
        "enametoolong" => "file name too long",
        "enavail" => "not available",
        "enet" => "ENET",
        "enetdown" => "network is down",
        "enetreset" => "network dropped connection on reset",
        "enetunreach" => "network is unreachable",
        "enfile" => "file table overflow",
        "enoano" => "anode table overflow",
        "enobufs" => "no buffer space available",
        "enocsi" => "no CSI structure available",
        "enodata" => "no data available",
        "enodev" => "no such device",
        "enoent" => "no such file or directory",
        "enoexec" => "exec format error",
        "enolck" => "no locks available",
        "enolink" => "link has be severed",
        "enomem" => "not enough memory",
        "enomsg" => "no message of desired type",
        "enonet" => "machine is not on the network",
        "enopkg" => "package not installed",
        "enoprotoopt" => "bad proocol option",
        "enospc" => "no space left on device",
        "enosr" => "out of stream resources or not a stream device",
        "enostr" => "not a stream",
        "enosym" => "unresolved symbol name",
        "enosys" => "function not implemented",
        "enotblk" => "block device required",
        "enotconn" => "socket is not connected",
        "enotdir" => "not a directory",
        "enotempty" => "directory not empty",
        "enotnam" => "not a name file",
        "enotsock" => "socket operation on non-socket",
        "enotsup" => "operation not supported",
        "enotty" => "inappropriate device for ioctl",
        "enotuniq" => "name not unique on network",
        "enxio" => "no such device or address",
        "eopnotsupp" => "operation not supported on socket",
        "eoverflow" => "offset too large for file system",
        "eperm" => "not owner",
        "epfnosupport" => "protocol family not supported",
        "epipe" => "broken pipe",
        "eproclim" => "too many processes",
        "eprocunavail" => "bad procedure for program",
        "eprogmismatch" => "program version wrong",
        "eprogunavail" => "RPC program not available",
        "eproto" => "protocol error",
        "eprotonosupport" => "protocol not supported",
        "eprototype" => "protocol wrong type for socket",
        "erange" => "math result unrepresentable",
        "erefused" => "EREFUSED",
        "eremchg" => "remote address changed",
        "eremdev" => "remote device",
        "eremote" => "pathname hit remote file system",
        "eremoteio" => "remote i/o error",
        "eremoterelease" => "EREMOTERELEASE",
        "erofs" => "read-only file system",
        "erpcmismatch" => "RPC version is wrong",
        "erremote" => "object is remote",
        "eshutdown" => "can't send after socket shutdown",
        "esocktnosupport" => "socket type not supported",
        "espipe" => "invalid seek",
        "esrch" => "no such process",
        "esrmnt" => "srmount error",
        "estale" => "stale remote file handle",
        "esuccess" => "Error 0",
        "etime" => "timer expired",
        "etimedout" => "connection timed out",
        "etoomanyrefs" => "too many references: can't splice",
        "etxtbsy" => "text file or pseudo-device busy",
        "euclean" => "structure needs cleaning",
        "eunatch" => "protocol driver not attached",
        "eusers" => "too many users",
        "eversion" => "version mismatch",
        "ewouldblock" => "operation would block",
        "exdev" => "cross-device link",
        "exfull" => "message tables full",
        "nxdomain" => "non-existing domain",
        "exbadport" => "inet_drv bad port state",
        "exbadseq" => "inet_drv bad request sequence",
        other => {
            let s = format!("unknown POSIX error: {}", other);
            return ctx.str(&s);
        }
    };
    ctx.str(m)
}

// ============================================================== paths & time

fn path_arg(t: Term) -> Option<PathBuf> {
    let mut v = Vec::new();
    if is_binary(t) {
        v.extend_from_slice(bin_bytes(t));
    } else if !flatten_io(t, &mut v, true) {
        return None;
    }
    if v.contains(&0) {
        return None;
    }
    Some(PathBuf::from(OsStr::from_bytes(&v)))
}

#[repr(C)]
#[derive(Default)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    _pad: [u64; 4],
}

extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    fn gmtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    fn mktime(tm: *mut Tm) -> i64;
    fn timegm(tm: *mut Tm) -> i64;
    fn access(path: *const u8, mode: i32) -> i32;
    fn chown(path: *const u8, uid: u32, gid: u32) -> i32;
}

fn datetime(ctx: &mut Ctx, secs: i64, local: bool) -> Term {
    let mut tm = Tm::default();
    unsafe {
        if local {
            localtime_r(&secs, &mut tm);
        } else {
            gmtime_r(&secs, &mut tm);
        }
    }
    let d = ctx.tuple(&[small(tm.year as i64 + 1900), small(tm.mon as i64 + 1), small(tm.mday as i64)]);
    let base = ctx.roots.len();
    ctx.roots.push(d);
    let t = ctx.tuple(&[small(tm.hour as i64), small(tm.min as i64), small(tm.sec as i64)]);
    ctx.roots.push(t);
    let r = ctx.tuple(&[ctx.roots[base], ctx.roots[base + 1]]);
    ctx.roots.truncate(base);
    r
}

/// {{Y,M,D},{h,m,s}} (local or universal) → posix seconds.
fn to_posix(t: Term, local: bool) -> Option<i64> {
    if is_small(t) {
        return Some(small_val(t));
    }
    if !is_tuple(t) || tuple_size(t) != 2 {
        return None;
    }
    let (d, tm) = (tuple_get(t, 0), tuple_get(t, 1));
    if !is_tuple(d) || tuple_size(d) != 3 || !is_tuple(tm) || tuple_size(tm) != 3 {
        return None;
    }
    let g = |x: Term, i: usize| -> Option<i32> {
        let v = tuple_get(x, i);
        if is_small(v) {
            Some(small_val(v) as i32)
        } else {
            None
        }
    };
    let mut s = Tm {
        year: g(d, 0)? - 1900,
        mon: g(d, 1)? - 1,
        mday: g(d, 2)?,
        hour: g(tm, 0)?,
        min: g(tm, 1)?,
        sec: g(tm, 2)?,
        isdst: -1,
        ..Default::default()
    };
    Some(unsafe {
        if local {
            mktime(&mut s)
        } else {
            timegm(&mut s)
        }
    })
}

/// Read file info. follow: true = stat, false = lstat. tmode: 0 local, 1 universal, 2 posix.
#[no_mangle]
pub extern "C" fn tn_fio_info(c: *mut Ctx, p: Term, follow: Term, tmode: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    let md = if follow == TRUE { std::fs::metadata(&path) } else { std::fs::symlink_metadata(&path) };
    let md = match md {
        Ok(m) => m,
        Err(e) => return err(ctx, &e),
    };
    use std::os::unix::fs::FileTypeExt;
    let ft = md.file_type();
    let typ = if ft.is_symlink() {
        "symlink"
    } else if ft.is_dir() {
        "directory"
    } else if ft.is_file() {
        "regular"
    } else if ft.is_block_device() || ft.is_char_device() {
        "device"
    } else {
        "other"
    };
    let mut cpath = path.as_os_str().as_bytes().to_vec();
    cpath.push(0);
    let r = unsafe { access(cpath.as_ptr(), 4) } == 0;
    let w = unsafe { access(cpath.as_ptr(), 2) } == 0;
    let acc = match (r, w) {
        (true, true) => "read_write",
        (true, false) => "read",
        (false, true) => "write",
        _ => "none",
    };
    let tm = small_val(tmode);
    let base = ctx.roots.len();
    for secs in [md.atime(), md.mtime(), md.ctime()] {
        let t = if tm == 2 { small(secs) } else { datetime(ctx, secs, tm == 0) };
        ctx.roots.push(t);
    }
    let elems = [
        sym("file_info"),
        small(md.size() as i64),
        sym(typ),
        sym(acc),
        ctx.roots[base],
        ctx.roots[base + 1],
        ctx.roots[base + 2],
        small(md.mode() as i64),
        small(md.nlink() as i64),
        small(md.dev() as i64),
        small(md.rdev() as i64),
        small(md.ino() as i64),
        small(md.uid() as i64),
        small(md.gid() as i64),
    ];
    let rec = ctx.tuple(&elems);
    ctx.roots.truncate(base);
    ok_tuple(ctx, rec)
}

/// write_file_info(path, record, tmode)
#[no_mangle]
pub extern "C" fn tn_fio_write_info(c: *mut Ctx, p: Term, rec: Term, tmode: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    if !is_tuple(rec) || tuple_size(rec) != 14 {
        return ctx.badarg_args(&[(2, "not a file_info record")]);
    }
    let local = small_val(tmode) == 0;
    let undef = sym("undefined");
    let mode = tuple_get(rec, 7);
    if is_small(mode) {
        let perm = std::fs::Permissions::from_mode(small_val(mode) as u32 & 0o7777);
        if let Err(e) = std::fs::set_permissions(&path, perm) {
            return err(ctx, &e);
        }
    }
    let (uid, gid) = (tuple_get(rec, 12), tuple_get(rec, 13));
    if is_small(uid) || is_small(gid) {
        let mut cpath = path.as_os_str().as_bytes().to_vec();
        cpath.push(0);
        let u = if is_small(uid) { small_val(uid) as u32 } else { u32::MAX };
        let g = if is_small(gid) { small_val(gid) as u32 } else { u32::MAX };
        if unsafe { chown(cpath.as_ptr(), u, g) } != 0 {
            return err(ctx, &std::io::Error::last_os_error());
        }
    }
    let (at, mt) = (tuple_get(rec, 4), tuple_get(rec, 5));
    if at != undef || mt != undef {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let a_s = if at != undef { to_posix(at, local) } else { None };
        let m_s = if mt != undef { to_posix(mt, local) } else { None };
        // OTP: a missing atime/mtime defaults to the other one, or now.
        let a_s = a_s.or(m_s).unwrap_or(now);
        let m_s = m_s.unwrap_or(a_s);
        if let Err(e) = set_times(&path, a_s, m_s) {
            return err(ctx, &e);
        }
    }
    atom(a::OK)
}

fn systime(s: i64) -> std::time::SystemTime {
    if s >= 0 {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(s as u64)
    } else {
        std::time::UNIX_EPOCH - std::time::Duration::from_secs((-s) as u64)
    }
}

fn set_times(path: &Path, atime: i64, mtime: i64) -> std::io::Result<()> {
    let f = std::fs::File::open(path)?;
    let times = std::fs::FileTimes::new().set_accessed(systime(atime)).set_modified(systime(mtime));
    f.set_times(times)
}

/// Generic path operations. op:
/// 0 make_dir, 1 del_dir, 2 delete, 3 rename, 4 make_link, 5 make_symlink,
/// 6 set_cwd, 7 change_mode(arg = int), 8 del_dir_r, 9 change_owner(uid), 10 change_group(gid)
#[no_mangle]
pub extern "C" fn tn_fio_op(c: *mut Ctx, op: Term, p: Term, arg: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    let op = small_val(op);
    let path2 = || path_arg(arg).ok_or_else(|| std::io::Error::from_raw_os_error(22));
    let r: std::io::Result<()> = match op {
        0 => std::fs::create_dir(&path),
        1 => std::fs::remove_dir(&path),
        2 => match std::fs::symlink_metadata(&path) {
            // erts: "Linux sets the wrong error code" (EISDIR) -> EPERM
            Ok(m) if m.is_dir() => Err(std::io::Error::from_raw_os_error(1)),
            _ => std::fs::remove_file(&path),
        },
        3 => path2().and_then(|q| std::fs::rename(&path, q)),
        4 => path2().and_then(|q| std::fs::hard_link(&path, q)),
        5 => path2().and_then(|q| std::os::unix::fs::symlink(&path, q)),
        6 => std::env::set_current_dir(&path),
        7 => {
            if !is_small(arg) {
                return ctx.badarg_args(&[(2, "not an integer")]);
            }
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(small_val(arg) as u32))
        }
        8 => match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&path),
            Ok(_) => std::fs::remove_file(&path),
            Err(e) => Err(e),
        },
        9 | 10 => {
            if !is_small(arg) {
                return ctx.badarg_args(&[(2, "not an integer")]);
            }
            let mut cpath = path.as_os_str().as_bytes().to_vec();
            cpath.push(0);
            let v = small_val(arg) as u32;
            let (u, g) = if op == 9 { (v, u32::MAX) } else { (u32::MAX, v) };
            if unsafe { chown(cpath.as_ptr(), u, g) } != 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        _ => return ctx.badarg("argument error"),
    };
    // erts reports ENOTEMPTY as EEXIST for del_dir and rename
    let r = match r {
        Err(e) if (op == 1 || op == 3) && e.raw_os_error() == Some(if cfg!(target_os = "macos") { 66 } else { 39 }) => {
            Err(std::io::Error::from_raw_os_error(17))
        }
        other => other,
    };
    match r {
        Ok(()) => atom(a::OK),
        Err(e) => err(ctx, &e),
    }
}

/// change_time(path, atime, mtime, tmode) — atime/mtime as datetime or posix
#[no_mangle]
pub extern "C" fn tn_fio_set_times(c: *mut Ctx, p: Term, at: Term, mt: Term, tmode: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    let local = small_val(tmode) == 0;
    let (a_s, m_s) = match (to_posix(at, local), to_posix(mt, local)) {
        (Some(x), Some(y)) => (x, y),
        _ => return ctx.badarg("argument error"),
    };
    match set_times(&path, a_s, m_s) {
        Ok(()) => atom(a::OK),
        Err(e) => err(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_fio_read_link(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    match std::fs::read_link(&path) {
        Ok(t) => {
            let l = charlist_from_bytes(ctx, t.as_os_str().as_bytes());
            ok_tuple(ctx, l)
        }
        Err(e) => err(ctx, &e),
    }
}

/// list_dir: {:ok, [charlist]} in directory order. all = true keeps undecodable names as binaries.
#[no_mangle]
pub extern "C" fn tn_fio_list_dir(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    match std::fs::read_dir(&path) {
        Ok(rd) => {
            let names: Vec<Vec<u8>> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().as_bytes().to_vec()).collect();
            let base = ctx.roots.len();
            for n in &names {
                let t = charlist_from_bytes(ctx, n);
                ctx.roots.push(t);
            }
            let l = ctx.list_from_roots(base, names.len(), NIL_LIST);
            ctx.roots.truncate(base);
            ok_tuple(ctx, l)
        }
        Err(e) => err(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_fio_get_cwd(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    match std::env::current_dir() {
        Ok(d) => {
            let l = charlist_from_bytes(ctx, d.as_os_str().as_bytes());
            ok_tuple(ctx, l)
        }
        Err(e) => err(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_fio_read_file(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    match std::fs::read(&path) {
        Ok(b) => {
            let bin = ctx.binary(&b);
            ok_tuple(ctx, bin)
        }
        Err(e) => err(ctx, &e),
    }
}

const F_READ: i64 = 1;
const F_WRITE: i64 = 2;
const F_APPEND: i64 = 4;
const F_EXCL: i64 = 8;
const F_SYNC: i64 = 16;

fn open_opts(path: &Path, fl: i64) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    let write = fl & (F_WRITE | F_APPEND | F_EXCL) != 0;
    let read = fl & F_READ != 0 || !write;
    o.read(read);
    if write {
        o.write(true);
        if fl & F_EXCL != 0 {
            o.create_new(true);
        } else {
            o.create(true);
        }
        if fl & F_APPEND != 0 {
            o.append(true);
        } else if fl & F_READ == 0 && fl & F_WRITE != 0 {
            o.truncate(true);
        }
    }
    if fl & F_SYNC != 0 {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "linux")]
        o.custom_flags(0o4010000);
        #[cfg(target_os = "macos")]
        o.custom_flags(0x80);
    }
    let f = o.open(path)?;
    if f.metadata().map(|m| m.is_dir()).unwrap_or(false) && !write {
        return Err(std::io::Error::from_raw_os_error(21));
    }
    Ok(f)
}

/// write_file(path, iodata, flags) — flags as for open (write implied).
#[no_mangle]
pub extern "C" fn tn_fio_write_file(c: *mut Ctx, p: Term, data: Term, flags: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    let mut bytes = Vec::new();
    if !flatten_io(data, &mut bytes, false) {
        return err_atom(ctx, "badarg");
    }
    let fl = (small_val(flags) | F_WRITE) & !F_READ;
    match open_opts(&path, fl).and_then(|mut f| f.write_all(&bytes)) {
        Ok(()) => atom(a::OK),
        Err(e) => err(ctx, &e),
    }
}

// ============================================================== open files

struct Handle {
    f: std::fs::File,
    buf: Vec<u8>,
    pos: usize,
}

impl Handle {
    fn unread(&self) -> usize {
        self.buf.len() - self.pos
    }
    /// Drop buffered read-ahead, moving the OS cursor back to the logical position.
    fn sync_cursor(&mut self) -> std::io::Result<()> {
        let n = self.unread();
        self.buf.clear();
        self.pos = 0;
        if n > 0 {
            self.f.seek(SeekFrom::Current(-(n as i64)))?;
        }
        Ok(())
    }
    /// Ensure at least `want` unread bytes are buffered (fewer at EOF).
    fn fill(&mut self, want: usize) -> std::io::Result<()> {
        if self.unread() >= want {
            return Ok(());
        }
        if self.pos > 0 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        let chunk = want.max(64 * 1024);
        while self.buf.len() < want {
            let old = self.buf.len();
            self.buf.resize(old + chunk, 0);
            let n = match self.f.read(&mut self.buf[old..]) {
                Ok(n) => n,
                Err(e) => {
                    self.buf.truncate(old);
                    return Err(e);
                }
            };
            self.buf.truncate(old + n);
            if n == 0 {
                break;
            }
        }
        Ok(())
    }
}

struct Files {
    next: u64,
    map: HashMap<u64, Handle>,
}

fn files() -> &'static Mutex<Files> {
    static F: OnceLock<Mutex<Files>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(Files { next: 1, map: HashMap::new() }))
}

#[no_mangle]
pub extern "C" fn tn_fio_open(c: *mut Ctx, p: Term, flags: Term) -> Term {
    let ctx = cx(c);
    let path = match path_arg(p) {
        Some(p) => p,
        None => return err_atom(ctx, "einval"),
    };
    match open_opts(&path, small_val(flags)) {
        Ok(f) => {
            let mut fs = files().lock();
            let id = fs.next;
            fs.next += 1;
            fs.map.insert(id, Handle { f, buf: Vec::new(), pos: 0 });
            drop(fs);
            ok_tuple(ctx, small(id as i64))
        }
        Err(e) => err(ctx, &e),
    }
}

macro_rules! with_handle {
    ($ctx:expr, $id:expr, $h:ident, $body:block) => {{
        let mut fs = files().lock();
        match fs.map.get_mut(&(small_val($id) as u64)) {
            None => {
                drop(fs);
                return err_atom($ctx, "einval");
            }
            Some($h) => $body,
        }
    }};
}

#[no_mangle]
pub extern "C" fn tn_fio_close(c: *mut Ctx, id: Term) -> Term {
    let ctx = cx(c);
    let h = files().lock().map.remove(&(small_val(id) as u64));
    match h {
        Some(h) => {
            drop(h);
            atom(a::OK)
        }
        None => err_atom(ctx, "einval"),
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 6 {
        2
    } else if b >> 4 == 14 {
        3
    } else if b >> 3 == 30 {
        4
    } else {
        1
    }
}

/// read(id, n, chars): n bytes, or n UTF-8 characters when chars == true.
#[no_mangle]
pub extern "C" fn tn_fio_read(c: *mut Ctx, id: Term, n: Term, chars: Term) -> Term {
    let ctx = cx(c);
    if !is_small(n) || small_val(n) < 0 {
        return ctx.badarg_args(&[(2, "not a non-negative integer")]);
    }
    let n = small_val(n) as usize;
    let res: std::io::Result<Option<Vec<u8>>> = with_handle!(ctx, id, h, {
        (|| {
            if chars == TRUE {
                let mut taken = 0;
                let mut cnt = 0;
                while cnt < n {
                    h.fill(taken + 4)?;
                    if taken >= h.unread() {
                        break;
                    }
                    let l = utf8_len(h.buf[h.pos + taken]);
                    taken = (taken + l).min(h.unread());
                    cnt += 1;
                }
                if taken == 0 && n > 0 {
                    return Ok(None);
                }
                let v = h.buf[h.pos..h.pos + taken].to_vec();
                h.pos += taken;
                Ok(Some(v))
            } else {
                if n == 0 {
                    return Ok(Some(Vec::new()));
                }
                h.fill(n)?;
                let k = n.min(h.unread());
                if k == 0 {
                    return Ok(None);
                }
                let v = h.buf[h.pos..h.pos + k].to_vec();
                h.pos += k;
                Ok(Some(v))
            }
        })()
    });
    match res {
        Ok(Some(v)) => {
            let b = ctx.binary(&v);
            ok_tuple(ctx, b)
        }
        Ok(None) => atom(a::EOF),
        Err(e) => err(ctx, &e),
    }
}

/// read_line(id) → {:ok, line} | :eof. crlf = true converts a trailing "\r\n" to "\n".
#[no_mangle]
pub extern "C" fn tn_fio_read_line(c: *mut Ctx, id: Term, crlf: Term) -> Term {
    let ctx = cx(c);
    let res: std::io::Result<Option<Vec<u8>>> = with_handle!(ctx, id, h, {
        (|| {
            let mut scanned = 0;
            loop {
                if let Some(i) = h.buf[h.pos + scanned..].iter().position(|&b| b == b'\n') {
                    let end = h.pos + scanned + i + 1;
                    let mut v = h.buf[h.pos..end].to_vec();
                    h.pos = end;
                    if crlf == TRUE && v.len() >= 2 && v[v.len() - 2] == b'\r' {
                        v.remove(v.len() - 2);
                    }
                    return Ok(Some(v));
                }
                scanned = h.unread();
                let before = h.unread();
                h.fill(before + 1)?;
                if h.unread() == before {
                    if before == 0 {
                        return Ok(None);
                    }
                    let v = h.buf[h.pos..].to_vec();
                    h.pos = h.buf.len();
                    return Ok(Some(v));
                }
            }
        })()
    });
    match res {
        Ok(Some(v)) => {
            let b = ctx.binary(&v);
            ok_tuple(ctx, b)
        }
        Ok(None) => atom(a::EOF),
        Err(e) => err(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_fio_write(c: *mut Ctx, id: Term, data: Term) -> Term {
    let ctx = cx(c);
    let mut bytes = Vec::new();
    if !flatten_io(data, &mut bytes, false) {
        return ctx.badarg_args(&[(2, "not iodata")]);
    }
    let res: std::io::Result<()> = with_handle!(ctx, id, h, {
        h.sync_cursor().and_then(|_| h.f.write_all(&bytes))
    });
    match res {
        Ok(()) => atom(a::OK),
        Err(e) => err(ctx, &e),
    }
}

/// position(id, whence, offset) whence: 0 bof, 1 cur, 2 eof
#[no_mangle]
pub extern "C" fn tn_fio_position(c: *mut Ctx, id: Term, whence: Term, off: Term) -> Term {
    let ctx = cx(c);
    if !is_small(off) {
        return err_atom(ctx, "einval");
    }
    let o = small_val(off);
    let res: std::io::Result<u64> = with_handle!(ctx, id, h, {
        (|| {
            h.sync_cursor()?;
            let sf = match small_val(whence) {
                0 => {
                    if o < 0 {
                        return Err(std::io::Error::from_raw_os_error(22));
                    }
                    SeekFrom::Start(o as u64)
                }
                1 => SeekFrom::Current(o),
                _ => SeekFrom::End(o),
            };
            h.f.seek(sf)
        })()
    });
    match res {
        Ok(p) => ok_tuple(ctx, small(p as i64)),
        Err(e) => err(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_fio_pread(c: *mut Ctx, id: Term, pos: Term, n: Term) -> Term {
    let ctx = cx(c);
    if !is_small(pos) || !is_small(n) || small_val(pos) < 0 || small_val(n) < 0 {
        return err_atom(ctx, "einval");
    }
    let (p, n) = (small_val(pos) as u64, small_val(n) as usize);
    let res: std::io::Result<Vec<u8>> = with_handle!(ctx, id, h, {
        (|| {
            let mut v = vec![0u8; n];
            let mut got = 0;
            while got < n {
                let k = h.f.read_at(&mut v[got..], p + got as u64)?;
                if k == 0 {
                    break;
                }
                got += k;
            }
            v.truncate(got);
            Ok(v)
        })()
    });
    match res {
        Ok(v) if v.is_empty() && n > 0 => atom(a::EOF),
        Ok(v) => {
            let b = ctx.binary(&v);
            ok_tuple(ctx, b)
        }
        Err(e) => err(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_fio_pwrite(c: *mut Ctx, id: Term, pos: Term, data: Term) -> Term {
    let ctx = cx(c);
    if !is_small(pos) || small_val(pos) < 0 {
        return err_atom(ctx, "einval");
    }
    let mut bytes = Vec::new();
    if !flatten_io(data, &mut bytes, false) {
        return ctx.badarg_args(&[(3, "not iodata")]);
    }
    let p = small_val(pos) as u64;
    let res: std::io::Result<()> = with_handle!(ctx, id, h, {
        (|| {
            h.sync_cursor()?;
            h.f.write_all_at(&bytes, p)
        })()
    });
    match res {
        Ok(()) => atom(a::OK),
        Err(e) => err(ctx, &e),
    }
}

/// op: 0 sync, 1 datasync, 2 truncate
#[no_mangle]
pub extern "C" fn tn_fio_ctl(c: *mut Ctx, id: Term, op: Term) -> Term {
    let ctx = cx(c);
    let res: std::io::Result<()> = with_handle!(ctx, id, h, {
        match small_val(op) {
            0 => h.f.sync_all(),
            1 => h.f.sync_data(),
            _ => h.sync_cursor().and_then(|_| {
                let p = h.f.stream_position()?;
                h.f.set_len(p)
            }),
        }
    });
    match res {
        Ok(()) => atom(a::OK),
        Err(e) => err(ctx, &e),
    }
}

/// copy(src_path, dst_path, nbytes (-1 = infinity), dst_flags) → {:ok, n}
#[no_mangle]
pub extern "C" fn tn_fio_copy(c: *mut Ctx, src: Term, dst: Term, n: Term, flags: Term) -> Term {
    let ctx = cx(c);
    let (s, d) = match (path_arg(src), path_arg(dst)) {
        (Some(s), Some(d)) => (s, d),
        _ => return err_atom(ctx, "einval"),
    };
    let limit = small_val(n);
    let r = (|| -> std::io::Result<u64> {
        let fin = open_opts(&s, F_READ)?;
        let mut fout = open_opts(&d, small_val(flags) | F_WRITE)?;
        if limit < 0 {
            std::io::copy(&mut &fin, &mut fout)
        } else {
            std::io::copy(&mut (&fin).take(limit as u64), &mut fout)
        }
    })();
    match r {
        Ok(k) => ok_tuple(ctx, small(k as i64)),
        Err(e) => err(ctx, &e),
    }
}

/// Re-encode a binary: latin1 → utf8 (to_utf8 = true) or utf8 → latin1.
/// Returns nil when the conversion is impossible.
#[no_mangle]
pub extern "C" fn tn_fio_recode(c: *mut Ctx, b: Term, to_utf8: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return NIL;
    }
    let src = bin_bytes(b);
    if to_utf8 == TRUE {
        if src.is_ascii() {
            return b;
        }
        let s: String = src.iter().map(|&x| x as char).collect();
        ctx.str(&s)
    } else {
        if src.is_ascii() {
            return b;
        }
        let s = match std::str::from_utf8(src) {
            Ok(s) => s,
            Err(_) => return NIL,
        };
        let mut out = Vec::with_capacity(s.len());
        for ch in s.chars() {
            if (ch as u32) > 255 {
                return NIL;
            }
            out.push(ch as u32 as u8);
        }
        ctx.binary(&out)
    }
}

/// Local datetime → universal datetime and back (for :file time options).
#[no_mangle]
pub extern "C" fn tn_fio_posix_to_datetime(c: *mut Ctx, secs: Term, local: Term) -> Term {
    let ctx = cx(c);
    if !is_small(secs) {
        return ctx.badarg("argument error");
    }
    datetime(ctx, small_val(secs), local == TRUE)
}

#[no_mangle]
pub extern "C" fn tn_fio_datetime_to_posix(c: *mut Ctx, dt: Term, local: Term) -> Term {
    let ctx = cx(c);
    match to_posix(dt, local == TRUE) {
        Some(s) => small(s),
        None => ctx.badarg("argument error"),
    }
}

/// Read `n` bytes (chars = false) or `n` UTF-8 characters from stdin → binary | :eof
#[no_mangle]
pub extern "C" fn tn_fio_stdin_read(c: *mut Ctx, n: Term, chars: Term) -> Term {
    use std::io::BufRead;
    let ctx = cx(c);
    if !is_small(n) || small_val(n) < 0 {
        return ctx.badarg_args(&[(2, "not a non-negative integer")]);
    }
    crate::io::stdout_flush();
    let n = small_val(n) as usize;
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let mut out: Vec<u8> = Vec::new();
    let mut count = 0;
    let mut need_cont = 0usize;
    while count < n || need_cont > 0 {
        let buf = match lock.fill_buf() {
            Ok(b) => b,
            Err(_) => break,
        };
        if buf.is_empty() {
            break;
        }
        let b = buf[0];
        lock.consume(1);
        out.push(b);
        if need_cont > 0 {
            need_cont -= 1;
            continue;
        }
        count += 1;
        if chars == TRUE {
            need_cont = utf8_len(b) - 1;
        }
    }
    if out.is_empty() && n > 0 {
        return atom(a::EOF);
    }
    ctx.binary(&out)
}
