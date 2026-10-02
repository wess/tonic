//! More runtime natives: checksums and hashes, external term format,
//! float formatting options, local time, system info.

use crate::atoms;
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::map;
use crate::num;
use crate::term::*;

// ---------------------------------------------------------------- system

#[no_mangle]
pub extern "C" fn tn_atom_count(_c: *mut Ctx) -> Term {
    small(atoms::count() as i64)
}

#[no_mangle]
pub extern "C" fn tn_system_architecture(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    let os = if cfg!(target_os = "macos") { "apple-darwin" } else { "pc-linux-gnu" };
    let s = format!("{}-{}", arch, os);
    let mut v: Vec<Term> = s.bytes().map(|b| small(b as i64)).collect();
    ctx.list_from_vec(&mut v)
}

// ---------------------------------------------------------------- checksums

fn crc32_table() -> &'static [u32; 256] {
    static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    })
}

#[no_mangle]
pub extern "C" fn tn_crc32(c: *mut Ctx, crc: Term, b: Term) -> Term {
    let ctx = cx(c);
    if !is_small(crc) || !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let t = crc32_table();
    let mut x = !(small_val(crc) as u32);
    for &byte in bin_bytes(b) {
        x = t[((x ^ byte as u32) & 0xff) as usize] ^ (x >> 8);
    }
    small((!x) as i64)
}

#[no_mangle]
pub extern "C" fn tn_adler32(c: *mut Ctx, a: Term, b: Term) -> Term {
    let ctx = cx(c);
    if !is_small(a) || !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let v = small_val(a) as u32;
    let (mut s1, mut s2) = (v & 0xffff, v >> 16);
    for &byte in bin_bytes(b) {
        s1 = (s1 + byte as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    small(((s2 << 16) | s1) as i64)
}

fn digest(alg: &str, data: &[u8]) -> Option<Vec<u8>> {
    use sha1::Digest;
    Some(match alg {
        "md5" => md5::Md5::digest(data).to_vec(),
        "sha" => sha1::Sha1::digest(data).to_vec(),
        "sha224" => sha2::Sha224::digest(data).to_vec(),
        "sha256" => sha2::Sha256::digest(data).to_vec(),
        "sha384" => sha2::Sha384::digest(data).to_vec(),
        "sha512" => sha2::Sha512::digest(data).to_vec(),
        _ => return None,
    })
}

#[no_mangle]
pub extern "C" fn tn_md5(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let d = digest("md5", bin_bytes(b)).unwrap();
    ctx.binary(&d)
}

/// :crypto.hash(alg, data) with `data` already flattened to a binary.
#[no_mangle]
pub extern "C" fn tn_crypto_hash(c: *mut Ctx, alg: Term, b: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(alg) || !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b).to_vec();
    match digest(atoms::name(atom_idx(alg)), &bytes) {
        Some(d) => ctx.binary(&d),
        None => ctx.badarg_args(&[(1, "Bad digest type")]),
    }
}

/// HMAC with the given hash.
#[no_mangle]
pub extern "C" fn tn_crypto_mac(c: *mut Ctx, alg: Term, key: Term, b: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(alg) || !is_binary(key) || !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let name = atoms::name(atom_idx(alg));
    let block = match name {
        "sha384" | "sha512" => 128,
        _ => 64,
    };
    let mut k = bin_bytes(key).to_vec();
    if k.len() > block {
        k = match digest(name, &k) {
            Some(d) => d,
            None => return ctx.badarg("argument error"),
        };
    }
    k.resize(block, 0);
    let mut inner: Vec<u8> = k.iter().map(|x| x ^ 0x36).collect();
    inner.extend_from_slice(bin_bytes(b));
    let ih = match digest(name, &inner) {
        Some(d) => d,
        None => return ctx.badarg("argument error"),
    };
    let mut outer: Vec<u8> = k.iter().map(|x| x ^ 0x5c).collect();
    outer.extend_from_slice(&ih);
    let d = digest(name, &outer).unwrap();
    ctx.binary(&d)
}

#[no_mangle]
pub extern "C" fn tn_strong_rand_bytes(c: *mut Ctx, n: Term) -> Term {
    let ctx = cx(c);
    if !is_small(n) || small_val(n) < 0 {
        return ctx.badarg("argument error");
    }
    let n = small_val(n) as usize;
    let mut buf = vec![0u8; n];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .is_ok();
    if !ok {
        for b in buf.iter_mut() {
            *b = (crate::sched::next_ref_id() as u8).wrapping_mul(31);
        }
    }
    ctx.binary(&buf)
}

// ---------------------------------------------------------------- floats

/// :erlang.float_to_binary(f, opts) for [decimals: n, compact], [scientific: n], [:short].
#[no_mangle]
pub extern "C" fn tn_float_to_binary_opts(c: *mut Ctx, f: Term, opts: Term) -> Term {
    let ctx = cx(c);
    if !is_float(f) {
        return ctx.badarg_args(&[(1, "not a float")]);
    }
    let x = float_val(f);
    let mut decimals: Option<usize> = None;
    let mut scientific: Option<usize> = None;
    let mut compact = false;
    let mut short = false;
    let mut l = opts;
    while is_cons(l) {
        let o = head(l);
        if is_tuple(o) && tuple_size(o) == 2 && is_atom(tuple_get(o, 0)) && is_small(tuple_get(o, 1)) {
            let n = small_val(tuple_get(o, 1)).max(0) as usize;
            match atoms::name(atom_idx(tuple_get(o, 0))) {
                "decimals" => decimals = Some(n.min(253)),
                "scientific" => scientific = Some(n.min(249)),
                _ => return ctx.badarg_args(&[(2, "invalid option in list")]),
            }
        } else if o == atom(atoms::intern("compact")) {
            compact = true;
        } else if o == atom(atoms::intern("short")) {
            short = true;
        } else {
            return ctx.badarg_args(&[(2, "invalid option in list")]);
        }
        l = tail(l);
    }
    let s = if short {
        num::float_to_string(x)
    } else if let Some(d) = decimals {
        let mut s = format!("{:.*}", d, x);
        if compact && s.contains('.') {
            while s.ends_with('0') && !s.ends_with(".0") {
                s.pop();
            }
        }
        s
    } else {
        let d = scientific.unwrap_or(20);
        erl_scientific(x, d)
    };
    ctx.str(&s)
}

fn erl_scientific(x: f64, d: usize) -> String {
    let s = format!("{:.*e}", d, x);
    // Rust: 1.5e2 / 1.5e-2 ; Erlang: 1.5e+02 / 1.5e-02
    match s.split_once('e') {
        Some((m, e)) => {
            let (sign, digits) = if let Some(r) = e.strip_prefix('-') { ('-', r) } else { ('+', e) };
            format!("{}e{}{:0>2}", m, sign, digits)
        }
        None => s,
    }
}

// ---------------------------------------------------------------- time

#[repr(C)]
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
}

/// {{y, m, d}, {h, mi, s}} in local time.
#[no_mangle]
pub extern "C" fn tn_localtime(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut tm = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, _pad: [0; 4] };
    unsafe { localtime_r(&now, &mut tm) };
    let d = ctx.tuple(&[small(tm.year as i64 + 1900), small(tm.mon as i64 + 1), small(tm.mday as i64)]);
    let base = ctx.roots.len();
    ctx.roots.push(d);
    let t = ctx.tuple(&[small(tm.hour as i64), small(tm.min as i64), small(tm.sec as i64)]);
    let d = ctx.roots[base];
    ctx.roots.truncate(base);
    ctx.tuple(&[d, t])
}

/// Seconds east of UTC for the local timezone right now.
#[no_mangle]
pub extern "C" fn tn_utc_offset(_c: *mut Ctx) -> Term {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut tm = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, _pad: [0; 4] };
    unsafe { localtime_r(&now, &mut tm) };
    // tm_gmtoff follows the 9 ints (+4 bytes padding) on both macOS and glibc.
    small(tm._pad[0] as i64)
}

// ---------------------------------------------------------------- external term format

const VERSION: u8 = 131;

fn etf_encode(t: Term, out: &mut Vec<u8>) -> Result<(), ()> {
    if is_small(t) {
        let v = small_val(t);
        if (0..=255).contains(&v) {
            out.push(97);
            out.push(v as u8);
        } else if (i32::MIN as i64..=i32::MAX as i64).contains(&v) {
            out.push(98);
            out.extend_from_slice(&(v as i32).to_be_bytes());
        } else {
            let mut mag = v.unsigned_abs();
            let mut digits = Vec::new();
            while mag > 0 {
                digits.push(mag as u8);
                mag >>= 8;
            }
            out.push(110);
            out.push(digits.len() as u8);
            out.push(if v < 0 { 1 } else { 0 });
            out.extend_from_slice(&digits);
        }
        return Ok(());
    }
    if is_atom(t) {
        let name = atoms::name(atom_idx(t)).as_bytes();
        if name.len() < 256 {
            out.push(119);
            out.push(name.len() as u8);
        } else {
            out.push(118);
            out.extend_from_slice(&(name.len() as u16).to_be_bytes());
        }
        out.extend_from_slice(name);
        return Ok(());
    }
    if t == NIL_LIST {
        out.push(106);
        return Ok(());
    }
    if is_pid(t) {
        out.push(88);
        etf_encode(atom(atoms::intern("nonode@nohost")), out)?;
        out.extend_from_slice(&(pid_id(t) as u32).to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        return Ok(());
    }
    if !is_ptr(t) {
        return Err(());
    }
    match boxed_tag(t) {
        T_FLOAT => {
            out.push(70);
            out.extend_from_slice(&float_val(t).to_bits().to_be_bytes());
        }
        T_BIGINT => {
            let b = num::to_big(t);
            let (sign, bytes) = b.to_bytes_le();
            if bytes.len() < 256 {
                out.push(110);
                out.push(bytes.len() as u8);
            } else {
                out.push(111);
                out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            }
            out.push(if sign == num_bigint::Sign::Minus { 1 } else { 0 });
            out.extend_from_slice(&bytes);
        }
        T_BITS => {
            let n = bit_len(t);
            let b = bit_bytes(t);
            out.push(77);
            out.extend_from_slice(&(b.len() as u32).to_be_bytes());
            out.push((n % 8) as u8);
            out.extend_from_slice(b);
        }
        T_BINARY | T_SUBBIN => {
            let b = bin_bytes(t);
            out.push(109);
            out.extend_from_slice(&(b.len() as u32).to_be_bytes());
            out.extend_from_slice(b);
        }
        T_TUPLE => {
            let n = tuple_size(t);
            if n < 256 {
                out.push(104);
                out.push(n as u8);
            } else {
                out.push(105);
                out.extend_from_slice(&(n as u32).to_be_bytes());
            }
            for i in 0..n {
                etf_encode(tuple_get(t, i), out)?;
            }
        }
        T_CONS => {
            // Byte lists use STRING_EXT.
            let mut n = 0usize;
            let mut x = t;
            let mut bytes = true;
            while is_cons(x) {
                let h = head(x);
                if !(is_small(h) && (0..=255).contains(&small_val(h))) {
                    bytes = false;
                }
                n += 1;
                x = tail(x);
            }
            if bytes && x == NIL_LIST && n < 65536 {
                out.push(107);
                out.extend_from_slice(&(n as u16).to_be_bytes());
                let mut x = t;
                while is_cons(x) {
                    out.push(small_val(head(x)) as u8);
                    x = tail(x);
                }
            } else {
                out.push(108);
                out.extend_from_slice(&(n as u32).to_be_bytes());
                let mut x = t;
                while is_cons(x) {
                    etf_encode(head(x), out)?;
                    x = tail(x);
                }
                etf_encode(x, out)?;
            }
        }
        T_MAP => {
            let es = map::entries_beam(t);
            out.push(116);
            out.extend_from_slice(&(es.len() as u32).to_be_bytes());
            for (k, v) in es {
                etf_encode(k, out)?;
                etf_encode(v, out)?;
            }
        }
        T_REF => {
            let id = word(t, 1);
            out.push(90);
            out.extend_from_slice(&3u16.to_be_bytes());
            etf_encode(atom(atoms::intern("nonode@nohost")), out)?;
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(id as u32).to_be_bytes());
            out.extend_from_slice(&((id >> 32) as u32).to_be_bytes());
            out.extend_from_slice(&0u32.to_be_bytes());
        }
        _ => return Err(()),
    }
    Ok(())
}

#[no_mangle]
pub extern "C" fn tn_term_to_binary(c: *mut Ctx, t: Term) -> Term {
    let ctx = cx(c);
    let mut out = vec![VERSION];
    if etf_encode(t, &mut out).is_err() {
        return ctx.badarg("argument error");
    }
    ctx.binary(&out)
}

/// Decoded term tree (built into the heap afterwards, GC-safely).
enum D {
    Int(i64),
    Big(num_bigint::BigInt),
    Float(f64),
    Atom(u64),
    Nil,
    Bin(Vec<u8>),
    Bits(Vec<u8>, usize),
    Tuple(Vec<D>),
    List(Vec<D>, Box<D>),
    Map(Vec<(D, D)>),
    Pid(u64),
    Ref(u64),
}

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.i + n > self.b.len() {
            return None;
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Some(s)
    }
    fn u16(&mut self) -> Option<usize> {
        let s = self.take(2)?;
        Some(u16::from_be_bytes([s[0], s[1]]) as usize)
    }
    fn u32(&mut self) -> Option<usize> {
        let s = self.take(4)?;
        Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize)
    }
    fn atom_name(&mut self) -> Option<String> {
        let tag = self.u8()?;
        let len = match tag {
            119 | 115 => self.u8()? as usize,
            118 | 100 => self.u16()?,
            _ => return None,
        };
        let s = self.take(len)?;
        Some(String::from_utf8_lossy(s).into_owned())
    }
    fn term(&mut self) -> Option<D> {
        let tag = self.u8()?;
        Some(match tag {
            97 => D::Int(self.u8()? as i64),
            98 => D::Int(self.u32()? as u32 as i32 as i64),
            70 => {
                let s = self.take(8)?;
                D::Float(f64::from_bits(u64::from_be_bytes(s.try_into().ok()?)))
            }
            110 | 111 => {
                let n = if tag == 110 { self.u8()? as usize } else { self.u32()? };
                let sign = self.u8()?;
                let digits = self.take(n)?;
                let mag = num_bigint::BigInt::from_bytes_le(num_bigint::Sign::Plus, digits);
                D::Big(if sign == 1 { -mag } else { mag })
            }
            119 | 115 | 118 | 100 => {
                self.i -= 1;
                let name = self.atom_name()?;
                D::Atom(atoms::intern(&name))
            }
            106 => D::Nil,
            107 => {
                let n = self.u16()?;
                let s = self.take(n)?;
                D::List(s.iter().map(|&b| D::Int(b as i64)).collect(), Box::new(D::Nil))
            }
            108 => {
                let n = self.u32()?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.term()?);
                }
                let tl = self.term()?;
                D::List(v, Box::new(tl))
            }
            104 | 105 => {
                let n = if tag == 104 { self.u8()? as usize } else { self.u32()? };
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.term()?);
                }
                D::Tuple(v)
            }
            109 => {
                let n = self.u32()?;
                D::Bin(self.take(n)?.to_vec())
            }
            77 => {
                let n = self.u32()?;
                let tail = self.u8()? as usize;
                let data = self.take(n)?.to_vec();
                let bits = if tail == 0 { n * 8 } else { (n - 1) * 8 + tail };
                D::Bits(data, bits)
            }
            116 => {
                let n = self.u32()?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = self.term()?;
                    let val = self.term()?;
                    v.push((k, val));
                }
                D::Map(v)
            }
            88 | 103 => {
                self.atom_name()?;
                let id = self.u32()?;
                self.u32()?;
                if tag == 88 {
                    self.u32()?;
                } else {
                    self.u8()?;
                }
                D::Pid(id as u64)
            }
            90 => {
                let n = self.u16()?;
                self.atom_name()?;
                self.u32()?;
                let mut ids = Vec::new();
                for _ in 0..n {
                    ids.push(self.u32()? as u64);
                }
                let id = ids.first().copied().unwrap_or(0) | (ids.get(1).copied().unwrap_or(0) << 32);
                D::Ref(id)
            }
            _ => return None,
        })
    }
}

fn build(ctx: &mut Ctx, d: &D) -> Term {
    match d {
        D::Int(i) => num::int_from_i128(ctx, *i as i128),
        D::Big(b) => num::from_big(ctx, b),
        D::Float(f) => ctx.float(*f),
        D::Atom(a) => atom(*a),
        D::Nil => NIL_LIST,
        D::Bin(b) => ctx.binary(b),
        D::Bits(b, n) => ctx.bitstring(b, *n),
        D::Pid(p) => pid(*p),
        D::Ref(id) => {
            let p = ctx.alloc(2);
            unsafe {
                *p = header(T_REF, 1);
                *p.add(1) = *id;
            }
            p as Term
        }
        D::Tuple(v) => {
            let base = ctx.roots.len();
            for e in v {
                let t = build(ctx, e);
                ctx.roots.push(t);
            }
            let elems: Vec<Term> = ctx.roots[base..].to_vec();
            let t = ctx.tuple(&elems);
            ctx.roots.truncate(base);
            t
        }
        D::List(v, tl) => {
            let base = ctx.roots.len();
            for e in v {
                let t = build(ctx, e);
                ctx.roots.push(t);
            }
            let tail_t = build(ctx, tl);
            let l = ctx.list_from_roots(base, v.len(), tail_t);
            ctx.roots.truncate(base);
            l
        }
        D::Map(kvs) => {
            let base = ctx.roots.len();
            for (k, v) in kvs {
                let kt = build(ctx, k);
                ctx.roots.push(kt);
                let vt = build(ctx, v);
                ctx.roots.push(vt);
            }
            let m = map::from_root_pairs(ctx, base, kvs.len());
            ctx.roots.truncate(base);
            m
        }
    }
}

#[no_mangle]
pub extern "C" fn tn_binary_to_term(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    let bytes = bin_bytes(b).to_vec();
    let mut r = Rd { b: &bytes, i: 0 };
    if r.u8() != Some(VERSION) {
        return ctx.badarg_args(&[(1, "invalid external representation of a term")]);
    }
    match r.term() {
        Some(d) => build(ctx, &d),
        None => ctx.badarg_args(&[(1, "invalid external representation of a term")]),
    }
}

// ---------------------------------------------------------------- Erlang term printing (io_lib)

const ERL_RESERVED: &[&str] = &[
    "after", "and", "andalso", "band", "begin", "bnot", "bor", "bsl", "bsr", "bxor", "case", "catch", "cond", "div",
    "end", "fun", "if", "let", "maybe", "not", "of", "or", "orelse", "receive", "rem", "try", "when", "xor", "else",
];

pub fn erl_atom(s: &str) -> String {
    let b = s.as_bytes();
    let plain = !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'@')
        && !ERL_RESERVED.contains(&s);
    if plain {
        s.to_string()
    } else {
        let mut o = String::from("'");
        for ch in s.chars() {
            match ch {
                '\'' => o.push_str("\\'"),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\t' => o.push_str("\\t"),
                c => o.push(c),
            }
        }
        o.push('\'');
        o
    }
}

fn erl_printable_list(t: Term) -> Option<String> {
    // Printable Unicode string (io_lib:printable_list with unicode range).
    let mut s = String::new();
    let mut x = t;
    if !is_cons(x) {
        return None;
    }
    while is_cons(x) {
        let h = head(x);
        if !is_small(h) {
            return None;
        }
        let v = small_val(h);
        let ch = char::from_u32(v as u32)?;
        if !(v >= 32 && v != 127 || matches!(v, 8 | 9 | 10 | 11 | 12 | 13 | 27)) || (v >= 0x80 && v < 0xA0) {
            return None;
        }
        s.push(ch);
        x = tail(x);
    }
    if x != NIL_LIST {
        return None;
    }
    Some(s)
}

fn erl_escape_str(s: &str, q: char) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            '\x0b' => o.push_str("\\v"),
            '\x0c' => o.push_str("\\f"),
            '\x08' => o.push_str("\\b"),
            '\x1b' => o.push_str("\\e"),
            '\\' => o.push_str("\\\\"),
            c if c == q => {
                o.push('\\');
                o.push(c);
            }
            c => o.push(c),
        }
    }
    o
}

/// io_lib:write (~w) and a flat version of io_lib:print (~p).
pub fn erl_write(t: Term, pretty: bool, out: &mut String) {
    if is_small(t) || is_bigint(t) {
        out.push_str(&num::int_to_string(t, 10));
        return;
    }
    if is_atom(t) {
        out.push_str(&erl_atom(atoms::name(atom_idx(t))));
        return;
    }
    if t == NIL_LIST {
        out.push_str("[]");
        return;
    }
    if is_pid(t) {
        out.push_str(&format!("<0.{}.0>", pid_id(t)));
        return;
    }
    if !is_ptr(t) {
        out.push_str("?");
        return;
    }
    match boxed_tag(t) {
        T_FLOAT => out.push_str(&num::float_to_string(float_val(t))),
        T_BINARY | T_SUBBIN | T_BITS => {
            let bytes = bit_bytes(t);
            let nbits = bit_len(t);
            let full = &bytes[..nbits / 8];
            let printable = pretty
                && nbits % 8 == 0
                && !full.is_empty()
                && std::str::from_utf8(full).map(|s| s.chars().all(|c| (c as u32) >= 32 && c != '\x7f' || "\n\t\r\x0b\x0c\x08\x1b".contains(c))).unwrap_or(false);
            if printable {
                let st = std::str::from_utf8(full).unwrap();
                out.push_str("<<\"");
                out.push_str(&erl_escape_str(st, '"'));
                out.push_str(if st.is_ascii() { "\">>" } else { "\"/utf8>>" });
                return;
            }
            out.push_str("<<");
            let mut first = true;
            for &b in full {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&b.to_string());
            }
            if nbits % 8 != 0 {
                if !first {
                    out.push(',');
                }
                let r = nbits % 8;
                let v = bytes[nbits / 8] >> (8 - r);
                out.push_str(&format!("{}:{}", v, r));
            }
            out.push_str(">>");
        }
        T_TUPLE => {
            out.push('{');
            for i in 0..tuple_size(t) {
                if i > 0 {
                    out.push(',');
                }
                erl_write(tuple_get(t, i), pretty, out);
            }
            out.push('}');
        }
        T_CONS => {
            if pretty {
                if let Some(s) = erl_printable_list(t) {
                    out.push('"');
                    out.push_str(&erl_escape_str(&s, '"'));
                    out.push('"');
                    return;
                }
            }
            out.push('[');
            let mut x = t;
            let mut first = true;
            while is_cons(x) {
                if !first {
                    out.push(',');
                }
                first = false;
                erl_write(head(x), pretty, out);
                x = tail(x);
            }
            if x != NIL_LIST {
                out.push('|');
                erl_write(x, pretty, out);
            }
            out.push(']');
        }
        T_MAP => {
            out.push_str("#{");
            let mut first = true;
            for (k, v) in map::entries_beam(t) {
                if !first {
                    out.push(',');
                }
                first = false;
                erl_write(k, pretty, out);
                out.push_str(" => ");
                erl_write(v, pretty, out);
            }
            out.push('}');
        }
        T_REF => out.push_str(&format!("#Ref<0.0.0.{}>", word(t, 1))),
        T_CLOSURE => out.push_str("#Fun<erl_eval.0.0>"),
        _ => out.push('?'),
    }
}

#[no_mangle]
pub extern "C" fn tn_erl_write(c: *mut Ctx, t: Term, pretty: Term) -> Term {
    let ctx = cx(c);
    let mut s = String::new();
    erl_write(t, pretty == TRUE, &mut s);
    ctx.str(&s)
}

// ---------------------------------------------------------------- Base encodings

const B16U: &[u8] = b"0123456789ABCDEF";
const B16L: &[u8] = b"0123456789abcdef";
const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64URL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const B32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const B32HEX: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";

fn base_alphabet(kind: i64) -> &'static [u8] {
    match kind {
        0 => B16U,
        1 => B16L,
        2 => B64,
        3 => B64URL,
        4 => B32,
        _ => B32HEX,
    }
}

/// Encode with alphabet `kind` (0 b16 upper, 1 b16 lower, 2 b64, 3 b64url,
/// 4 b32, 5 hex32); `pad`; `lower` lowercases base32 output.
#[no_mangle]
pub extern "C" fn tn_base_encode(c: *mut Ctx, b: Term, kind: Term, pad: Term, lower: Term) -> Term {
    let ctx = cx(c);
    let data = bin_bytes(b).to_vec();
    let kind = small_val(kind);
    let alpha = base_alphabet(kind);
    let mut out = Vec::with_capacity(data.len() * 2);
    match kind {
        0 | 1 => {
            for &x in &data {
                out.push(alpha[(x >> 4) as usize]);
                out.push(alpha[(x & 15) as usize]);
            }
        }
        2 | 3 => {
            for ch in data.chunks(3) {
                let n = ch.len();
                let v = ((ch[0] as u32) << 16) | ((*ch.get(1).unwrap_or(&0) as u32) << 8) | *ch.get(2).unwrap_or(&0) as u32;
                let chars = [(v >> 18) & 63, (v >> 12) & 63, (v >> 6) & 63, v & 63];
                for i in 0..n + 1 {
                    out.push(alpha[chars[i] as usize]);
                }
                if pad == TRUE {
                    for _ in n + 1..4 {
                        out.push(b'=');
                    }
                }
            }
        }
        _ => {
            for ch in data.chunks(5) {
                let n = ch.len();
                let mut v: u64 = 0;
                for i in 0..5 {
                    v = (v << 8) | *ch.get(i).unwrap_or(&0) as u64;
                }
                let nchars = match n {
                    1 => 2,
                    2 => 4,
                    3 => 5,
                    4 => 7,
                    _ => 8,
                };
                for i in 0..nchars {
                    let idx = ((v >> (35 - 5 * i)) & 31) as usize;
                    let mut ch = alpha[idx];
                    if lower == TRUE {
                        ch = ch.to_ascii_lowercase();
                    }
                    out.push(ch);
                }
                if pad == TRUE {
                    for _ in nchars..8 {
                        out.push(b'=');
                    }
                }
            }
        }
    }
    ctx.binary(&out)
}

/// Decode; returns {:ok, bin} | {:bad_char, byte} | :padding | :length.
/// `casemode`: 0 upper, 1 lower, 2 mixed (base16 / base32).
#[no_mangle]
pub extern "C" fn tn_base_decode(c: *mut Ctx, b: Term, kind: Term, casemode: Term, pad: Term) -> Term {
    let ctx = cx(c);
    let s = bin_bytes(b).to_vec();
    let kind = small_val(kind);
    let casemode = small_val(casemode);
    let pad = pad == TRUE;
    let alpha = base_alphabet(kind);
    let lookup = |ch: u8| -> Option<u32> {
        let ch2 = match (kind, casemode) {
            (0, 1) | (4, 1) | (5, 1) => {
                if ch.is_ascii_uppercase() {
                    return None;
                }
                ch.to_ascii_uppercase()
            }
            (0, 2) | (4, 2) | (5, 2) => ch.to_ascii_uppercase(),
            _ => ch,
        };
        alpha.iter().position(|&a| a == ch2).map(|p| p as u32)
    };
    let err_char = |ctx: &mut Ctx, ch: u8| ctx.tuple(&[a("bad_char"), small(ch as i64)]);
    let mut out = Vec::new();
    match kind {
        0 => {
            if s.len() % 2 != 0 {
                return a("length");
            }
            for p in s.chunks(2) {
                let hi = match lookup(p[0]) { Some(v) => v, None => return err_char(ctx, p[0]) };
                let lo = match lookup(p[1]) { Some(v) => v, None => return err_char(ctx, p[1]) };
                out.push(((hi << 4) | lo) as u8);
            }
        }
        2 | 3 => {
            let group = 4;
            let rem = s.len() % group;
            if rem == 1 || (pad && rem != 0) {
                // still report a bad character that comes first
                for &ch in &s {
                    if ch != b'=' && lookup(ch).is_none() {
                        return err_char(ctx, ch);
                    }
                }
                return a("padding");
            }
            let mut i = 0;
            while i < s.len() {
                let chunk = &s[i..(i + group).min(s.len())];
                let last = i + group >= s.len();
                let npad = chunk.iter().rev().take_while(|&&x| x == b'=').count();
                if npad > 0 && (!last || chunk.len() != 4 || npad > 2) {
                    return a("padding");
                }
                let body = &chunk[..chunk.len() - npad];
                let mut v: u32 = 0;
                for &ch in body {
                    match lookup(ch) {
                        Some(x) => v = (v << 6) | x,
                        None => return err_char(ctx, ch),
                    }
                }
                let bits = body.len() * 6;
                v <<= 24 - bits;
                let nbytes = bits / 8;
                for k in 0..nbytes {
                    out.push((v >> (16 - 8 * k)) as u8);
                }
                i += group;
            }
        }
        _ => {
            let group = 8;
            let rem = s.len() % group;
            if (pad && rem != 0) || matches!(rem, 1 | 3 | 6) {
                for &ch in &s {
                    if ch != b'=' && lookup(ch).is_none() {
                        return err_char(ctx, ch);
                    }
                }
                return a("padding");
            }
            let mut i = 0;
            while i < s.len() {
                let chunk = &s[i..(i + group).min(s.len())];
                let last = i + group >= s.len();
                let npad = chunk.iter().rev().take_while(|&&x| x == b'=').count();
                if npad > 0 && (!last || chunk.len() != 8 || !matches!(npad, 1 | 3 | 4 | 6)) {
                    return a("padding");
                }
                let body = &chunk[..chunk.len() - npad];
                if matches!(body.len(), 1 | 3 | 6) {
                    return a("padding");
                }
                let mut v: u64 = 0;
                for &ch in body {
                    match lookup(ch) {
                        Some(x) => v = (v << 5) | x as u64,
                        None => return err_char(ctx, ch),
                    }
                }
                let bits = body.len() * 5;
                v <<= 40 - bits;
                let nbytes = bits / 8;
                for k in 0..nbytes {
                    out.push((v >> (32 - 8 * k)) as u8);
                }
                i += group;
            }
        }
    }
    let bin = ctx.binary(&out);
    ctx.tuple(&[a("ok"), bin])
}

fn a(name: &str) -> Term {
    atom(atoms::intern(name))
}

// ---------------------------------------------------------------- System

fn term_string(t: Term) -> Option<String> {
    if is_binary(t) {
        Some(String::from_utf8_lossy(bin_bytes(t)).into_owned())
    } else {
        None
    }
}

/// [{name, value}] of the OS environment.
#[no_mangle]
pub extern "C" fn tn_system_env_all(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let vars: Vec<(String, String)> = std::env::vars().collect();
    let base = ctx.roots.len();
    for (k, v) in &vars {
        let kt = ctx.str(k);
        ctx.roots.push(kt);
        let vt = ctx.str(v);
        let kt = ctx.roots.pop().unwrap();
        let t = ctx.tuple(&[kt, vt]);
        ctx.roots.push(t);
    }
    let l = ctx.list_from_roots(base, vars.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

#[no_mangle]
pub extern "C" fn tn_system_delete_env(c: *mut Ctx, name: Term) -> Term {
    let ctx = cx(c);
    match term_string(name) {
        Some(n) => {
            unsafe { std::env::remove_var(n) };
            a("ok")
        }
        None => ctx.badarg("argument error"),
    }
}

#[no_mangle]
pub extern "C" fn tn_system_getpid(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    let s = std::process::id().to_string();
    ctx.str(&s)
}

#[no_mangle]
pub extern "C" fn tn_system_find_executable(c: *mut Ctx, name: Term) -> Term {
    let ctx = cx(c);
    let Some(n) = term_string(name) else { return ctx.badarg("argument error") };
    if n.contains('/') {
        let p = std::path::Path::new(&n);
        return if p.is_file() { ctx.str(&n) } else { NIL };
    }
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        let p = std::path::Path::new(dir).join(&n);
        if p.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(m) = p.metadata() {
                    if m.permissions().mode() & 0o111 == 0 {
                        continue;
                    }
                }
            }
            let s = p.to_string_lossy().into_owned();
            return ctx.str(&s);
        }
    }
    NIL
}

/// Run a program: (cmd, [args], cd | nil, [{k, v | nil}], stderr_to_stdout)
/// -> {output_binary, exit_status} or {:error, reason}.
#[no_mangle]
pub extern "C" fn tn_system_cmd(c: *mut Ctx, cmd: Term, args: Term, cd: Term, env: Term, merge: Term) -> Term {
    let ctx = cx(c);
    let Some(program) = term_string(cmd) else { return ctx.badarg("argument error") };
    let mut argv = Vec::new();
    let mut l = args;
    while is_cons(l) {
        match term_string(head(l)) {
            Some(s) => argv.push(s),
            None => return ctx.badarg("argument error"),
        }
        l = tail(l);
    }
    let mut command = std::process::Command::new(&program);
    command.args(&argv);
    if let Some(d) = term_string(cd) {
        command.current_dir(d);
    }
    let mut l = env;
    while is_cons(l) {
        let kv = head(l);
        if is_tuple(kv) && tuple_size(kv) == 2 {
            if let Some(k) = term_string(tuple_get(kv, 0)) {
                match term_string(tuple_get(kv, 1)) {
                    Some(v) => {
                        command.env(k, v);
                    }
                    None => {
                        command.env_remove(k);
                    }
                }
            }
        }
        l = tail(l);
    }
    crate::io::stdout_flush();
    let merge = merge == TRUE;
    let result = if merge {
        // Merge stderr into stdout through a shell-independent pipe pair.
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.output().map(|o| {
            let mut v = o.stdout;
            v.extend(o.stderr);
            (v, o.status)
        })
    } else {
        command.stdin(std::process::Stdio::null());
        command.stderr(std::process::Stdio::inherit());
        command.output().map(|o| (o.stdout, o.status))
    };
    match result {
        Ok((out, status)) => {
            let code = status.code().unwrap_or_else(|| {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    128 + status.signal().unwrap_or(0)
                }
                #[cfg(not(unix))]
                1
            });
            let b = ctx.binary(&out);
            ctx.tuple(&[b, small(code as i64)])
        }
        Err(e) => {
            let reason = match e.kind() {
                std::io::ErrorKind::NotFound => "enoent",
                std::io::ErrorKind::PermissionDenied => "eacces",
                _ => "eio",
            };
            ctx.tuple(&[a("error"), a(reason)])
        }
    }
}

#[no_mangle]
pub extern "C" fn tn_system_tmp_dir(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    for var in ["TMPDIR", "TEMP", "TMP"] {
        if let Ok(v) = std::env::var(var) {
            if std::path::Path::new(&v).is_dir() {
                return ctx.str(&v);
            }
        }
    }
    if std::path::Path::new("/tmp").is_dir() {
        return ctx.str("/tmp");
    }
    NIL
}

#[no_mangle]
pub extern "C" fn tn_system_user_home(c: *mut Ctx) -> Term {
    let ctx = cx(c);
    match std::env::var("HOME") {
        Ok(h) => ctx.str(&h),
        Err(_) => NIL,
    }
}

extern "C" {
    fn isatty(fd: i32) -> i32;
}

#[no_mangle]
pub extern "C" fn tn_isatty(_c: *mut Ctx, fd: Term) -> Term {
    boolean(unsafe { isatty(small_val(fd) as i32) } == 1)
}

/// HH:MM:SS.mmm local time, as Logger prints it.
pub fn log_time() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let mut tm = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, _pad: [0; 4] };
    unsafe { localtime_r(&secs, &mut tm) };
    format!("{:02}:{:02}:{:02}.{:03}", tm.hour, tm.min, tm.sec, d.subsec_millis())
}

pub fn stdout_is_tty() -> bool {
    unsafe { isatty(1) == 1 }
}
