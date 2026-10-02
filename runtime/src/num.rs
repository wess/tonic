//! Integers (small + arbitrary precision) and floats.

use crate::heap::Ctx;
use crate::term::*;
use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive, Zero};

pub fn to_big(t: Term) -> BigInt {
    if is_small(t) {
        BigInt::from(small_val(t))
    } else {
        let n = size_of_hdr(hdr(t)) as usize - 1;
        let neg = word(t, 1) != 0;
        let mut digits: Vec<u32> = Vec::with_capacity(n * 2);
        for i in 0..n {
            let l = word(t, 2 + i);
            digits.push(l as u32);
            digits.push((l >> 32) as u32);
        }
        BigInt::from_slice(if neg { Sign::Minus } else { Sign::Plus }, &digits)
    }
}

pub fn from_big(ctx: &mut Ctx, b: &BigInt) -> Term {
    if let Some(i) = b.to_i64() {
        if fits_small(i) {
            return small(i);
        }
    }
    let (sign, digits) = b.to_u64_digits();
    let n = digits.len();
    let p = ctx.alloc(2 + n);
    unsafe {
        *p = header(T_BIGINT, (1 + n) as u64);
        *p.add(1) = if sign == Sign::Minus { 1 } else { 0 };
        for (i, d) in digits.iter().enumerate() {
            *p.add(2 + i) = *d;
        }
    }
    p as Term
}

pub fn int_from_i128(ctx: &mut Ctx, v: i128) -> Term {
    if v >= SMALL_MIN as i128 && v <= SMALL_MAX as i128 {
        small(v as i64)
    } else {
        from_big(ctx, &BigInt::from(v))
    }
}

pub fn int_from_i64(ctx: &mut Ctx, v: i64) -> Term {
    if fits_small(v) {
        small(v)
    } else {
        from_big(ctx, &BigInt::from(v))
    }
}

pub fn int_to_f64(t: Term) -> f64 {
    if is_small(t) {
        small_val(t) as f64
    } else {
        to_big(t).to_f64().unwrap_or(f64::INFINITY)
    }
}

pub fn num_to_f64(t: Term) -> f64 {
    if is_float(t) {
        float_val(t)
    } else {
        int_to_f64(t)
    }
}

pub fn int_to_string(t: Term, base: u32) -> String {
    if is_small(t) {
        let v = small_val(t);
        if base == 10 {
            return v.to_string();
        }
        to_big(t).to_str_radix(base).to_uppercase()
    } else {
        to_big(t).to_str_radix(base).to_uppercase()
    }
}

pub fn parse_int(ctx: &mut Ctx, s: &str, base: u32) -> Option<Term> {
    if s.is_empty() {
        return None;
    }
    if base == 10 && s.len() < 18 {
        if let Ok(v) = s.parse::<i64>() {
            return Some(small(v));
        }
    }
    let (neg, digits) = if let Some(r) = s.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (false, r)
    } else {
        (false, s)
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(base)) {
        return None;
    }
    let b = BigInt::parse_bytes(digits.as_bytes(), base)?;
    let b = if neg { -b } else { b };
    Some(from_big(ctx, &b))
}

// ---------------- Float formatting (Erlang short / io_lib_format:fwrite_g) ---

/// Shortest round-trip digits and decimal exponent: value = 0.DIGITS * 10^place
fn shortest_digits(f: f64) -> (String, i32) {
    // Rust's LowerExp gives the shortest representation that round-trips.
    let s = format!("{:e}", f.abs());
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0').to_string();
    let digits = if digits.is_empty() { "0".to_string() } else { digits };
    (digits, exp + 1)
}

/// Format like `Float.to_string/1` (float_to_binary(F, [short])).
pub fn float_to_string(f: f64) -> String {
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let neg = f < 0.0;
    let (s, place) = shortest_digits(f);
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    let l = s.len() as i32;
    let big = f.abs() >= 9007199254740992.0; // 2^53
    let insert_exp = |out: &mut String| {
        let exp = place - 1;
        let mut chars = s.chars();
        out.push(chars.next().unwrap());
        out.push('.');
        let rest: String = chars.collect();
        if rest.is_empty() {
            out.push('0');
        } else {
            out.push_str(&rest);
        }
        out.push('e');
        out.push_str(&exp.to_string());
    };
    if big {
        insert_exp(&mut out);
        return out;
    }
    if place == 0 {
        out.push_str("0.");
        out.push_str(&s);
        return out;
    }
    if place < 0 || place >= l {
        let exp_l = (place - 1).to_string();
        let exp_dot = if l == 1 { 2 } else { 1 };
        let exp_cost = exp_l.len() as i32 + 1 + exp_dot;
        if place < 0 {
            if 2 - place <= exp_cost {
                out.push_str("0.");
                for _ in 0..(-place) {
                    out.push('0');
                }
                out.push_str(&s);
            } else {
                insert_exp(&mut out);
            }
        } else if place - l + 2 <= exp_cost {
            out.push_str(&s);
            for _ in 0..(place - l) {
                out.push('0');
            }
            out.push_str(".0");
        } else {
            insert_exp(&mut out);
        }
        return out;
    }
    let (a, b) = s.split_at(place as usize);
    out.push_str(a);
    out.push('.');
    out.push_str(b);
    out
}

/// Format like `inspect/1` on floats (Inspect.Float in Elixir 1.18).
pub fn float_inspect(f: f64) -> String {
    let abs = f.abs();
    if abs >= 1.0 && abs < 1.0e16 && f.trunc() == f {
        format!("{}.0", f as i64)
    } else {
        float_to_string(f)
    }
}

// ---------------- Arithmetic ---------------------------------------------------

pub enum ArithErr {
    BadArith,
}

pub fn add(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if is_small(a) && is_small(b) {
        return Ok(int_from_i64(ctx, small_val(a) + small_val(b)));
    }
    if is_integer(a) && is_integer(b) {
        let r = to_big(a) + to_big(b);
        return Ok(from_big(ctx, &r));
    }
    if is_number(a) && is_number(b) {
        return float_result(ctx, num_to_f64(a) + num_to_f64(b));
    }
    Err(ArithErr::BadArith)
}

pub fn sub(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if is_small(a) && is_small(b) {
        return Ok(int_from_i64(ctx, small_val(a) - small_val(b)));
    }
    if is_integer(a) && is_integer(b) {
        let r = to_big(a) - to_big(b);
        return Ok(from_big(ctx, &r));
    }
    if is_number(a) && is_number(b) {
        return float_result(ctx, num_to_f64(a) - num_to_f64(b));
    }
    Err(ArithErr::BadArith)
}

pub fn mul(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if is_small(a) && is_small(b) {
        return Ok(int_from_i128(ctx, small_val(a) as i128 * small_val(b) as i128));
    }
    if is_integer(a) && is_integer(b) {
        let r = to_big(a) * to_big(b);
        return Ok(from_big(ctx, &r));
    }
    if is_number(a) && is_number(b) {
        return float_result(ctx, num_to_f64(a) * num_to_f64(b));
    }
    Err(ArithErr::BadArith)
}

pub fn fdiv(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if is_number(a) && is_number(b) {
        let d = num_to_f64(b);
        if d == 0.0 {
            return Err(ArithErr::BadArith);
        }
        return float_result(ctx, num_to_f64(a) / d);
    }
    Err(ArithErr::BadArith)
}

fn float_result(ctx: &mut Ctx, f: f64) -> Result<Term, ArithErr> {
    if f.is_finite() {
        Ok(ctx.float(f))
    } else {
        Err(ArithErr::BadArith)
    }
}

/// Truncating integer division (Kernel.div/2).
pub fn idiv(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if is_small(a) && is_small(b) {
        let d = small_val(b);
        if d == 0 {
            return Err(ArithErr::BadArith);
        }
        return Ok(int_from_i64(ctx, small_val(a) / d));
    }
    if is_integer(a) && is_integer(b) {
        let d = to_big(b);
        if d.is_zero() {
            return Err(ArithErr::BadArith);
        }
        let q = to_big(a) / d; // truncates toward zero
        return Ok(from_big(ctx, &q));
    }
    Err(ArithErr::BadArith)
}

pub fn irem(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if is_small(a) && is_small(b) {
        let d = small_val(b);
        if d == 0 {
            return Err(ArithErr::BadArith);
        }
        return Ok(small(small_val(a) % d));
    }
    if is_integer(a) && is_integer(b) {
        let d = to_big(b);
        if d.is_zero() {
            return Err(ArithErr::BadArith);
        }
        let r = to_big(a) % d; // sign follows dividend
        return Ok(from_big(ctx, &r));
    }
    Err(ArithErr::BadArith)
}

/// Integer.floor_div / Integer.mod semantics
pub fn floor_div(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if !(is_integer(a) && is_integer(b)) {
        return Err(ArithErr::BadArith);
    }
    let d = to_big(b);
    if d.is_zero() {
        return Err(ArithErr::BadArith);
    }
    Ok(from_big(ctx, &to_big(a).div_floor(&d)))
}

pub fn modulo(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    if !(is_integer(a) && is_integer(b)) {
        return Err(ArithErr::BadArith);
    }
    let d = to_big(b);
    if d.is_zero() {
        return Err(ArithErr::BadArith);
    }
    Ok(from_big(ctx, &to_big(a).mod_floor(&d)))
}

pub fn neg(ctx: &mut Ctx, a: Term) -> Result<Term, ArithErr> {
    if is_small(a) {
        return Ok(int_from_i64(ctx, -small_val(a)));
    }
    if is_bigint(a) {
        return Ok(from_big(ctx, &-to_big(a)));
    }
    if is_float(a) {
        return Ok(ctx.float(-float_val(a)));
    }
    Err(ArithErr::BadArith)
}

pub fn abs(ctx: &mut Ctx, a: Term) -> Result<Term, ArithErr> {
    if is_small(a) {
        return Ok(int_from_i64(ctx, small_val(a).abs()));
    }
    if is_bigint(a) {
        return Ok(from_big(ctx, &to_big(a).abs()));
    }
    if is_float(a) {
        return Ok(ctx.float(float_val(a).abs()));
    }
    Err(ArithErr::BadArith)
}

pub fn pow_int(ctx: &mut Ctx, a: Term, b: Term) -> Result<Term, ArithErr> {
    let e = to_big(b);
    if e.is_negative() {
        return Err(ArithErr::BadArith);
    }
    let e = e.to_u32().ok_or(ArithErr::BadArith)?;
    let r = num_traits::pow(to_big(a), e as usize);
    Ok(from_big(ctx, &r))
}

pub fn bitop(ctx: &mut Ctx, op: u8, a: Term, b: Term) -> Result<Term, ArithErr> {
    if !(is_integer(a) && is_integer(b)) {
        return Err(ArithErr::BadArith);
    }
    if is_small(a) && is_small(b) {
        let (x, y) = (small_val(a), small_val(b));
        let r = match op {
            b'&' => x & y,
            b'|' => x | y,
            b'^' => x ^ y,
            b'<' => {
                if y < 0 {
                    return bitop(ctx, b'>', a, small(-y));
                }
                if y < 62 && (x.unsigned_abs() >> (62 - y)) == 0 {
                    x << y
                } else {
                    let r = to_big(a) << (y as usize);
                    return Ok(from_big(ctx, &r));
                }
            }
            b'>' => {
                if y < 0 {
                    return bitop(ctx, b'<', a, small(-y));
                }
                if y >= 63 {
                    if x < 0 {
                        -1
                    } else {
                        0
                    }
                } else {
                    x >> y
                }
            }
            _ => unreachable!(),
        };
        return Ok(int_from_i64(ctx, r));
    }
    let x = to_big(a);
    let r = match op {
        b'&' => x & to_big(b),
        b'|' => x | to_big(b),
        b'^' => x ^ to_big(b),
        b'<' | b'>' => {
            let s = to_big(b).to_i64().ok_or(ArithErr::BadArith)?;
            let left = (op == b'<') == (s >= 0);
            let s = s.unsigned_abs() as usize;
            if left {
                x << s
            } else {
                x >> s
            }
        }
        _ => unreachable!(),
    };
    Ok(from_big(ctx, &r))
}

pub fn bnot(ctx: &mut Ctx, a: Term) -> Result<Term, ArithErr> {
    if is_small(a) {
        return Ok(small(!small_val(a)));
    }
    if is_bigint(a) {
        return Ok(from_big(ctx, &!to_big(a)));
    }
    Err(ArithErr::BadArith)
}

pub fn cmp_int(a: Term, b: Term) -> std::cmp::Ordering {
    if is_small(a) && is_small(b) {
        return small_val(a).cmp(&small_val(b));
    }
    to_big(a).cmp(&to_big(b))
}

/// Compare an integer against a float numerically.
pub fn cmp_int_float(i: Term, f: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    if is_small(i) {
        let v = small_val(i);
        // Exact for |v| < 2^53
        if v.unsigned_abs() < (1u64 << 53) {
            return (v as f64).partial_cmp(&f).unwrap_or(Less);
        }
    }
    if f.is_nan() {
        return Less;
    }
    let fi = f.floor();
    let bf = {
        use num_traits::FromPrimitive;
        BigInt::from_f64(fi).unwrap_or_default()
    };
    let bi = to_big(i);
    match bi.cmp(&bf) {
        Equal => {
            if f > fi {
                Less
            } else {
                Equal
            }
        }
        o => o,
    }
}
