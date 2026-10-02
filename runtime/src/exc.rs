//! Exceptions. A raised exception is stored in the process context and the
//! raising function returns `NONE` (0); compiled code propagates 0 upward
//! until a `try` handler or the process top.

use crate::heap::Ctx;
use crate::map;
use crate::term::*;

impl Ctx {
    #[inline]
    pub fn raise(&mut self, kind: Term, reason: Term) -> Term {
        self.exc_kind = kind;
        self.exc_reason = reason;
        self.exc_stack = NIL_LIST;
        self.raise_frame = self.frame;
        self.pending_args = 0;
        let st = self.capture_stacktrace();
        self.exc_stack = st;
        NONE
    }

    /// Build `[{module, fun, arity, [file: ~c"...", line: n]}]` from the
    /// shadow-stack frames (innermost first, at most 8 entries like the BEAM's
    /// default backtrace depth).
    pub fn capture_stacktrace(&mut self) -> Term {
        self.capture_stacktrace_skip(0)
    }

    /// As capture_stacktrace, first dropping up to `skip_erlang` innermost
    /// frames of the `erlang` module (tonic's Elixir-implemented BIFs, which
    /// are not on the BEAM's stack).
    pub fn capture_stacktrace_skip(&mut self, skip_erlang: usize) -> Term {
        #[repr(C)]
        struct FInfo {
            module: u64,
            name: u64,
            arity: u64,
            file: *const std::ffi::c_char,
        }
        let mut entries: Vec<(u64, u64, u64, String, u64)> = Vec::new();
        let erlang = atom(crate::atoms::intern("erlang"));
        let mut skip = skip_erlang;
        unsafe {
            let mut f = self.frame;
            while !f.is_null() && entries.len() < 8 {
                let info = *f.add(2) as *const FInfo;
                if !info.is_null() && skip > 0 && (*info).module == erlang && entries.is_empty() {
                    skip -= 1;
                    f = *f as *mut u64;
                    continue;
                }
                skip = 0;
                if !info.is_null() {
                    let fi = &*info;
                    let file = if fi.file.is_null() {
                        String::new()
                    } else {
                        std::ffi::CStr::from_ptr(fi.file).to_string_lossy().into_owned()
                    };
                    entries.push((fi.module, fi.name, fi.arity, file, *f.add(3)));
                }
                f = *f as *mut u64;
            }
        }
        let k_file = atom(crate::atoms::intern("file"));
        let k_line = atom(crate::atoms::intern("line"));
        let base = self.roots.len();
        for (m, n, ar, file, line) in &entries {
            let mut chars: Vec<Term> = file.chars().map(|c| small(c as i64)).collect();
            let fl = self.list_from_vec(&mut chars);
            self.roots.push(fl);
            let ft = self.tuple(&[k_file, fl]);
            self.roots.pop();
            self.roots.push(ft);
            let loc = if *line > 0 {
                let lt = self.tuple(&[k_line, small(*line as i64)]);
                let l2 = self.cons(lt, NIL_LIST);
                let ft = self.roots.pop().unwrap();
                self.cons(ft, l2)
            } else {
                let ft = self.roots.pop().unwrap();
                self.cons(ft, NIL_LIST)
            };
            let e = self.tuple(&[*m, *n, small(*ar as i64), loc]);
            self.roots.push(e);
        }
        let l = self.list_from_roots(base, entries.len(), NIL_LIST);
        self.roots.truncate(base);
        l
    }

    pub fn raise_error(&mut self, reason: Term) -> Term {
        self.raise(atom(a::ERROR), reason)
    }

    /// Build `%Module{field: value, ...}` (fields given as atom indices).
    pub fn mk_struct(&mut self, module: u64, fields: &[(u64, Term)], exception: bool) -> Term {
        let base = self.roots.len();
        self.roots.push(atom(a::STRUCT));
        self.roots.push(atom(module));
        if exception {
            self.roots.push(atom(a::EXCEPTION));
            self.roots.push(TRUE);
        }
        for &(k, v) in fields {
            self.roots.push(atom(k));
            self.roots.push(v);
        }
        let n = (self.roots.len() - base) / 2;
        let m = map::from_root_pairs(self, base, n);
        self.roots.truncate(base);
        m
    }

    pub fn raise_exception(&mut self, module: u64, fields: &[(u64, Term)]) -> Term {
        let e = self.mk_struct(module, fields, true);
        self.raise_error(e)
    }

    pub fn raise_msg(&mut self, module: u64, msg: &str) -> Term {
        let m = self.str(msg);
        self.raise_exception(module, &[(a::MESSAGE, m)])
    }

    /// `:badarg` (plain "argument error"), or an ArgumentError with a custom
    /// message (as raised by Elixir code).
    pub fn badarg(&mut self, msg: &str) -> Term {
        if msg == "argument error" {
            return self.raise_error(atom(a::BADARG));
        }
        self.raise_msg(a::ARGUMENT_ERROR, msg)
    }

    /// `:badarg` from a BIF with OTP's per-argument error_info: the
    /// descriptions travel in the BIF's top stack frame (`tn_bif_frame`) and
    /// `:erl_erts_errors.format_error/2` hands them to ErlangError.normalize.
    pub fn badarg_args(&mut self, errs: &[(usize, &str)]) -> Term {
        self.raise_error(atom(a::BADARG));
        let base = self.roots.len();
        for (i, m) in errs {
            self.roots.push(small(*i as i64));
            let s = self.str(m);
            self.roots.push(s);
        }
        let mp = map::from_root_pairs(self, base, errs.len());
        self.roots.truncate(base);
        self.pending_args = mp;
        NONE
    }

    /// Errors for `:erlang.element/2`-style BIFs (1-based `index`, `tuple`).
    pub fn badarg_element(&mut self, index: Term, tuple: Term, ipos: usize, tpos: usize) -> Term {
        let mut errs: Vec<(usize, &str)> = Vec::new();
        if !is_small(index) && !is_bigint(index) {
            errs.push((ipos, "not an integer"));
        } else if !is_small(index) || small_val(index) <= 0 || (is_tuple(tuple) && small_val(index) as usize > tuple_size(tuple)) {
            errs.push((ipos, "out of range"));
        }
        if !is_tuple(tuple) {
            errs.push((tpos, "not a tuple"));
        }
        errs.sort();
        self.badarg_args(&errs)
    }

    pub fn badarith(&mut self) -> Term {
        self.raise_error(atom(a::BADARITH))
    }

    fn raise_tuple(&mut self, items: &[Term]) -> Term {
        let t = self.tuple(items);
        self.raise_error(t)
    }

    /// badarith from an `:erlang` operator: like the BEAM, the stacktrace
    /// starts with `{:erlang, op, args, [error_info: ...]}`.
    pub fn badarith_op(&mut self, op: &str, args: &[Term]) -> Term {
        let base = self.roots.len();
        for &a in args {
            self.roots.push(a);
        }
        self.badarith();
        let al = self.list_from_roots(base, args.len(), NIL_LIST);
        self.roots.truncate(base);
        self.push_top_frame(atom(crate::atoms::intern("erlang")), atom(crate::atoms::intern(op)), al);
        NONE
    }

    /// Argument errors raised by a native helper called from one of tonic's
    /// Elixir-implemented Erlang modules (`:ets.lookup/2` calling into the
    /// runtime) are not seen by `tn_bif_frame`; when the exception is taken,
    /// attach their error_info to that module's frame (file `*.erl`).
    pub fn flush_pending_args(&mut self) {
        let pa = self.pending_args;
        if pa == 0 {
            return;
        }
        self.pending_args = 0;
        let st = self.exc_stack;
        if st == 0 || !is_cons(st) {
            return;
        }
        let top = head(st);
        if !is_tuple(top) || tuple_size(top) != 4 {
            return;
        }
        let meta = tuple_get(top, 3);
        let mut is_erl = false;
        let mut l = meta;
        while is_cons(l) {
            let kv = head(l);
            if is_tuple(kv) && tuple_size(kv) == 2 && tuple_get(kv, 0) == atom(crate::atoms::intern("file")) {
                let mut name = Vec::new();
                let mut c = tuple_get(kv, 1);
                while is_cons(c) {
                    if is_small(head(c)) {
                        name.push(small_val(head(c)) as u8);
                    }
                    c = tail(c);
                }
                is_erl = name.ends_with(b".erl");
            }
            l = tail(l);
        }
        if !is_erl {
            return;
        }
        let si = self.push(st);
        self.push(pa);
        let k_mod = atom(crate::atoms::intern("module"));
        let v_mod = atom(crate::atoms::intern("erl_erts_errors"));
        let mp = crate::map::put(self, crate::map::empty(), k_mod, v_mod);
        let k = atom(crate::atoms::intern("tonic_args"));
        let mp = crate::map::put(self, mp, k, self.get(si + 1));
        let mi = self.push(mp);
        let ei = self.tuple(&[atom(crate::atoms::intern("error_info")), self.get(mi)]);
        self.set(mi, ei);
        let meta = self.cons(self.get(mi), NIL_LIST);
        self.set(mi, meta);
        let top = head(self.get(si));
        let e = self.tuple(&[tuple_get(top, 0), tuple_get(top, 1), tuple_get(top, 2), self.get(mi)]);
        self.set(mi, e);
        let ns = self.cons(self.get(mi), tail(self.get(si)));
        self.exc_stack = ns;
        self.truncate(si);
    }

    /// Prepends `{m, f, args, [error_info: %{module: :erl_erts_errors}]}`
    /// to the current exception stacktrace.
    pub fn push_top_frame(&mut self, m: Term, f: Term, args: Term) {
        let ai = self.push(args);
        let k_mod = atom(crate::atoms::intern("module"));
        let v_mod = atom(crate::atoms::intern("erl_erts_errors"));
        let mut mp = crate::map::put(self, crate::map::empty(), k_mod, v_mod);
        if self.pending_args != 0 {
            let k = atom(crate::atoms::intern("tonic_args"));
            let vi = self.push(self.pending_args);
            self.pending_args = 0;
            let mi = self.push(mp);
            mp = crate::map::put(self, self.get(mi), k, self.get(vi));
            self.truncate(vi);
        }
        let mi = self.push(mp);
        let ei = self.tuple(&[atom(crate::atoms::intern("error_info")), self.get(mi)]);
        self.set(mi, ei);
        let meta = self.cons(self.get(mi), NIL_LIST);
        self.set(mi, meta);
        let e = self.tuple(&[m, f, self.get(ai), self.get(mi)]);
        self.set(mi, e);
        let st = self.cons(self.get(mi), self.exc_stack);
        self.exc_stack = st;
        self.truncate(ai);
    }

    pub fn badmap(&mut self, t: Term) -> Term {
        self.raise_tuple(&[atom(a::BADMAP), t])
    }

    pub fn badkey(&mut self, key: Term, t: Term) -> Term {
        self.raise_tuple(&[atom(a::BADKEY), key, t])
    }

    pub fn match_error(&mut self, t: Term) -> Term {
        self.raise_tuple(&[atom(crate::atoms::intern("badmatch")), t])
    }

    pub fn case_clause(&mut self, t: Term) -> Term {
        self.raise_tuple(&[atom(crate::atoms::intern("case_clause")), t])
    }

    pub fn with_clause(&mut self, t: Term) -> Term {
        self.raise_tuple(&[atom(crate::atoms::intern("else_clause")), t])
    }

    pub fn try_clause(&mut self, t: Term) -> Term {
        self.raise_tuple(&[atom(crate::atoms::intern("try_clause")), t])
    }

    pub fn cond_clause(&mut self) -> Term {
        self.raise_error(atom(crate::atoms::intern("cond_clause")))
    }

    pub fn bad_function(&mut self, t: Term) -> Term {
        self.raise_tuple(&[atom(crate::atoms::intern("badfun")), t])
    }

    pub fn bad_arity(&mut self, f: Term, args: Term) -> Term {
        let ai = self.push(args);
        let fi = self.push(f);
        let inner = self.tuple(&[self.get(fi), self.get(ai)]);
        self.truncate(ai);
        self.raise_tuple(&[atom(crate::atoms::intern("badarity")), inner])
    }

    pub fn bad_boolean(&mut self, t: Term, op: Term) -> Term {
        self.raise_tuple(&[atom(crate::atoms::intern("badbool")), op, t])
    }

    /// `:undef`, with `{m, f, arity, []}` on top of the stacktrace as on the BEAM.
    pub fn undef(&mut self, m: Term, f: Term, arity: u64) -> Term {
        self.raise_error(atom(crate::atoms::intern("undef")));
        let e = self.tuple(&[m, f, small(arity as i64), NIL_LIST]);
        let st = self.cons(e, self.exc_stack);
        self.exc_stack = st;
        NONE
    }

    pub fn function_clause(&mut self, m: Term, f: Term, arity: u64, args: Term) -> Term {
        let ai = self.push(args);
        let _ = arity;
        self.raise_error(atom(crate::atoms::intern("function_clause")));
        // As on the BEAM, the failing call's stack entry carries its
        // arguments instead of the arity.
        let st = self.exc_stack;
        if is_cons(st) {
            let top = head(st);
            if is_tuple(top) && tuple_size(top) == 4 && tuple_get(top, 0) == m && tuple_get(top, 1) == f {
                let si = self.push(st);
                let e = self.tuple(&[m, f, self.get(ai), tuple_get(head(self.get(si)), 3)]);
                let ns = self.cons(e, tail(self.get(si)));
                self.exc_stack = ns;
            }
        }
        self.truncate(ai);
        NONE
    }

    pub fn system_limit(&mut self) -> Term {
        self.raise_error(atom(crate::atoms::intern("system_limit")))
    }
}

pub fn ordinal(n: usize) -> String {
    match n {
        1 => "1st".into(),
        2 => "2nd".into(),
        3 => "3rd".into(),
        n => format!("{}th", n),
    }
}

pub fn badarg_args_msg(errs: &[(usize, &str)]) -> String {
    if errs.is_empty() {
        return "argument error".into();
    }
    let mut m = String::from("errors were found at the given arguments:\n\n");
    for (n, e) in errs {
        m.push_str(&format!("  * {} argument: {}\n", ordinal(*n), e));
    }
    m
}
