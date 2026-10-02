//! Binaries, strings, unicode, bit syntax, iodata and inspect helpers.

use crate::atoms;
use crate::bif_core::cx;
use crate::heap::{empty_bin, Ctx};
use crate::num;
use crate::term::*;

// ---------------------------------------------------------------------------
// iodata / chardata
// ---------------------------------------------------------------------------

/// Flatten iodata (bytes 0..255, binaries, nested lists). With `chars`, list
/// integers are unicode codepoints encoded as UTF-8 (chardata).
pub fn flatten_io(t: Term, out: &mut Vec<u8>, chars: bool) -> bool {
    if is_binary(t) {
        out.extend_from_slice(bin_bytes(t));
        return true;
    }
    let mut x = t;
    while is_cons(x) {
        let h = head(x);
        if is_small(h) {
            let v = small_val(h);
            if chars {
                match char::from_u32(v as u32) {
                    Some(ch) if v >= 0 => {
                        let mut b = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                    }
                    _ => return false,
                }
            } else {
                if !(0..=255).contains(&v) {
                    return false;
                }
                out.push(v as u8);
            }
        } else if is_binary(h) {
            out.extend_from_slice(bin_bytes(h));
        } else if is_list(h) {
            if !flatten_io(h, out, chars) {
                return false;
            }
        } else {
            return false;
        }
        x = tail(x);
    }
    if x == NIL_LIST {
        true
    } else if is_binary(x) {
        out.extend_from_slice(bin_bytes(x));
        true
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn tn_iodata_to_binary(c: *mut Ctx, t: Term) -> Term {
    let ctx = cx(c);
    if is_binary(t) {
        return t;
    }
    let mut v = Vec::new();
    if !flatten_io(t, &mut v, false) {
        return ctx.badarg_args(&[(1, "not an iodata term")]);
    }
    ctx.binary(&v)
}

#[no_mangle]
pub extern "C" fn tn_chardata_to_binary(c: *mut Ctx, t: Term) -> Term {
    let ctx = cx(c);
    if is_binary(t) {
        return t;
    }
    let mut v = Vec::new();
    if !flatten_io(t, &mut v, true) {
        return ctx.badarg("argument error");
    }
    ctx.binary(&v)
}

#[no_mangle]
pub extern "C" fn tn_iodata_length(c: *mut Ctx, t: Term) -> Term {
    let mut v = Vec::new();
    if !flatten_io(t, &mut v, false) {
        return cx(c).badarg_args(&[(1, "not an iodata term")]);
    }
    small(v.len() as i64)
}

// ---------------------------------------------------------------------------
// Basic binary ops
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn tn_byte_size(c: *mut Ctx, b: Term) -> Term {
    if is_bitstring(b) {
        small(((bit_len(b) + 7) / 8) as i64)
    } else {
        cx(c).badarg_args(&[(1, "not a bitstring")])
    }
}

#[no_mangle]
pub extern "C" fn tn_bit_size(c: *mut Ctx, b: Term) -> Term {
    if is_bitstring(b) {
        small(bit_len(b) as i64)
    } else {
        cx(c).badarg_args(&[(1, "not a bitstring")])
    }
}

/// `<>`
#[no_mangle]
pub extern "C" fn tn_bin_concat(c: *mut Ctx, x: Term, y: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(x) || !is_binary(y) {
        let bad = if !is_binary(x) { x } else { y };
        let _ = bad;
        return ctx.badarg("expected binary argument in <> operator");
    }
    let (lx, ly) = (bin_len(x), bin_len(y));
    if lx == 0 {
        return y;
    }
    if ly == 0 {
        return x;
    }
    let mut r = [x, y];
    ctx.reserve_with(2 + (lx + ly + 7) / 8, &mut r);
    let (t, d) = ctx.bin_alloc_nogc(lx + ly);
    unsafe {
        std::ptr::copy_nonoverlapping(bin_bytes(r[0]).as_ptr(), d, lx);
        std::ptr::copy_nonoverlapping(bin_bytes(r[1]).as_ptr(), d.add(lx), ly);
    }
    t
}

/// Concatenate `n` binaries laid out in memory.
#[no_mangle]
pub extern "C" fn tn_bin_concat_n(c: *mut Ctx, parts: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    let n = n as usize;
    let mut total = 0;
    for i in 0..n {
        let p = unsafe { *parts.add(i) };
        if !is_binary(p) {
            return ctx.badarg("expected binary argument in <> operator");
        }
        total += bin_len(p);
    }
    if total == 0 {
        return empty_bin();
    }
    // The parts live in the caller's frame slots, so they are rooted and
    // updated if a GC happens during allocation.
    let (t, d) = ctx.bin_alloc(total);
    let mut off = 0;
    for i in 0..n {
        let p = unsafe { *parts.add(i) };
        let b = bin_bytes(p);
        unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), d.add(off), b.len()) };
        off += b.len();
    }
    t
}

// ---------------------------------------------------------------------------
// Bit syntax: construction
// ---------------------------------------------------------------------------

pub const BS_INT: u64 = 0;
pub const BS_BIN: u64 = 1;
pub const BS_FLOAT: u64 = 2;
pub const BS_UTF8: u64 = 3;
pub const BS_UTF16: u64 = 4;
pub const BS_UTF32: u64 = 5;
pub const BS_LITTLE: u64 = 1 << 8;
pub const BS_SIGNED: u64 = 1 << 9;

fn int_bytes(v: &num_bigint::BigInt, nbytes: usize, little: bool) -> Vec<u8> {
    let mut bytes = v.to_signed_bytes_le();
    let fill = if v.sign() == num_bigint::Sign::Minus { 0xff } else { 0 };
    bytes.resize(nbytes.max(bytes.len()), fill);
    bytes.truncate(nbytes);
    if !little {
        bytes.reverse();
    }
    bytes
}

pub const BS_BITS: u64 = 6;

/// Bit buffer, MSB-first.
pub struct BitBuf {
    pub v: Vec<u8>,
    pub n: usize,
}

impl BitBuf {
    pub fn new() -> BitBuf {
        BitBuf { v: Vec::new(), n: 0 }
    }
    #[inline]
    fn push_bit(&mut self, bit: u8) {
        if self.n % 8 == 0 {
            self.v.push(0);
        }
        if bit != 0 {
            let last = self.v.len() - 1;
            self.v[last] |= 0x80 >> (self.n % 8);
        }
        self.n += 1;
    }
    /// Append `n` bits of `src` starting at bit `off`.
    pub fn push_bits(&mut self, src: &[u8], off: usize, n: usize) {
        if self.n % 8 == 0 && off % 8 == 0 {
            let whole = n / 8;
            self.v.extend_from_slice(&src[off / 8..off / 8 + whole]);
            self.n += whole * 8;
            for i in whole * 8..n {
                self.push_bit(get_bit(src, off + i));
            }
            return;
        }
        for i in 0..n {
            self.push_bit(get_bit(src, off + i));
        }
    }
}

#[inline]
pub fn get_bit(src: &[u8], i: usize) -> u8 {
    (src[i >> 3] >> (7 - (i & 7))) & 1
}

/// `n` bits of `src` from bit `off`, packed MSB-first.
pub fn extract_bits(src: &[u8], off: usize, n: usize) -> Vec<u8> {
    if off % 8 == 0 {
        let mut v = src[off / 8..off / 8 + (n + 7) / 8].to_vec();
        if n % 8 != 0 {
            let last = v.len() - 1;
            v[last] &= 0xffu8 << (8 - n % 8);
        }
        return v;
    }
    let mut b = BitBuf::new();
    b.push_bits(src, off, n);
    b.v
}

/// Big-endian two's-complement bytes of `v` truncated to `nbits` bits,
/// returned as (bytes, leading pad bits to skip).
fn int_bits(v: Term, nbits: usize) -> (Vec<u8>, usize) {
    let nb = (nbits + 7) / 8;
    let bytes = if is_small(v) && nb <= 8 {
        let x = small_val(v) as u64;
        let mut b = vec![0u8; nb];
        for i in 0..nb {
            b[nb - 1 - i] = (x >> (8 * i)) as u8;
        }
        b
    } else {
        int_bytes(&num::to_big(v), nb, false)
    };
    (bytes, nb * 8 - nbits)
}

/// Encode one segment of a bitstring construction as a standalone bitstring.
/// `size` is in bits (-1 = default / whole value).
#[no_mangle]
pub extern "C" fn tn_bs_seg(c: *mut Ctx, v: Term, size: Term, flags: u64) -> Term {
    let ctx = cx(c);
    let ty = flags & 0xff;
    let little = flags & BS_LITTLE != 0;
    let size = if is_small(size) { small_val(size) } else { -2 };
    match ty {
        BS_INT => {
            if !is_integer(v) || size < 0 {
                return ctx.badarg("argument error");
            }
            let bits = size as usize;
            if bits % 8 == 0 {
                let nb = bits / 8;
                if is_small(v) && !little && nb <= 8 {
                    let x = small_val(v) as u64;
                    let mut b = vec![0u8; nb];
                    for i in 0..nb {
                        b[nb - 1 - i] = (x >> (8 * i)) as u8;
                    }
                    return ctx.binary(&b);
                }
                let b = int_bytes(&num::to_big(v), nb, little);
                return ctx.binary(&b);
            }
            let (bytes, skip) = int_bits(v, bits);
            let mut buf = BitBuf::new();
            buf.push_bits(&bytes, skip, bits);
            ctx.bitstring(&buf.v, bits)
        }
        BS_FLOAT => {
            if !is_number(v) {
                return ctx.badarg("argument error");
            }
            let f = num::num_to_f64(v);
            let bits = if size == -1 { 64 } else { size };
            let b: Vec<u8> = match (bits, little) {
                (64, false) => f.to_be_bytes().to_vec(),
                (64, true) => f.to_le_bytes().to_vec(),
                (32, false) => (f as f32).to_be_bytes().to_vec(),
                (32, true) => (f as f32).to_le_bytes().to_vec(),
                (16, _) => {
                    let h = f32_to_f16(f as f32);
                    if little { h.to_le_bytes().to_vec() } else { h.to_be_bytes().to_vec() }
                }
                _ => return ctx.badarg("argument error"),
            };
            ctx.binary(&b)
        }
        BS_BIN | BS_BITS => {
            if ty == BS_BIN && !is_binary(v) || !is_bitstring(v) {
                return ctx.badarg("argument error");
            }
            if size == -1 || size as usize == bit_len(v) {
                return v;
            }
            if size < 0 || size as usize > bit_len(v) {
                return ctx.badarg("argument error");
            }
            let n = size as usize;
            if is_binary(v) && n % 8 == 0 {
                return ctx.sub_binary(v, 0, n / 8);
            }
            let bytes = extract_bits(bit_bytes(v), 0, n);
            ctx.bitstring(&bytes, n)
        }
        BS_UTF8 | BS_UTF16 | BS_UTF32 => {
            if !is_small(v) {
                return ctx.badarg("argument error");
            }
            let ch = match char::from_u32(small_val(v) as u32) {
                Some(ch) if small_val(v) >= 0 => ch,
                _ => return ctx.badarg("argument error"),
            };
            match ty {
                BS_UTF8 => {
                    let mut b = [0u8; 4];
                    let s = ch.encode_utf8(&mut b);
                    ctx.binary(s.as_bytes())
                }
                BS_UTF16 => {
                    let mut b = [0u16; 2];
                    let s = ch.encode_utf16(&mut b);
                    let mut out = Vec::new();
                    for u in s.iter() {
                        if little {
                            out.extend_from_slice(&u.to_le_bytes());
                        } else {
                            out.extend_from_slice(&u.to_be_bytes());
                        }
                    }
                    ctx.binary(&out)
                }
                _ => {
                    let u = ch as u32;
                    let b = if little { u.to_le_bytes() } else { u.to_be_bytes() };
                    ctx.binary(&b)
                }
            }
        }
        _ => ctx.badarg("argument error"),
    }
}

fn f32_to_f16(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32 - 127 + 15;
    let man = x & 0x7fffff;
    if exp >= 31 {
        sign | 0x7c00
    } else if exp <= 0 {
        sign
    } else {
        sign | ((exp as u16) << 10) | ((man >> 13) as u16)
    }
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as u32;
    let man = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if man == 0 {
            sign
        } else {
            let mut e = 127 - 15 + 1;
            let mut m = man;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 31 {
        sign | 0x7f800000 | (man << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (man << 13)
    };
    f32::from_bits(bits)
}

/// Concatenate the segments of a bitstring construction.
#[no_mangle]
pub extern "C" fn tn_bs_concat_n(c: *mut Ctx, parts: *const Term, n: u64) -> Term {
    let ctx = cx(c);
    let n = n as usize;
    let mut aligned = true;
    for i in 0..n {
        let p = unsafe { *parts.add(i) };
        if !is_bitstring(p) {
            return ctx.badarg("argument error");
        }
        if !is_binary(p) {
            aligned = false;
        }
    }
    if aligned {
        return tn_bin_concat_n(c, parts, n as u64);
    }
    let mut buf = BitBuf::new();
    for i in 0..n {
        let p = unsafe { *parts.add(i) };
        buf.push_bits(bit_bytes(p), 0, bit_len(p));
    }
    let bits = buf.n;
    ctx.bitstring(&buf.v, bits)
}

// ---------------------------------------------------------------------------
// Bit syntax: matching. Offsets are bit offsets as small ints.
// ---------------------------------------------------------------------------

/// Bit size of a bitstring, or NONE if not a bitstring.
#[no_mangle]
pub extern "C" fn tn_bs_size(_c: *mut Ctx, b: Term) -> Term {
    if is_bitstring(b) {
        small(bit_len(b) as i64)
    } else {
        NONE
    }
}

#[no_mangle]
pub extern "C" fn tn_bs_int(c: *mut Ctx, b: Term, off: Term, bits: Term, flags: u64) -> Term {
    let ctx = cx(c);
    if !is_small(bits) || small_val(bits) < 0 {
        return NONE;
    }
    let nbits = small_val(bits) as usize;
    let off = small_val(off) as usize;
    if off + nbits > bit_len(b) {
        return NONE;
    }
    let little = flags & BS_LITTLE != 0;
    let signed = flags & BS_SIGNED != 0;
    let src = bit_bytes(b);
    if off % 8 == 0 && nbits % 8 == 0 && nbits <= 56 {
        let nb = nbits / 8;
        let s = &src[off / 8..off / 8 + nb];
        let mut x: u64 = 0;
        if little {
            for i in (0..nb).rev() {
                x = (x << 8) | s[i] as u64;
            }
        } else {
            for &byte in s {
                x = (x << 8) | byte as u64;
            }
        }
        let mut v = x as i64;
        if signed && nb > 0 && (x >> (nb * 8 - 1)) & 1 == 1 {
            v = (x as i64) - (1i64 << (nb * 8));
        }
        return small(v);
    }
    if nbits <= 56 && !(little && nbits % 8 == 0) {
        let mut x: u64 = 0;
        for i in 0..nbits {
            x = (x << 1) | get_bit(src, off + i) as u64;
        }
        let mut v = x as i64;
        if signed && nbits > 0 && (x >> (nbits - 1)) & 1 == 1 {
            v = (x as i64) - (1i64 << nbits);
        }
        return small(v);
    }
    // General case via bytes (big-endian, left-padded).
    let raw = extract_bits(src, off, nbits);
    let pad = raw.len() * 8 - nbits;
    let mut be = vec![0u8; raw.len()];
    // shift right by `pad` bits to right-align
    let mut carry = 0u16;
    for (i, &byte) in raw.iter().enumerate() {
        let w = (carry << 8) | byte as u16;
        be[i] = (w >> pad) as u8;
        carry = w & ((1 << pad) - 1);
    }
    let mut le = be;
    if !little {
        le.reverse();
    }
    let v = if signed {
        num_bigint::BigInt::from_signed_bytes_le(&le)
    } else {
        num_bigint::BigInt::from_bytes_le(num_bigint::Sign::Plus, &le)
    };
    num::from_big(ctx, &v)
}

#[no_mangle]
pub extern "C" fn tn_bs_float(c: *mut Ctx, b: Term, off: Term, bits: Term, flags: u64) -> Term {
    let ctx = cx(c);
    let bits = if is_small(bits) { small_val(bits) } else { 64 };
    if bits != 64 && bits != 32 && bits != 16 {
        return NONE;
    }
    let nbits = bits as usize;
    let off = small_val(off) as usize;
    if off + nbits > bit_len(b) {
        return NONE;
    }
    let s = extract_bits(bit_bytes(b), off, nbits);
    let little = flags & BS_LITTLE != 0;
    let f = match nbits {
        64 => {
            let a: [u8; 8] = s[..8].try_into().unwrap();
            if little { f64::from_le_bytes(a) } else { f64::from_be_bytes(a) }
        }
        32 => {
            let a: [u8; 4] = s[..4].try_into().unwrap();
            (if little { f32::from_le_bytes(a) } else { f32::from_be_bytes(a) }) as f64
        }
        _ => {
            let a: [u8; 2] = s[..2].try_into().unwrap();
            f16_to_f32(if little { u16::from_le_bytes(a) } else { u16::from_be_bytes(a) }) as f64
        }
    };
    if !f.is_finite() {
        return NONE;
    }
    ctx.float(f)
}

/// `len` bits at bit offset `off`.
#[no_mangle]
pub extern "C" fn tn_bs_bin(c: *mut Ctx, b: Term, off: Term, len: Term) -> Term {
    let ctx = cx(c);
    if !is_small(len) || small_val(len) < 0 {
        return NONE;
    }
    let off = small_val(off) as usize;
    let len = small_val(len) as usize;
    if off + len > bit_len(b) {
        return NONE;
    }
    if is_binary(b) && off % 8 == 0 && len % 8 == 0 {
        return ctx.sub_binary(b, off / 8, len / 8);
    }
    let bytes = extract_bits(bit_bytes(b), off, len);
    ctx.bitstring(&bytes, len)
}

/// The rest from bit offset `off`; with `aligned` = true it must be a whole
/// number of bytes (a `::binary` rest).
#[no_mangle]
pub extern "C" fn tn_bs_rest(c: *mut Ctx, b: Term, off: Term, aligned: Term) -> Term {
    let ctx = cx(c);
    let off = small_val(off) as usize;
    let l = bit_len(b);
    if off > l {
        return NONE;
    }
    let n = l - off;
    if aligned == TRUE && n % 8 != 0 {
        return NONE;
    }
    if is_binary(b) && off % 8 == 0 {
        return ctx.sub_binary(b, off / 8, n / 8);
    }
    let bytes = extract_bits(bit_bytes(b), off, n);
    ctx.bitstring(&bytes, n)
}

/// Decode one UTF-8 codepoint at bit offset `off`; NONE if invalid.
#[no_mangle]
pub extern "C" fn tn_bs_utf8(_c: *mut Ctx, b: Term, off: Term) -> Term {
    let off = small_val(off) as usize;
    let total = bit_len(b);
    if off >= total {
        return NONE;
    }
    let src = bit_bytes(b);
    let r = if off % 8 == 0 {
        let bytes = &src[off / 8..total / 8];
        decode_utf8(bytes)
    } else {
        let avail = ((total - off) / 8).min(4);
        let bytes = extract_bits(src, off, avail * 8);
        decode_utf8(&bytes)
    };
    match r {
        Some((cp, _)) => small(cp as i64),
        None => NONE,
    }
}

/// Decode a utf16/utf32 codepoint (flags: 4 = utf16, 5 = utf32, | BS_LITTLE).
#[no_mangle]
pub extern "C" fn tn_bs_utfn(_c: *mut Ctx, b: Term, off: Term, flags: u64) -> Term {
    let off = small_val(off) as usize;
    let total = bit_len(b);
    let little = flags & BS_LITTLE != 0;
    let src = bit_bytes(b);
    let read = |o: usize, n: usize| -> Option<u32> {
        if o + n * 8 > total {
            return None;
        }
        let bytes = extract_bits(src, o, n * 8);
        let mut v = 0u32;
        if little {
            for i in (0..n).rev() {
                v = (v << 8) | bytes[i] as u32;
            }
        } else {
            for &x in &bytes[..n] {
                v = (v << 8) | x as u32;
            }
        }
        Some(v)
    };
    let cp = if flags & 0xff == BS_UTF16 {
        let Some(w1) = read(off, 2) else { return NONE };
        if (0xD800..0xDC00).contains(&w1) {
            let Some(w2) = read(off + 16, 2) else { return NONE };
            if !(0xDC00..0xE000).contains(&w2) {
                return NONE;
            }
            0x10000 + ((w1 - 0xD800) << 10) + (w2 - 0xDC00)
        } else if (0xDC00..0xE000).contains(&w1) {
            return NONE;
        } else {
            w1
        }
    } else {
        let Some(v) = read(off, 4) else { return NONE };
        if v > 0x10FFFF || (0xD800..0xE000).contains(&v) {
            return NONE;
        }
        v
    };
    small(cp as i64)
}

/// Bits consumed by a utf16/utf32 codepoint (raw i64, not a term).
#[no_mangle]
pub extern "C" fn tn_bs_utfn_bits(_c: *mut Ctx, cp: Term, flags: u64) -> Term {
    let v = small_val(cp);
    small(if flags & 0xff == BS_UTF32 { 32 } else if v > 0xFFFF { 32 } else { 16 })
}

#[no_mangle]
pub extern "C" fn tn_utf8_len(_c: *mut Ctx, cp: Term) -> Term {
    let v = small_val(cp);
    small(if v < 0x80 {
        1
    } else if v < 0x800 {
        2
    } else if v < 0x10000 {
        3
    } else {
        4
    })
}

/// Does `lit` appear at bit offset `off` in `b`?
#[no_mangle]
pub extern "C" fn tn_bs_lit(_c: *mut Ctx, b: Term, off: Term, lit: Term) -> Term {
    let l = bin_bytes(lit);
    let off = small_val(off) as usize;
    if off + l.len() * 8 > bit_len(b) {
        return FALSE;
    }
    let src = bit_bytes(b);
    if off % 8 == 0 {
        return boolean(&src[off / 8..off / 8 + l.len()] == l);
    }
    boolean(extract_bits(src, off, l.len() * 8) == l)
}

pub fn decode_utf8(s: &[u8]) -> Option<(u32, usize)> {
    let b0 = *s.first()?;
    if b0 < 0x80 {
        return Some((b0 as u32, 1));
    }
    let (len, init) = if b0 & 0xE0 == 0xC0 {
        (2, (b0 & 0x1F) as u32)
    } else if b0 & 0xF0 == 0xE0 {
        (3, (b0 & 0x0F) as u32)
    } else if b0 & 0xF8 == 0xF0 {
        (4, (b0 & 0x07) as u32)
    } else {
        return None;
    };
    if s.len() < len {
        return None;
    }
    let mut cp = init;
    for &b in &s[1..len] {
        if b & 0xC0 != 0x80 {
            return None;
        }
        cp = (cp << 6) | (b & 0x3F) as u32;
    }
    let min = [0, 0, 0x80, 0x800, 0x10000][len];
    if cp < min || cp > 0x10FFFF || (0xD800..=0xDFFF).contains(&cp) {
        return None;
    }
    Some((cp, len))
}

// ---------------------------------------------------------------------------
// Graphemes (approximation of extended grapheme clusters)
// ---------------------------------------------------------------------------

fn is_extend(cp: u32) -> bool {
    matches!(cp,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0610..=0x061A |
        0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 |
        0x0900..=0x0903 | 0x093A..=0x094F | 0x0951..=0x0957 | 0x0962..=0x0963 |
        0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E |
        0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200C | 0x20D0..=0x20FF |
        0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F |
        0xE0100..=0xE01EF)
}

fn is_regional(cp: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&cp)
}

/// Split into extended grapheme cluster byte ranges (UAX #29). Invalid UTF-8
/// bytes form single graphemes.
pub fn graphemes(s: &[u8]) -> Vec<(usize, usize)> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let (valid, bad) = match std::str::from_utf8(&s[i..]) {
            Ok(v) => (v, false),
            Err(e) => (unsafe { std::str::from_utf8_unchecked(&s[i..i + e.valid_up_to()]) }, true),
        };
        for (off, g) in valid.grapheme_indices(true) {
            out.push((i + off, i + off + g.len()));
        }
        i += valid.len();
        if bad && i < s.len() {
            out.push((i, i + 1));
            i += 1;
        }
    }
    out
}

#[no_mangle]
pub extern "C" fn tn_str_length(c: *mut Ctx, b: Term) -> Term {
    if !is_binary(b) {
        return cx(c).badarg("argument error");
    }
    small(graphemes(bin_bytes(b)).len() as i64)
}

#[no_mangle]
pub extern "C" fn tn_str_graphemes(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let gs = graphemes(bin_bytes(b));
    let bi = ctx.push(b);
    let base = ctx.roots.len();
    for &(s, e) in &gs {
        let g = ctx.sub_binary(ctx.get(bi), s, e - s);
        ctx.roots.push(g);
    }
    let l = ctx.list_from_roots(base, gs.len(), NIL_LIST);
    ctx.truncate(bi);
    l
}

#[no_mangle]
pub extern "C" fn tn_str_codepoints(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let l = decode_utf8(&bytes[i..]).map(|x| x.1).unwrap_or(1);
        ranges.push((i, l));
        i += l;
    }
    let bi = ctx.push(b);
    let base = ctx.roots.len();
    for &(s, l) in &ranges {
        let g = ctx.sub_binary(ctx.get(bi), s, l);
        ctx.roots.push(g);
    }
    let r = ctx.list_from_roots(base, ranges.len(), NIL_LIST);
    ctx.truncate(bi);
    r
}

/// String.to_charlist: list of codepoints
#[no_mangle]
pub extern "C" fn tn_str_to_charlist(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let mut cps = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match decode_utf8(&bytes[i..]) {
            Some((cp, l)) => {
                cps.push(small(cp as i64));
                i += l;
            }
            None => {
                return ctx.raise_msg(a::UNICODE_CONVERSION_ERROR, "invalid encoding");
            }
        }
    }
    ctx.list_from_vec(&mut cps)
}

#[no_mangle]
pub extern "C" fn tn_bin_to_list(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg_args(&[(1, "not a binary")]);
    }
    let mut v: Vec<Term> = bin_bytes(b).iter().map(|&x| small(x as i64)).collect();
    ctx.list_from_vec(&mut v)
}

fn to_str(t: Term) -> String {
    String::from_utf8_lossy(bin_bytes(t)).into_owned()
}

/// mode: 0 = upcase, 1 = downcase, 2 = capitalize; ascii: only ASCII
#[no_mangle]
pub extern "C" fn tn_str_case(c: *mut Ctx, b: Term, mode: Term, ascii: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let s = to_str(b);
    let ascii = ascii == TRUE;
    let r = match small_val(mode) {
        0 => {
            if ascii {
                s.to_ascii_uppercase()
            } else {
                s.to_uppercase()
            }
        }
        1 => {
            if ascii {
                s.to_ascii_lowercase()
            } else {
                s.to_lowercase()
            }
        }
        _ => {
            let bytes = s.as_bytes();
            let gs = graphemes(bytes);
            if gs.is_empty() {
                String::new()
            } else {
                let first = std::str::from_utf8(&bytes[gs[0].0..gs[0].1]).unwrap_or("");
                let rest = std::str::from_utf8(&bytes[gs[0].1..]).unwrap_or("");
                let mut out = String::new();
                // Titlecase approximation.
                let mut chars = first.chars();
                if let Some(ch) = chars.next() {
                    let up: String = match ch {
                        'ǆ' | 'ǅ' | 'Ǆ' => "ǅ".to_string(),
                        'ǉ' | 'ǈ' | 'Ǉ' => "ǈ".to_string(),
                        'ǌ' | 'ǋ' | 'Ǌ' => "ǋ".to_string(),
                        'ß' => "Ss".to_string(),
                        _ => ch.to_uppercase().collect(),
                    };
                    out.push_str(&up);
                    out.extend(chars);
                }
                if ascii {
                    out.push_str(&rest.to_ascii_lowercase());
                } else {
                    out.push_str(&rest.to_lowercase());
                }
                out
            }
        }
    };
    ctx.str(&r)
}

fn is_ws(cp: u32) -> bool {
    matches!(cp, 0x09..=0x0D | 0x20 | 0x85 | 0xA0 | 0x1680 | 0x2000..=0x200A |
        0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000)
}

/// mode 0 both, 1 leading, 2 trailing
#[no_mangle]
pub extern "C" fn tn_str_trim(c: *mut Ctx, b: Term, mode: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let m = small_val(mode);
    let mut start = 0;
    let mut end = bytes.len();
    if m == 0 || m == 1 {
        while start < end {
            match decode_utf8(&bytes[start..]) {
                Some((cp, l)) if is_ws(cp) => start += l,
                _ => break,
            }
        }
    }
    if m == 0 || m == 2 {
        while end > start {
            // step back one codepoint
            let mut k = end - 1;
            while k > start && bytes[k] & 0xC0 == 0x80 {
                k -= 1;
            }
            match decode_utf8(&bytes[k..end]) {
                Some((cp, _)) if is_ws(cp) => end = k,
                _ => break,
            }
        }
    }
    ctx.sub_binary(b, start, end - start)
}

/// String.trim(s, to_trim) family — removes repeated occurrences of `t`.
#[no_mangle]
pub extern "C" fn tn_str_trim_str(c: *mut Ctx, b: Term, t: Term, mode: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_binary(t) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let tb = bin_bytes(t);
    if tb.is_empty() {
        return b;
    }
    let m = small_val(mode);
    let mut start = 0;
    let mut end = bytes.len();
    if m == 0 || m == 1 {
        while end - start >= tb.len() && &bytes[start..start + tb.len()] == tb {
            start += tb.len();
        }
    }
    if m == 0 || m == 2 {
        while end - start >= tb.len() && &bytes[end - tb.len()..end] == tb {
            end -= tb.len();
        }
    }
    ctx.sub_binary(b, start, end - start)
}

/// Collect patterns (binary or list of binaries).
fn patterns(p: Term) -> Option<Vec<Vec<u8>>> {
    if is_binary(p) {
        return Some(vec![bin_bytes(p).to_vec()]);
    }
    let mut v = Vec::new();
    let mut x = p;
    while is_cons(x) {
        let h = head(x);
        if !is_binary(h) {
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

/// Find the leftmost match (longest among those at the same position).
fn find_first(hay: &[u8], from: usize, pats: &[Vec<u8>]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for p in pats {
        if p.is_empty() {
            continue;
        }
        if let Some(pos) = find_sub(&hay[from..], p) {
            let pos = pos + from;
            match best {
                None => best = Some((pos, p.len())),
                Some((bp, bl)) => {
                    if pos < bp || (pos == bp && p.len() > bl) {
                        best = Some((pos, p.len()))
                    }
                }
            }
        }
    }
    best
}

pub fn find_sub(h: &[u8], n: &[u8]) -> Option<usize> {
    if n.is_empty() {
        return Some(0);
    }
    if n.len() > h.len() {
        return None;
    }
    if n.len() == 1 {
        return h.iter().position(|&b| b == n[0]);
    }
    h.windows(n.len()).position(|w| w == n)
}

#[no_mangle]
pub extern "C" fn tn_str_contains(c: *mut Ctx, b: Term, p: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let h = bin_bytes(b);
    boolean(pats.iter().any(|p| find_sub(h, p).is_some()))
}

#[no_mangle]
pub extern "C" fn tn_str_starts_with(c: *mut Ctx, b: Term, p: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let h = bin_bytes(b);
    boolean(pats.iter().any(|p| h.starts_with(p)))
}

#[no_mangle]
pub extern "C" fn tn_str_ends_with(c: *mut Ctx, b: Term, p: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let h = bin_bytes(b);
    boolean(pats.iter().any(|p| h.ends_with(p)))
}

/// String.split(s, pattern, parts, trim)
#[no_mangle]
pub extern "C" fn tn_str_split(c: *mut Ctx, b: Term, p: Term, parts: Term, trim: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let h = bin_bytes(b).to_vec();
    let trim = trim == TRUE;
    let max_parts = if is_small(parts) && small_val(parts) > 0 {
        small_val(parts) as usize
    } else {
        usize::MAX
    };
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if pats.iter().all(|p| p.is_empty()) {
        // Split into graphemes: ["", "a", "b", ""] semantics.
        let gs = graphemes(&h);
        let mut out: Vec<(usize, usize)> = Vec::new();
        if max_parts == 1 {
            out.push((0, h.len()));
        } else {
            out.push((0, 0));
            for (i, g) in gs.iter().enumerate() {
                if out.len() + 1 == max_parts {
                    out.push((g.0, h.len()));
                    break;
                }
                out.push(*g);
                if i == gs.len() - 1 {
                    out.push((h.len(), h.len()));
                }
            }
            if gs.is_empty() {
                out.push((0, 0));
            }
        }
        ranges = out;
    } else {
        let mut start = 0;
        loop {
            if ranges.len() + 1 >= max_parts {
                ranges.push((start, h.len()));
                break;
            }
            match find_first(&h, start, &pats) {
                Some((pos, len)) => {
                    ranges.push((start, pos));
                    start = pos + len;
                }
                None => {
                    ranges.push((start, h.len()));
                    break;
                }
            }
        }
    }
    if trim {
        ranges.retain(|(s, e)| e > s);
    }
    let bi = ctx.push(b);
    let base = ctx.roots.len();
    for &(s, e) in &ranges {
        let piece = ctx.sub_binary(ctx.get(bi), s, e - s);
        ctx.roots.push(piece);
    }
    let l = ctx.list_from_roots(base, ranges.len(), NIL_LIST);
    ctx.truncate(bi);
    l
}

/// String.split/1 — on unicode whitespace, dropping empties.
#[no_mangle]
pub extern "C" fn tn_str_split_ws(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let h = bin_bytes(b);
    let mut ranges = Vec::new();
    let mut i = 0;
    let mut start: Option<usize> = None;
    while i < h.len() {
        let (cp, l) = decode_utf8(&h[i..]).unwrap_or((h[i] as u32, 1));
        // String.split/1 does not break on non-breaking spaces.
        if is_ws(cp) && !matches!(cp, 0xA0 | 0x2007 | 0x202F) {
            if let Some(s) = start.take() {
                ranges.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
        i += l;
    }
    if let Some(s) = start {
        ranges.push((s, h.len()));
    }
    let bi = ctx.push(b);
    let base = ctx.roots.len();
    for &(s, e) in &ranges {
        let piece = ctx.sub_binary(ctx.get(bi), s, e - s);
        ctx.roots.push(piece);
    }
    let l = ctx.list_from_roots(base, ranges.len(), NIL_LIST);
    ctx.truncate(bi);
    l
}

/// String.replace with binary replacement. global: true/false
#[no_mangle]
pub extern "C" fn tn_str_replace(c: *mut Ctx, b: Term, p: Term, r: Term, global: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_binary(r) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) => p,
        None => return ctx.badarg("argument error"),
    };
    let h = bin_bytes(b);
    let rep = bin_bytes(r);
    let mut out = Vec::with_capacity(h.len());
    if pats.iter().all(|p| p.is_empty()) {
        // Insert replacement between every grapheme (and at ends).
        let gs = graphemes(h);
        out.extend_from_slice(rep);
        for (s, e) in gs {
            out.extend_from_slice(&h[s..e]);
            if global == TRUE {
                out.extend_from_slice(rep);
            }
        }
        if global != TRUE {
            // only the first insertion
        }
        return ctx.binary(&out);
    }
    let mut start = 0;
    loop {
        match find_first(h, start, &pats) {
            Some((pos, len)) => {
                out.extend_from_slice(&h[start..pos]);
                out.extend_from_slice(rep);
                start = pos + len;
                if global != TRUE {
                    break;
                }
            }
            None => break,
        }
    }
    out.extend_from_slice(&h[start..]);
    ctx.binary(&out)
}

/// :binary.matches(subject, patterns) → [{pos, len}]
#[no_mangle]
pub extern "C" fn tn_binary_matches(c: *mut Ctx, b: Term, p: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) if p.iter().all(|x| !x.is_empty()) => p,
        _ => return ctx.badarg("argument error"),
    };
    let h = bin_bytes(b);
    let mut found = Vec::new();
    let mut start = 0;
    while let Some((pos, len)) = find_first(h, start, &pats) {
        found.push((pos, len));
        start = pos + len;
    }
    let base = ctx.roots.len();
    for &(pos, len) in &found {
        let t = ctx.tuple(&[small(pos as i64), small(len as i64)]);
        ctx.roots.push(t);
    }
    let l = ctx.list_from_roots(base, found.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

/// :binary.match → {pos, len} | :nomatch
#[no_mangle]
pub extern "C" fn tn_binary_match(c: *mut Ctx, b: Term, p: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let pats = match patterns(p) {
        Some(p) if p.iter().all(|x| !x.is_empty()) => p,
        _ => return ctx.badarg("argument error"),
    };
    match find_first(bin_bytes(b), 0, &pats) {
        Some((pos, len)) => ctx.tuple(&[small(pos as i64), small(len as i64)]),
        None => atom(atoms::intern("nomatch")),
    }
}

#[no_mangle]
pub extern "C" fn tn_binary_part(c: *mut Ctx, b: Term, pos: Term, len: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(pos) || !is_small(len) || small_val(pos) < 0 {
        let mut errs = Vec::new();
        if !is_binary(b) {
            errs.push((1, "not a binary"));
        }
        if !is_small(pos) && !is_bigint(pos) {
            errs.push((2, "not an integer"));
        } else if !is_small(pos) || small_val(pos) < 0 {
            errs.push((2, "out of range"));
        }
        if !is_small(len) && !is_bigint(len) {
            errs.push((3, "not an integer"));
        }
        return ctx.badarg_args(&errs);
    }
    let (mut p, mut l) = (small_val(pos), small_val(len));
    if l < 0 {
        p += l;
        l = -l;
    }
    if p < 0 || (p + l) as usize > bin_len(b) {
        return if small_val(pos) as usize > bin_len(b) {
            ctx.badarg_args(&[(2, "out of range")])
        } else {
            ctx.badarg_args(&[(3, "out of range")])
        };
    }
    ctx.sub_binary(b, p as usize, l as usize)
}

#[no_mangle]
pub extern "C" fn tn_binary_at(c: *mut Ctx, b: Term, pos: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(pos) {
        return ctx.badarg("argument error");
    }
    let p = small_val(pos);
    let bytes = bin_bytes(b);
    if p < 0 || p as usize >= bytes.len() {
        return ctx.badarg("argument error");
    }
    small(bytes[p as usize] as i64)
}

#[no_mangle]
pub extern "C" fn tn_binary_copy(c: *mut Ctx, b: Term, n: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(n) || small_val(n) < 0 {
        return {
            let mut errs = Vec::new();
            if !is_binary(b) {
                errs.push((1, "not a binary"));
            }
            if !is_small(n) && !is_bigint(n) {
                errs.push((2, "not an integer"));
            } else {
                errs.push((2, "out of range"));
            }
            ctx.badarg_args(&errs)
        };
    }
    let n = small_val(n) as usize;
    let l = bin_len(b);
    if n == 0 || l == 0 {
        return empty_bin();
    }
    let mut r = [b];
    ctx.reserve_with(2 + (l * n + 7) / 8, &mut r);
    let (t, d) = ctx.bin_alloc_nogc(l * n);
    let src = bin_bytes(r[0]);
    for i in 0..n {
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), d.add(i * l), l) };
    }
    t
}

/// String.slice(s, start, len) on graphemes; negative start counts from end.
#[no_mangle]
pub extern "C" fn tn_str_slice(c: *mut Ctx, b: Term, start: Term, len: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(start) || !is_small(len) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let gs = graphemes(bytes);
    let n = gs.len() as i64;
    let mut s = small_val(start);
    let l = small_val(len);
    if s < 0 {
        s += n;
        if s < 0 {
            s = 0;
        }
    }
    if l <= 0 || s >= n {
        return empty_bin();
    }
    let e = (s + l).min(n);
    let bs = gs[s as usize].0;
    let be = gs[(e - 1) as usize].1;
    ctx.sub_binary(b, bs, be - bs)
}

#[no_mangle]
pub extern "C" fn tn_str_reverse(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let gs = graphemes(bytes);
    let mut out = Vec::with_capacity(bytes.len());
    for &(s, e) in gs.iter().rev() {
        out.extend_from_slice(&bytes[s..e]);
    }
    ctx.binary(&out)
}

#[no_mangle]
pub extern "C" fn tn_str_valid(_c: *mut Ctx, b: Term) -> Term {
    boolean(is_binary(b) && std::str::from_utf8(bin_bytes(b)).is_ok())
}

/// Next grapheme: {g, rest} | nil
#[no_mangle]
pub extern "C" fn tn_str_next_grapheme(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    if bytes.is_empty() {
        return NIL;
    }
    let gs = graphemes(bytes);
    let (s, e) = gs[0];
    let total = bytes.len();
    let bi = ctx.push(b);
    let g = ctx.sub_binary(ctx.get(bi), s, e - s);
    let gi = ctx.push(g);
    let rest = ctx.sub_binary(ctx.get(bi), e, total - e);
    let ri = ctx.push(rest);
    let t = ctx.tuple(&[ctx.get(gi), ctx.get(ri)]);
    ctx.truncate(bi);
    t
}

// ---------------------------------------------------------------------------
// Inspect helpers
// ---------------------------------------------------------------------------

fn printable_cp(cp: u32) -> bool {
    matches!(cp, 0x20..=0x7E | 0xA0..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
        || matches!(cp, 0x0A | 0x0D | 0x09 | 0x0B | 0x08 | 0x0C | 0x1B | 0x7F | 0x07)
}

pub fn str_printable(s: &[u8]) -> bool {
    let mut i = 0;
    while i < s.len() {
        match decode_utf8(&s[i..]) {
            Some((cp, l)) if printable_cp(cp) => i += l,
            _ => return false,
        }
    }
    true
}

#[no_mangle]
pub extern "C" fn tn_str_printable(_c: *mut Ctx, b: Term) -> Term {
    boolean(is_binary(b) && str_printable(bin_bytes(b)))
}

/// Escape for inspect with the given quote character (no surrounding quotes).
pub fn escape(s: &[u8], quote: u8) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    let mut i = 0;
    while i < s.len() {
        let (cp, l) = decode_utf8(&s[i..]).unwrap_or((s[i] as u32, 1));
        let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
        match cp {
            0x5C => out.push_str("\\\\"),
            0x23 if i + 1 < s.len() && s[i + 1] == b'{' => out.push_str("\\#"),
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            0x7F => out.push_str("\\d"),
            0x1B => out.push_str("\\e"),
            0x0C => out.push_str("\\f"),
            0x0A => out.push_str("\\n"),
            0x0D => out.push_str("\\r"),
            0x09 => out.push_str("\\t"),
            0x0B => out.push_str("\\v"),
            0x00 => out.push_str("\\0"),
            _ if cp == quote as u32 => {
                out.push('\\');
                out.push(ch);
            }
            _ if cp < 0x20 => out.push_str(&format!("\\x{:02X}", cp)),
            _ => out.push(ch),
        }
        i += l;
    }
    out
}

/// Inspect of a binary: `"..."` when printable, `<<1, 2>>` otherwise (single line).
#[no_mangle]
pub extern "C" fn tn_inspect_escape(c: *mut Ctx, b: Term, quote: Term) -> Term {
    let ctx = cx(c);
    let q = small_val(quote) as u8;
    let s = escape(bin_bytes(b), q);
    ctx.str(&s)
}

fn is_ident_start(c: char) -> bool {
    c.is_lowercase() || c == '_' || (c.is_alphabetic() && !c.is_uppercase())
}
fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Classify an atom name for printing.
fn atom_kind(s: &str) -> &'static str {
    // Macro.inner_classify/1
    const NOT_CALLABLE: &[&str] = &["%", "%{}", "{}", "<<>>", "...", "..", ".", "..//", "->"];
    const QUOTED_OPS: &[&str] = &["::", "^^^", "~~~", "<|>"];
    const OPS: &[&str] = &[
        "+", "-", "*", "/", "==", "!=", "===", "!==", "<", ">", "<=", ">=", "&&", "||", "!", "<>",
        "++", "--", "|>", "=~", "<-", "\\\\", "@", "^", "&", "|", "=", "<<<", ">>>", "&&&", "|||",
        "<<~", "~>>", "<~", "~>", "<~>", "+++", "---", "**",
    ];
    if NOT_CALLABLE.contains(&s) {
        return "notcall";
    }
    if QUOTED_OPS.contains(&s) {
        return "quoted";
    }
    if OPS.contains(&s) {
        return "op";
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c0) if is_ident_start(c0) => {
            let body: Vec<char> = s.chars().collect();
            let mut i = 1;
            while i < body.len() && (is_ident_char(body[i]) || body[i] == '@') {
                i += 1;
            }
            if i < body.len() && (body[i] == '?' || body[i] == '!') {
                i += 1;
            }
            if i == body.len() {
                "ident"
            } else {
                "quoted"
            }
        }
        Some(c0) if c0.is_uppercase() => {
            // Alias-like: Foo, Foo.Bar (each segment capitalized identifier)
            if s.split('.').all(|seg| {
                let mut cs = seg.chars();
                matches!(cs.next(), Some(c) if c.is_uppercase())
                    && cs.all(|c| is_ident_char(c))
            }) {
                "alias"
            } else {
                "quoted"
            }
        }
        _ => "quoted",
    }
}

/// mode 0: literal (`:foo`, `Foo.Bar`); 1: keyword key (`foo:`); 2: remote call name
#[no_mangle]
pub extern "C" fn tn_inspect_atom(c: *mut Ctx, t: Term, mode: Term) -> Term {
    let ctx = cx(c);
    let s = atoms::name(atom_idx(t));
    let m = small_val(mode);
    let out = match m {
        0 => {
            if s == "nil" || s == "true" || s == "false" {
                s.to_string()
            } else if let Some(rest) = s.strip_prefix("Elixir.") {
                if atom_kind(rest) == "alias" {
                    rest.to_string()
                } else {
                    format!(":\"{}\"", escape(s.as_bytes(), b'"'))
                }
            } else if s == "Elixir" {
                "Elixir".to_string()
            } else {
                match atom_kind(s) {
                    "ident" | "op" | "alias" | "notcall" => format!(":{}", s),
                    _ => format!(":\"{}\"", escape(s.as_bytes(), b'"')),
                }
            }
        }
        1 => match atom_kind(s) {
            "alias" if s.starts_with("Elixir.") => format!("\"{}\":", s),
            "ident" | "alias" | "op" | "notcall" => format!("{}:", s),
            _ => format!("\"{}\":", escape(s.as_bytes(), b'"')),
        },
        _ => match atom_kind(s) {
            "ident" | "op" => s.to_string(),
            _ => format!("\"{}\"", escape(s.as_bytes(), b'"')),
        },
    };
    ctx.str(&out)
}

/// List.ascii_printable?
#[no_mangle]
pub extern "C" fn tn_list_ascii_printable(_c: *mut Ctx, l: Term) -> Term {
    let mut x = l;
    if x == NIL_LIST {
        return TRUE;
    }
    while is_cons(x) {
        let h = head(x);
        if !is_small(h) {
            return FALSE;
        }
        let v = small_val(h);
        if !(matches!(v, 32..=126) || matches!(v, 7..=13 | 27 | 127)) {
            return FALSE;
        }
        x = tail(x);
    }
    boolean(x == NIL_LIST)
}

#[no_mangle]
pub extern "C" fn tn_pid_to_string(c: *mut Ctx, p: Term) -> Term {
    let ctx = cx(c);
    let s = format!("#PID<0.{}.0>", pid_id(p));
    ctx.str(&s)
}

#[no_mangle]
pub extern "C" fn tn_ref_to_string(c: *mut Ctx, r: Term) -> Term {
    let ctx = cx(c);
    let id = word(r, 1);
    let s = format!("#Reference<0.{}.{}.{}>", 1234567 + (id >> 32), 3000000000u64 + (id % 1000), id);
    ctx.str(&s)
}

static SCRIPT_FILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Records the program's script file (for inspecting script-level funs).
#[no_mangle]
pub extern "C" fn tn_set_script_file(p: *const std::ffi::c_char) {
    if !p.is_null() {
        let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned();
        if !s.is_empty() {
            let _ = SCRIPT_FILE.set(s);
        }
    }
}

#[no_mangle]
pub extern "C" fn tn_fun_to_string(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    let info = word(f, 3) as *const FunInfo;
    let arity = closure_arity(f);
    let s = if info.is_null() {
        format!("#Function<0.0/{}>", arity)
    } else {
        let fi = unsafe { &*info };
        let m = atoms::name(atom_idx(fi.module));
        let mname = match m.strip_prefix("Elixir.") {
            Some(x) => x.to_string(),
            None => format!(":{}", m),
        };
        let n = atoms::name(atom_idx(fi.name));
        let remote_name = |n: &str| match atom_kind(n) {
            "ident" | "op" => n.to_string(),
            _ => format!("\"{}\"", escape(n.as_bytes(), b'"')),
        };
        if fi.kind != 1 && m.starts_with("elixir_compiler_") && SCRIPT_FILE.get().is_some() {
            // Script-level funs: Inspect.Function's "in file:" form.
            let file = SCRIPT_FILE.get().unwrap();
            let rel = match std::env::current_dir() {
                Ok(cwd) => {
                    let p = std::path::Path::new(file);
                    let abs = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
                    abs.strip_prefix(&cwd).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| file.clone())
                }
                Err(_) => file.clone(),
            };
            let s = format!("#Function<{}.{} in file:{}>", fi.index, (crate::cmp::hash(fi.module) % 100_000_000) + fi.arity, rel);
            return ctx.str(&s);
        }
        if fi.kind == 1 {
            format!("&{}.{}/{}", mname, remote_name(n), arity)
        } else {
            // -parent/arity-fun-N-
            let parent = n
                .strip_prefix('-')
                .and_then(|x| x.rfind("-fun-").map(|i| &x[..i]))
                .unwrap_or(n);
            format!(
                "#Function<{}.{}/{} in {}.{}>",
                fi.index,
                (crate::cmp::hash(fi.module) % 100_000_000) + fi.arity,
                arity,
                mname,
                match parent.rsplit_once('/') {
                    Some((f, a)) => format!("{}/{}", remote_name(f), a),
                    None => remote_name(parent),
                }
            )
        }
    };
    ctx.str(&s)
}

#[no_mangle]
pub extern "C" fn tn_fun_info(c: *mut Ctx, f: Term) -> Term {
    let ctx = cx(c);
    let info = word(f, 3) as *const FunInfo;
    let (m, n) = if info.is_null() {
        (NIL, NIL)
    } else {
        let fi = unsafe { &*info };
        (fi.module, fi.name)
    };
    ctx.tuple(&[m, n, small(closure_arity(f) as i64)])
}

/// Integer.to_string(i, base) for inspect
#[no_mangle]
pub extern "C" fn tn_int_to_string(c: *mut Ctx, i: Term, base: Term) -> Term {
    crate::bif_core::tn_integer_to_binary(c, i, base)
}

/// String.pad_leading/trailing(s, count, padding) — mode 0 leading, 1 trailing
#[no_mangle]
pub extern "C" fn tn_str_pad(c: *mut Ctx, b: Term, count: Term, pad: Term, mode: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(count) || !is_binary(pad) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let n = graphemes(bytes).len() as i64;
    let want = small_val(count);
    if want <= n {
        return b;
    }
    let pb = bin_bytes(pad);
    let pg = graphemes(pb);
    if pg.is_empty() {
        return ctx.badarg("argument error");
    }
    let need = (want - n) as usize;
    let mut padding = Vec::new();
    for i in 0..need {
        let (s, e) = pg[i % pg.len()];
        padding.extend_from_slice(&pb[s..e]);
    }
    let mut out = Vec::with_capacity(bytes.len() + padding.len());
    if small_val(mode) == 0 {
        out.extend_from_slice(&padding);
        out.extend_from_slice(bytes);
    } else {
        out.extend_from_slice(bytes);
        out.extend_from_slice(&padding);
    }
    ctx.binary(&out)
}

/// Collect graphemes-or-codepoints index: String.at
#[no_mangle]
pub extern "C" fn tn_str_at(c: *mut Ctx, b: Term, idx: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(idx) {
        return ctx.badarg("argument error");
    }
    let bytes = bin_bytes(b);
    let gs = graphemes(bytes);
    let mut i = small_val(idx);
    if i < 0 {
        i += gs.len() as i64;
    }
    if i < 0 || i as usize >= gs.len() {
        return NIL;
    }
    let (s, e) = gs[i as usize];
    ctx.sub_binary(b, s, e - s)
}

/// Build a charlist-to-binary for List.to_string etc. (chardata)
#[no_mangle]
pub extern "C" fn tn_list_to_binary(c: *mut Ctx, l: Term) -> Term {
    tn_chardata_to_binary(c, l)
}

/// String.jaro/levenshtein not provided; simple helpers for String.myers? no.

/// Integer to charlist (Integer.to_charlist)
#[no_mangle]
pub extern "C" fn tn_integer_to_list(c: *mut Ctx, i: Term, base: Term) -> Term {
    let ctx = cx(c);
    if !is_integer(i) || !is_small(base) || !(2..=36).contains(&small_val(base)) {
        let mut errs = Vec::new();
        if !is_integer(i) {
            errs.push((1, "not an integer"));
        }
        if !is_small(base) || !(2..=36).contains(&small_val(base)) {
            errs.push((2, "not an integer in the range 2 through 36"));
        }
        return ctx.badarg_args(&errs);
    }
    let s = num::int_to_string(i, small_val(base) as u32);
    let mut v: Vec<Term> = s.bytes().map(|b| small(b as i64)).collect();
    ctx.list_from_vec(&mut v)
}
