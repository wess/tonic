//! PCRE2 bindings backing the `:re` module (Erlang's re is PCRE too).
//!
//! Patterns are compiled once per (source, flags) and cached process-wide.

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::term::*;
use pcre2_sys::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

struct Code(*mut pcre2_code_8);
unsafe impl Send for Code {}
unsafe impl Sync for Code {}

struct Compiled {
    code: Code,
    ncaps: u32,
    /// (name, group number), sorted by name
    names: Vec<(Vec<u8>, u32)>,
}

type Cache = Mutex<HashMap<(Vec<u8>, u32), Result<std::sync::Arc<Compiled>, (String, usize)>>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

// Flag bits shared with lib/erlang_re (Elixir side).
const F_CASELESS: u32 = 1;
const F_MULTILINE: u32 = 2;
const F_DOTALL: u32 = 4;
const F_EXTENDED: u32 = 8;
const F_UNICODE: u32 = 16;
const F_UCP: u32 = 32;
const F_UNGREEDY: u32 = 64;
const F_FIRSTLINE: u32 = 128;
const F_ANCHORED: u32 = 256;
const F_DOLLAR_ENDONLY: u32 = 512;
const F_DUPNAMES: u32 = 1024;
const F_NO_AUTO_CAPTURE: u32 = 2048;
const F_NEWLINE_ANY: u32 = 4096;
const F_NEWLINE_CRLF: u32 = 8192;
const F_NEWLINE_ANYCRLF: u32 = 16384;
const F_NEWLINE_CR: u32 = 32768;

// Match flags
const M_NOTEMPTY: u32 = 1;
const M_NOTEMPTY_ATSTART: u32 = 2;
const M_ANCHORED: u32 = 4;
const M_NOTBOL: u32 = 8;
const M_NOTEOL: u32 = 16;

fn compile(src: &[u8], flags: u32) -> Result<std::sync::Arc<Compiled>, (String, usize)> {
    let key = (src.to_vec(), flags);
    if let Some(r) = cache().lock().unwrap().get(&key) {
        return r.clone();
    }
    let mut opts = 0u32;
    let map = [
        (F_CASELESS, PCRE2_CASELESS),
        (F_MULTILINE, PCRE2_MULTILINE),
        (F_DOTALL, PCRE2_DOTALL),
        (F_EXTENDED, PCRE2_EXTENDED),
        (F_UNICODE, PCRE2_UTF),
        (F_UCP, PCRE2_UCP),
        (F_UNGREEDY, PCRE2_UNGREEDY),
        (F_FIRSTLINE, PCRE2_FIRSTLINE),
        (F_ANCHORED, PCRE2_ANCHORED),
        (F_DOLLAR_ENDONLY, PCRE2_DOLLAR_ENDONLY),
        (F_DUPNAMES, PCRE2_DUPNAMES),
        (F_NO_AUTO_CAPTURE, PCRE2_NO_AUTO_CAPTURE),
    ];
    for (f, p) in map {
        if flags & f != 0 {
            opts |= p;
        }
    }
    let r = unsafe {
        let cctx = pcre2_compile_context_create_8(std::ptr::null_mut());
        let nl = if flags & F_NEWLINE_ANY != 0 {
            PCRE2_NEWLINE_ANY
        } else if flags & F_NEWLINE_CRLF != 0 {
            PCRE2_NEWLINE_CRLF
        } else if flags & F_NEWLINE_ANYCRLF != 0 {
            PCRE2_NEWLINE_ANYCRLF
        } else if flags & F_NEWLINE_CR != 0 {
            PCRE2_NEWLINE_CR
        } else {
            PCRE2_NEWLINE_LF
        };
        pcre2_set_newline_8(cctx, nl);
        let mut err: i32 = 0;
        let mut off: usize = 0;
        let code = pcre2_compile_8(src.as_ptr(), src.len(), opts, &mut err, &mut off, cctx);
        pcre2_compile_context_free_8(cctx);
        if code.is_null() {
            let mut buf = [0u8; 256];
            let n = pcre2_get_error_message_8(err, buf.as_mut_ptr(), buf.len());
            let msg = match err as u32 {
                PCRE2_ERROR_MISSING_CLOSING_PARENTHESIS => "missing )".to_string(),
                PCRE2_ERROR_UNMATCHED_CLOSING_PARENTHESIS => "unmatched parentheses".to_string(),
                PCRE2_ERROR_QUANTIFIER_INVALID => "nothing to repeat".to_string(),
                _ if n > 0 => String::from_utf8_lossy(&buf[..n as usize]).into_owned(),
                _ => "error".to_string(),
            };
            Err((msg, off))
        } else {
            pcre2_jit_compile_8(code, PCRE2_JIT_COMPLETE);
            let mut ncaps: u32 = 0;
            pcre2_pattern_info_8(code, PCRE2_INFO_CAPTURECOUNT, &mut ncaps as *mut u32 as *mut _);
            let mut count: u32 = 0;
            let mut esize: u32 = 0;
            let mut table: *const u8 = std::ptr::null();
            pcre2_pattern_info_8(code, PCRE2_INFO_NAMECOUNT, &mut count as *mut u32 as *mut _);
            pcre2_pattern_info_8(code, PCRE2_INFO_NAMEENTRYSIZE, &mut esize as *mut u32 as *mut _);
            pcre2_pattern_info_8(code, PCRE2_INFO_NAMETABLE, &mut table as *mut *const u8 as *mut _);
            let mut names = Vec::new();
            for i in 0..count as usize {
                let e = table.add(i * esize as usize);
                let num = ((*e as u32) << 8) | (*e.add(1) as u32);
                let mut nm = Vec::new();
                let mut k = 2;
                while k < esize as usize && *e.add(k) != 0 {
                    nm.push(*e.add(k));
                    k += 1;
                }
                names.push((nm, num));
            }
            names.sort();
            Ok(std::sync::Arc::new(Compiled { code: Code(code), ncaps, names }))
        }
    };
    cache().lock().unwrap().insert(key, r.clone());
    r
}

/// :tonic.pcre_compile(source, flags) ->
///   {:ok, ncaps, [{name, n}]} | {:error, {message, offset}}
#[no_mangle]
pub extern "C" fn tn_pcre_compile(c: *mut Ctx, src: Term, flags: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(src) || !is_small(flags) {
        return ctx.badarg("argument error");
    }
    match compile(bin_bytes(src), small_val(flags) as u32) {
        Ok(comp) => {
            let base = ctx.roots.len();
            for (n, i) in &comp.names {
                let b = ctx.binary(n);
                let bi = ctx.push(b);
                let t = ctx.tuple(&[ctx.get(bi), small(*i as i64)]);
                ctx.roots.truncate(bi);
                ctx.roots.push(t);
            }
            let l = ctx.list_from_roots(base, comp.names.len(), NIL_LIST);
            ctx.roots.truncate(base);
            let li = ctx.push(l);
            let t = ctx.tuple(&[atom(a::OK), small(comp.ncaps as i64), ctx.get(li)]);
            ctx.truncate(li);
            t
        }
        Err((msg, off)) => {
            let m = ctx.str(&msg);
            let mi = ctx.push(m);
            let inner = ctx.tuple(&[ctx.get(mi), small(off as i64)]);
            ctx.set(mi, inner);
            let t = ctx.tuple(&[atom(a::ERROR), ctx.get(mi)]);
            ctx.truncate(mi);
            t
        }
    }
}

/// :tonic.pcre_match(source, flags, subject, offset, match_flags) ->
///   nil | [{start, len}] (one per group up to the last set one; unset {-1, 0})
#[no_mangle]
pub extern "C" fn tn_pcre_match(c: *mut Ctx, src: Term, flags: Term, subj: Term, off: Term, mflags: Term) -> Term {
    let ctx = cx(c);
    if !is_binary(src) || !is_small(flags) || !is_binary(subj) || !is_small(off) || !is_small(mflags) {
        return ctx.badarg("argument error");
    }
    let comp = match compile(bin_bytes(src), small_val(flags) as u32) {
        Ok(c) => c,
        Err(_) => return ctx.badarg("argument error"),
    };
    let s = bin_bytes(subj);
    let offset = small_val(off).max(0) as usize;
    if offset > s.len() {
        return ctx.badarg("argument error");
    }
    let mf = small_val(mflags) as u32;
    let mut o = 0u32;
    if mf & M_NOTEMPTY != 0 {
        o |= PCRE2_NOTEMPTY;
    }
    if mf & M_NOTEMPTY_ATSTART != 0 {
        o |= PCRE2_NOTEMPTY_ATSTART;
    }
    if mf & M_ANCHORED != 0 {
        o |= PCRE2_ANCHORED;
    }
    if mf & M_NOTBOL != 0 {
        o |= PCRE2_NOTBOL;
    }
    if mf & M_NOTEOL != 0 {
        o |= PCRE2_NOTEOL;
    }
    let pairs: Vec<(i64, i64)> = unsafe {
        let md = pcre2_match_data_create_from_pattern_8(comp.code.0, std::ptr::null_mut());
        let rc = pcre2_match_8(comp.code.0, s.as_ptr(), s.len(), offset, o, md, std::ptr::null_mut());
        let r = if rc <= 0 {
            None
        } else {
            let ov = pcre2_get_ovector_pointer_8(md);
            let mut v = Vec::with_capacity(rc as usize);
            for i in 0..rc as usize {
                let a = *ov.add(2 * i);
                let b = *ov.add(2 * i + 1);
                if a == usize::MAX {
                    v.push((-1, 0));
                } else {
                    v.push((a as i64, (b - a) as i64));
                }
            }
            Some(v)
        };
        pcre2_match_data_free_8(md);
        match r {
            Some(v) => v,
            None => return NIL,
        }
    };
    let base = ctx.roots.len();
    for &(st, ln) in &pairs {
        let t = ctx.tuple(&[small(st), small(ln)]);
        ctx.roots.push(t);
    }
    let l = ctx.list_from_roots(base, pairs.len(), NIL_LIST);
    ctx.roots.truncate(base);
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_errors_preserve_offsets_and_elixir_messages() {
        for (pattern, message, offset) in [
            ("(", "missing )", 1),
            (")", "unmatched parentheses", 0),
            ("*", "nothing to repeat", 0),
            ("a**", "nothing to repeat", 2),
            ("[", "missing terminating ] for character class", 1),
            ("[z-a]", "range out of order in character class", 3),
        ] {
            let error = compile(pattern.as_bytes(), 0).err().unwrap();
            assert_eq!(error, (message.to_string(), offset), "{pattern}");
        }
    }

    #[test]
    fn regex_flags_and_named_captures_survive_caching() {
        let pattern = b"(?<word>caf\xc3\xa9)(?=!)";
        let first = compile(pattern, F_UNICODE | F_CASELESS).unwrap();
        let second = compile(pattern, F_UNICODE | F_CASELESS).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.ncaps, 1);
        assert_eq!(first.names, vec![(b"word".to_vec(), 1)]);
        let sensitive = compile(pattern, F_UNICODE).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &sensitive));
    }
}
