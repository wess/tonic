//! IO, files, system, math, random numbers, regular expressions and error
//! formatting.

use crate::atoms;
use crate::bif_bin::flatten_io;
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::io;
use crate::num;
use crate::term::*;
use parking_lot::Mutex;
use std::sync::OnceLock;

fn bytes_of(ctx: &mut Ctx, t: Term) -> Option<Vec<u8>> {
    let _ = ctx;
    let mut v = Vec::new();
    if flatten_io(t, &mut v, true) {
        Some(v)
    } else {
        None
    }
}

/// IO.write/puts backend. device: :stdio | :stderr | pid (treated as stdio)
#[no_mangle]
pub extern "C" fn tn_io_write(c: *mut Ctx, dev: Term, data: Term) -> Term {
    let ctx = cx(c);
    let bytes = match bytes_of(ctx, data) {
        Some(b) => b,
        None => return ctx.badarg("argument error"),
    };
    if dev == atom(atoms::intern("stderr")) {
        io::stderr_write(&bytes);
    } else {
        io::stdout_write(&bytes);
    }
    atom(a::OK)
}

#[no_mangle]
pub extern "C" fn tn_io_gets(c: *mut Ctx, prompt: Term) -> Term {
    let ctx = cx(c);
    if let Some(p) = bytes_of(ctx, prompt) {
        io::stdout_write(&p);
    }
    io::stdout_flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => atom(a::EOF),
        Ok(_) => ctx.str(&line),
        Err(_) => ctx.tuple(&[atom(a::ERROR), atom(a::BADARG)]),
    }
}

/// Reads exactly n bytes from stdin (a binary), or :eof.
#[no_mangle]
pub extern "C" fn tn_io_read_bytes(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    io::stdout_flush();
    if !is_small(n) || small_val(n) < 0 {
        return ctx.badarg("argument error");
    }
    let mut buf = vec![0u8; small_val(n) as usize];
    use std::io::Read;
    match std::io::stdin().read_exact(&mut buf) {
        Ok(()) => ctx.binary(&buf),
        Err(_) => atom(a::EOF),
    }
}

/// Flushes buffered standard output.
#[no_mangle]
pub extern "C" fn tn_io_flush(_c: *mut Ctx) -> Term {
    io::stdout_flush();
    atom(a::OK)
}

#[no_mangle]
pub extern "C" fn tn_io_read_all(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    io::stdout_flush();
    let mut s = Vec::new();
    use std::io::Read;
    match std::io::stdin().read_to_end(&mut s) {
        Ok(_) => {
            if s.is_empty() {
                atom(a::EOF)
            } else {
                ctx.binary(&s)
            }
        }
        Err(_) => atom(a::EOF),
    }
}

fn path_of(t: Term) -> Option<String> {
    let mut v = Vec::new();
    if flatten_io(t, &mut v, true) {
        Some(String::from_utf8_lossy(&v).into_owned())
    } else {
        None
    }
}

fn errno_atom(e: &std::io::Error) -> Term {
    use std::io::ErrorKind::*;
    atom(match e.kind() {
        NotFound => a::ENOENT,
        PermissionDenied => a::EACCES,
        AlreadyExists => a::EEXIST,
        _ => {
            if let Some(code) = e.raw_os_error() {
                match code {
                    21 => a::EISDIR,
                    20 => a::ENOTDIR,
                    _ => atoms::intern("eio"),
                }
            } else {
                atoms::intern("eio")
            }
        }
    })
}

fn err_tuple(ctx: &mut Ctx, e: &std::io::Error) -> Term {
    let r = errno_atom(e);
    ctx.tuple(&[atom(a::ERROR), r])
}

#[no_mangle]
pub extern "C" fn tn_file_read(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let path = match path_of(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    match std::fs::read(&path) {
        Ok(b) => {
            let bin = ctx.binary(&b);
            let bi = ctx.push(bin);
            let t = ctx.tuple(&[atom(a::OK), ctx.get(bi)]);
            ctx.truncate(bi);
            t
        }
        Err(e) => err_tuple(ctx, &e),
    }
}

/// mode: 0 write, 1 append
#[no_mangle]
pub extern "C" fn tn_file_write(c: *mut Ctx, p: Term, data: Term, mode: Term) -> Term {
    let ctx = cx(c);
    let path = match path_of(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let mut bytes = Vec::new();
    if !flatten_io(data, &mut bytes, false) && !flatten_io(data, &mut bytes, true) {
        return ctx.badarg("argument error");
    }
    let r = if small_val(mode) == 1 {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .and_then(|mut f| f.write_all(&bytes))
    } else {
        std::fs::write(&path, &bytes)
    };
    match r {
        Ok(_) => atom(a::OK),
        Err(e) => err_tuple(ctx, &e),
    }
}

/// op: 0 exists?, 1 dir?, 2 regular?
#[no_mangle]
pub extern "C" fn tn_file_test(c: *mut Ctx, p: Term, op: Term) -> Term {
    let ctx = cx(c);
    let path = match path_of(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let md = std::fs::metadata(&path);
    boolean(match small_val(op) {
        0 => md.is_ok(),
        1 => md.map(|m| m.is_dir()).unwrap_or(false),
        _ => md.map(|m| m.is_file()).unwrap_or(false),
    })
}

/// op: 0 rm, 1 mkdir, 2 mkdir_p, 3 rm_rf, 4 rmdir
#[no_mangle]
pub extern "C" fn tn_file_op(c: *mut Ctx, p: Term, op: Term) -> Term {
    let ctx = cx(c);
    let path = match path_of(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let r = match small_val(op) {
        0 => std::fs::remove_file(&path),
        1 => std::fs::create_dir(&path),
        2 => std::fs::create_dir_all(&path),
        3 => {
            let md = std::fs::symlink_metadata(&path);
            match md {
                Ok(m) if m.is_dir() => std::fs::remove_dir_all(&path),
                Ok(_) => std::fs::remove_file(&path),
                Err(_) => Ok(()),
            }
        }
        _ => std::fs::remove_dir(&path),
    };
    match r {
        Ok(_) => atom(a::OK),
        Err(e) => err_tuple(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_file_ls(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let path = match path_of(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    match std::fs::read_dir(&path) {
        Ok(rd) => {
            let mut names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            let base = ctx.roots.len();
            for n in &names {
                let b = ctx.str(n);
                ctx.roots.push(b);
            }
            let l = ctx.list_from_roots(base, names.len(), NIL_LIST);
            ctx.roots.truncate(base);
            let li = ctx.push(l);
            let t = ctx.tuple(&[atom(a::OK), ctx.get(li)]);
            ctx.truncate(li);
            t
        }
        Err(e) => err_tuple(ctx, &e),
    }
}

#[no_mangle]
pub extern "C" fn tn_file_cwd(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let d = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    ctx.str(&d)
}

#[no_mangle]
pub extern "C" fn tn_file_stat_size(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let path = match path_of(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    match std::fs::metadata(&path) {
        Ok(m) => small(m.len() as i64),
        Err(_) => NIL,
    }
}

// ---------------- system ----------------

#[no_mangle]
pub extern "C" fn tn_system_argv(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let args = crate::sched::argv();
    let base = ctx.roots.len();
    for s in &args {
        let b = ctx.str(s);
        ctx.roots.push(b);
    }
    let l = ctx.list_from_roots(base, args.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

#[no_mangle]
pub extern "C" fn tn_system_get_env(c: *mut Ctx, name: Term) -> Term {
    let ctx = cx(c);
    let n = match path_of(name) {
        Some(n) => n,
        None => return ctx.badarg("argument error"),
    };
    match std::env::var(&n) {
        Ok(v) => ctx.str(&v),
        Err(_) => NIL,
    }
}

#[no_mangle]
pub extern "C" fn tn_system_put_env(c: *mut Ctx, name: Term, v: Term) -> Term {
    let ctx = cx(c);
    match (path_of(name), path_of(v)) {
        (Some(n), Some(v)) => {
            unsafe { std::env::set_var(n, v) };
            atom(a::OK)
        }
        _ => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_system_halt(_c: *mut Ctx, code: Term) -> Term {
    io::stdout_flush();
    let code = if is_small(code) { small_val(code) as i32 } else { 1 };
    std::process::exit(code);
}

fn epoch() -> &'static std::time::Instant {
    static E: OnceLock<std::time::Instant> = OnceLock::new();
    E.get_or_init(std::time::Instant::now)
}

/// Nanoseconds -> `unit` (an atom or an integer "parts per second").
fn convert_ns(ns: u128, unit: Term) -> i128 {
    if is_small(unit) && small_val(unit) > 0 {
        return (ns * small_val(unit) as u128 / 1_000_000_000) as i128;
    }
    (ns / unit_div(unit).unwrap_or(1)) as i128
}

fn unit_div(unit: Term) -> Option<u128> {
    if unit == atom(a::SECOND) || unit == atom(crate::atoms::intern("seconds")) {
        return Some(1_000_000_000);
    }
    if unit == atom(crate::atoms::intern("milli_seconds")) {
        return Some(1_000_000);
    }
    if unit == atom(crate::atoms::intern("micro_seconds")) {
        return Some(1_000);
    }
    if unit == atom(crate::atoms::intern("perf_counter")) || unit == atom(crate::atoms::intern("nano_seconds")) {
        return Some(1);
    }
    if unit == atom(a::SECOND) {
        Some(1_000_000_000)
    } else if unit == atom(a::MILLISECOND) {
        Some(1_000_000)
    } else if unit == atom(a::MICROSECOND) {
        Some(1_000)
    } else if unit == atom(a::NANOSECOND) || unit == atom(a::NATIVE) {
        Some(1)
    } else {
        None
    }
}

#[no_mangle]
pub extern "C" fn tn_monotonic_time(c: *mut Ctx, unit: Term) -> Term {
    let ctx = cx(c);
    let ns = epoch().elapsed().as_nanos();
    num::int_from_i128(ctx, convert_ns(ns, unit))
}

#[no_mangle]
pub extern "C" fn tn_system_time(c: *mut Ctx, unit: Term) -> Term {
    let ctx = cx(c);
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    num::int_from_i128(ctx, convert_ns(ns, unit))
}

#[no_mangle]
pub extern "C" fn tn_schedulers(_c: *mut Ctx) -> Term {
    small(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1) as i64,
    )
}

// ---------------- math ----------------

#[no_mangle]
pub extern "C" fn tn_math1(c: *mut Ctx, op: Term, x: Term) -> Term {
    let ctx = cx(c);
    if !is_number(x) {
        return ctx.badarith();
    }
    let v = num::num_to_f64(x);
    let r = match small_val(op) {
        0 => v.sqrt(),
        1 => v.sin(),
        2 => v.cos(),
        3 => v.tan(),
        4 => v.asin(),
        5 => v.acos(),
        6 => v.atan(),
        7 => v.exp(),
        8 => v.ln(),
        9 => v.log2(),
        10 => v.log10(),
        11 => v.floor(),
        12 => v.ceil(),
        13 => v.sinh(),
        14 => v.cosh(),
        15 => v.tanh(),
        _ => f64::NAN,
    };
    if !r.is_finite() {
        return ctx.badarith();
    }
    ctx.float(r)
}

#[no_mangle]
pub extern "C" fn tn_math2(c: *mut Ctx, op: Term, x: Term, y: Term) -> Term {
    let ctx = cx(c);
    if !is_number(x) || !is_number(y) {
        return ctx.badarith();
    }
    let (a, b) = (num::num_to_f64(x), num::num_to_f64(y));
    let r = match small_val(op) {
        0 => a.powf(b),
        1 => a.atan2(b),
        2 => a % b,
        _ => f64::NAN,
    };
    if !r.is_finite() {
        return ctx.badarith();
    }
    ctx.float(r)
}

// ---------------- random ----------------

fn rng() -> &'static Mutex<u64> {
    static R: OnceLock<Mutex<u64>> = OnceLock::new();
    R.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        Mutex::new(seed | 1)
    })
}

fn next_u64() -> u64 {
    let mut s = rng().lock();
    // xorshift64*
    let mut x = *s;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *s = x;
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

#[no_mangle]
pub extern "C" fn tn_rand_uniform(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if !is_small(n) || small_val(n) < 1 {
        return ctx.badarg("argument error");
    }
    small((next_u64() % small_val(n) as u64) as i64 + 1)
}

#[no_mangle]
pub extern "C" fn tn_rand_float(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let f = (next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    ctx.float(f)
}

#[no_mangle]
pub extern "C" fn tn_rand_seed(_c: *mut Ctx, s: Term) -> Term {
    let v = if is_small(s) { small_val(s) as u64 } else { crate::cmp::hash(s) };
    *rng().lock() = tonic_shared::mix64(v) | 1;
    atom(a::OK)
}

// ---------------- error formatting ----------------

/// Format an uncaught exception as Elixir prints it; uses the prelude's
/// `Tonic.Internal.format_exit/2` when available.
pub fn format_exit_message(ctx: &mut Ctx, kind: Term, reason: Term) -> String {
    format_exit_message_x(ctx, kind, reason, false)
}

/// `main`: the uncaught error ends the script (adds the trailing
/// `Code.require_file/2` frame Elixir shows).
pub fn format_exit_message_x(ctx: &mut Ctx, kind: Term, reason: Term, main: bool) -> String {
    if let Some(fp) = crate::sched::lookup_fun(atom(a::TONIC_INTERNAL), atom(a::FORMAT_EXIT), 2) {
        let saved = (ctx.exc_kind, ctx.exc_reason, ctx.exc_stack);
        let stack = if ctx.exc_stack == 0 { NIL_LIST } else { ctx.exc_stack };
        let base = ctx.roots.len();
        ctx.roots.push(reason);
        let k = ctx.tuple(&[kind, stack, boolean(main)]);
        let reason = ctx.roots[base];
        ctx.roots.truncate(base);
        let args = [k, reason];
        let r = unsafe {
            crate::sched::tonic_call_fnptr(ctx, fp as *const std::ffi::c_void, 2, args.as_ptr())
        };
        if r != NONE && is_binary(r) {
            return String::from_utf8_lossy(bin_bytes(r)).into_owned();
        }
        ctx.exc_kind = saved.0;
        ctx.exc_reason = saved.1;
        ctx.exc_stack = saved.2;
    }
    format!("** ({}) {}", atoms::name(atom_idx(kind)), debug_term(reason))
}

/// Minimal term printer for diagnostics.
pub fn debug_term(t: Term) -> String {
    if is_small(t) {
        return small_val(t).to_string();
    }
    if is_atom(t) {
        return format!(":{}", atoms::name(atom_idx(t)));
    }
    if t == NIL_LIST {
        return "[]".into();
    }
    if is_pid(t) {
        return format!("#PID<0.{}.0>", pid_id(t));
    }
    if !is_ptr(t) {
        return format!("<imm {:x}>", t);
    }
    match boxed_tag(t) {
        T_FLOAT => num::float_to_string(float_val(t)),
        T_BIGINT => num::int_to_string(t, 10),
        T_BITS => format!("<<{} bits>>", bit_len(t)),
        T_BINARY | T_SUBBIN => format!("{:?}", String::from_utf8_lossy(bin_bytes(t))),
        T_TUPLE => {
            let parts: Vec<String> = (0..tuple_size(t)).map(|i| debug_term(tuple_get(t, i))).collect();
            format!("{{{}}}", parts.join(", "))
        }
        T_CONS => {
            let mut parts = Vec::new();
            let mut x = t;
            while is_cons(x) {
                parts.push(debug_term(head(x)));
                x = tail(x);
            }
            if x != NIL_LIST {
                format!("[{} | {}]", parts.join(", "), debug_term(x))
            } else {
                format!("[{}]", parts.join(", "))
            }
        }
        T_MAP => {
            let parts: Vec<String> = crate::map::entries(t)
                .iter()
                .map(|(k, v)| format!("{} => {}", debug_term(*k), debug_term(*v)))
                .collect();
            format!("%{{{}}}", parts.join(", "))
        }
        T_CLOSURE => "#Function<>".into(),
        T_REF => "#Reference<>".into(),
        _ => "<?>".into(),
    }
}

#[no_mangle]
pub extern "C" fn tn_debug_print(c: *mut Ctx, t: Term) -> Term {
    let _ = c;
    eprintln!("{}", debug_term(t));
    t
}

/// Identifier class of a codepoint for String.Tokenizer:
/// 0 none, 1 upper (ID_Start and uppercase/titlecase), 2 ID_Start, 3 ID_Continue.
#[no_mangle]
pub extern "C" fn tn_ident_class(_c: *mut Ctx, cp: Term) -> Term {
    if !is_small(cp) {
        return small(0);
    }
    let Some(ch) = char::from_u32(small_val(cp) as u32) else { return small(0) };
    if unicode_ident::is_xid_start(ch) {
        if ch.is_uppercase() || (ch.to_lowercase().next() != Some(ch) && !ch.is_lowercase()) {
            return small(1);
        }
        return small(2);
    }
    if unicode_ident::is_xid_continue(ch) {
        return small(3);
    }
    small(0)
}
