//! Unicode helpers backing :unicode_util.gc, :string.titlecase,
//! String.Unicode case mapping and :unicode normalization.

use crate::bif_bin::{decode_utf8, graphemes};
use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::term::*;

/// :unicode_util.gc(binary) -> [] | [cp_or_cps | rest] | {:error, binary}
#[no_mangle]
pub extern "C" fn tn_unicode_gc(c: *mut Ctx, b: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) {
        return ctx.badarg("argument error");
    }
    let s = bin_bytes(b);
    if s.is_empty() {
        return NIL_LIST;
    }
    let (c1, l1) = match decode_utf8(s) {
        Some(x) => x,
        None => return ctx.tuple(&[atom(a::ERROR), b]),
    };
    // Fast path (as unicode_util does): two Latin-1 codepoints never join
    // unless the first is CR.
    let fast = c1 < 256
        && c1 != 13
        && (l1 == s.len() || decode_utf8(&s[l1..]).map(|(c2, _)| c2 < 256).unwrap_or(true));
    let end = if fast {
        l1
    } else {
        // First cluster only: segment a bounded prefix.
        let lim = s.len().min(64);
        let mut e = graphemes(&s[..lim]).first().map(|r| r.1).unwrap_or(1);
        if e == lim && lim < s.len() {
            e = graphemes(s).first().map(|r| r.1).unwrap_or(1);
        }
        e
    };
    let mut cps = Vec::new();
    let mut i = 0;
    while i < end {
        let (cp, l) = decode_utf8(&s[i..end]).unwrap_or((s[i] as u32, 1));
        cps.push(cp);
        i += l;
    }
    let bi = ctx.push(b);
    let rest = if end == s.len() { NIL_LIST } else { ctx.sub_binary(ctx.get(bi), end, s.len() - end) };
    let ri = ctx.push(rest);
    let head = if cps.len() == 1 {
        small(cps[0] as i64)
    } else {
        let mut v: Vec<Term> = cps.iter().map(|&c| small(c as i64)).collect();
        ctx.list_from_vec(&mut v)
    };
    let hi = ctx.push(head);
    let l = ctx.cons(ctx.get(hi), ctx.get(ri));
    ctx.truncate(bi);
    l
}

fn title_special(cp: u32) -> Option<&'static [u32]> {
    Some(match cp {
        0xDF => &[0x53, 0x73],
        0xFB00 => &[0x46, 0x66],
        0xFB01 => &[0x46, 0x69],
        0xFB02 => &[0x46, 0x6C],
        0xFB03 => &[0x46, 0x66, 0x69],
        0xFB04 => &[0x46, 0x66, 0x6C],
        0xFB05 | 0xFB06 => &[0x53, 0x74],
        0x587 => &[0x535, 0x582],
        0x149 => &[0x2BC, 0x4E],
        0x1C4..=0x1C6 => &[0x1C5],
        0x1C7..=0x1C9 => &[0x1C8],
        0x1CA..=0x1CC => &[0x1CB],
        0x1F1..=0x1F3 => &[0x1F2],
        _ => return None,
    })
}

/// Titlecase mapping of one codepoint -> list of codepoints.
#[no_mangle]
pub extern "C" fn tn_char_titlecase(c: *mut Ctx, cp: Term) -> Term {
    let ctx = cx(c);
    if !is_small(cp) {
        return ctx.badarg("argument error");
    }
    let v = small_val(cp) as u32;
    let out: Vec<u32> = match title_special(v) {
        Some(s) => s.to_vec(),
        None => match char::from_u32(v) {
            Some(ch) => ch.to_uppercase().map(|c| c as u32).collect(),
            None => vec![v],
        },
    };
    let mut t: Vec<Term> = out.iter().map(|&c| small(c as i64)).collect();
    ctx.list_from_vec(&mut t)
}

fn is_cased_letter(ch: char) -> bool {
    ch.is_lowercase() || ch.is_uppercase()
}

/// Approximation of the Unicode Case_Ignorable property.
fn is_case_ignorable(cp: u32) -> bool {
    matches!(
        cp,
        0x27 | 0x2E | 0x3A | 0x5E | 0x60 | 0xA8 | 0xAD | 0xAF | 0xB4 | 0xB7 | 0xB8
            | 0x2B0..=0x36F | 0x374 | 0x375 | 0x37A | 0x384 | 0x385 | 0x387
            | 0x483..=0x489 | 0x591..=0x5BD | 0x5BF | 0x5C1 | 0x5C2 | 0x5C4 | 0x5C5 | 0x5C7 | 0x5F4
            | 0x600..=0x605 | 0x610..=0x61A | 0x61C | 0x640 | 0x64B..=0x65F | 0x670
            | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200B..=0x200F | 0x2018 | 0x2019 | 0x2024 | 0x2027
            | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F | 0x2071 | 0x207F | 0x2090..=0x209C
            | 0x20D0..=0x20F0 | 0xFE00..=0xFE0F | 0xFE13 | 0xFE20..=0xFE2F | 0xFE52 | 0xFE55
            | 0xFEFF | 0xFF07 | 0xFF0E | 0xFF1A | 0xFF3E | 0xFF40 | 0xFF70 | 0xFF9E | 0xFF9F | 0xFFE3
    )
}

/// Sigma context as String.Unicode.downcase/3 sees it: the accumulator holds
/// ASCII as integers (never "cased"), so look at the previous non-ignorable
/// non-ASCII character; the rest is scanned skipping case-ignorables.
fn sigma_final(s: &[u8], i: usize, l: usize) -> bool {
    let before = match std::str::from_utf8(&s[..i]) {
        Ok(p) => {
            let mut r = false;
            for ch in p.chars().rev() {
                if (ch as u32) < 0x80 {
                    break;
                }
                if is_case_ignorable(ch as u32) {
                    continue;
                }
                r = is_cased_letter(ch);
                break;
            }
            r
        }
        Err(_) => false,
    };
    let after = match std::str::from_utf8(&s[i + l..]) {
        Ok(p) => {
            let mut r = false;
            for ch in p.chars() {
                if is_case_ignorable(ch as u32) {
                    continue;
                }
                r = is_cased_letter(ch);
                break;
            }
            r
        }
        Err(_) => false,
    };
    before && !after
}

/// String.Unicode.upcase/downcase. up: true/false; mode 0 default, 1 greek, 2 turkic.
#[no_mangle]
pub extern "C" fn tn_ucase(c: *mut Ctx, b: Term, up: Term, mode: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(mode) {
        return ctx.badarg("argument error");
    }
    let s = bin_bytes(b);
    let mode = small_val(mode);
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    let mut buf = [0u8; 4];
    while i < s.len() {
        let (cp, l) = match decode_utf8(&s[i..]) {
            Some(x) => x,
            None => {
                out.push(s[i]);
                i += 1;
                continue;
            }
        };
        let ch = char::from_u32(cp).unwrap();
        if up == TRUE {
            if mode == 2 && ch == 'i' {
                out.extend_from_slice("İ".as_bytes());
            } else {
                for u in ch.to_uppercase() {
                    out.extend_from_slice(u.encode_utf8(&mut buf).as_bytes());
                }
            }
        } else if ch == 'I' && s[i + l..].starts_with("\u{307}".as_bytes()) {
            out.extend_from_slice(if mode == 2 { "i" } else { "i\u{307}" }.as_bytes());
            i += l + 2;
            continue;
        } else if mode == 2 && ch == 'I' {
            out.extend_from_slice("ı".as_bytes());
        } else if mode == 2 && ch == 'İ' {
            out.push(b'i');
        } else if mode == 1 && ch == 'Σ' {
            // Final sigma: preceded by a cased letter and not followed by one.
            out.extend_from_slice(if sigma_final(s, i, l) { "ς" } else { "σ" }.as_bytes());
        } else if ch == 'İ' {
            out.extend_from_slice("i̇".as_bytes());
        } else {
            for u in ch.to_lowercase() {
                out.extend_from_slice(u.encode_utf8(&mut buf).as_bytes());
            }
        }
        i += l;
    }
    ctx.binary(&out)
}

/// :unicode normalization of a UTF-8 binary. form: 0 nfc, 1 nfd, 2 nfkc, 3 nfkd
#[no_mangle]
pub extern "C" fn tn_unorm(c: *mut Ctx, b: Term, form: Term) -> Term {
    use unicode_normalization::UnicodeNormalization;
    let ctx = cx(c);
    if !is_binary(b) || !is_small(form) {
        return ctx.badarg("argument error");
    }
    let s = match std::str::from_utf8(bin_bytes(b)) {
        Ok(s) => s,
        Err(_) => return ctx.tuple(&[atom(a::ERROR), b, b]),
    };
    let r: String = match small_val(form) {
        0 => s.nfc().collect(),
        1 => s.nfd().collect(),
        2 => s.nfkc().collect(),
        _ => s.nfkd().collect(),
    };
    ctx.str(&r)
}

/// Bytes remaining after skipping `n` graphemes of a binary.
#[no_mangle]
pub extern "C" fn tn_str_remaining_at(c: *mut Ctx, b: Term, n: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(b) || !is_small(n) {
        return ctx.badarg("argument error");
    }
    let s = bin_bytes(b);
    let n = small_val(n).max(0) as usize;
    if n == 0 {
        return small(s.len() as i64);
    }
    // ASCII run fast path
    let mut i = 0;
    let mut k = 0;
    while k < n && i + 1 < s.len() && s[i] < 0x80 && s[i] != b'\r' && s[i + 1] < 0x80 {
        i += 1;
        k += 1;
    }
    if k == n {
        return small((s.len() - i) as i64);
    }
    let g = graphemes(&s[i..]);
    let left = n - k;
    if left >= g.len() {
        return small(0);
    }
    small((s.len() - i - g[left].0) as i64)
}
