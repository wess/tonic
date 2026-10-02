//! Expression, pattern and guard expansion into Core IR.

use crate::ast::*;
use crate::bifs;
use crate::collect::*;
use crate::core::Clause;
use crate::core::*;
use crate::expand::*;
use std::collections::HashMap;
use std::rc::Rc;

type Bound = HashMap<String, VarId>;

pub fn int_lit(s: &str) -> Lit {
    match s.parse::<i64>() {
        Ok(v) if (-(1i64 << 62)..(1i64 << 62)).contains(&v) => Lit::Int(v),
        _ => Lit::Big(s.to_string()),
    }
}

fn is_erlang_mod(m: &str) -> bool {
    !m.starts_with("Elixir.") && m != "tonic"
}

enum Resolved {
    Fun(FunKey),
    Bif(bifs::Bif),
    Guard(String),
}

impl Expander {
    // ------------------------------------------------------------------
    // scopes
    // ------------------------------------------------------------------

    fn lookup(&self, cx: &Cx, name: &str) -> Option<VarId> {
        for s in cx.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(*v);
            }
        }
        None
    }

    fn commit(&self, cx: &mut Cx, b: Bound) {
        let top = cx.scopes.last_mut().unwrap();
        for (k, v) in b {
            top.insert(k, v);
        }
    }

    fn scoped<T>(&mut self, cx: &mut Cx, f: impl FnOnce(&mut Self, &mut Cx) -> R<T>) -> R<T> {
        cx.scopes.push(HashMap::new());
        let r = f(self, cx);
        cx.scopes.pop();
        r
    }

    pub fn guard(&mut self, e: &E, cx: &mut Cx) -> R<CE> {
        // `when a when b`: independent guards; an exception in `a` only fails `a`.
        if let K::Bin { op, l, r } = &e.k {
            if op == "when" {
                let a = self.guard(l, cx)?;
                let b = self.guard(r, cx)?;
                return Ok(CE::bif("__gor", vec![a, b]));
            }
        }
        let g = cx.guard;
        cx.guard = true;
        let r = self.expr(e, cx);
        cx.guard = g;
        r
    }

    fn body(&mut self, e: &E, cx: &mut Cx) -> R<CE> {
        self.scoped(cx, |s, cx| s.expr(e, cx))
    }

    // ------------------------------------------------------------------
    // expressions
    // ------------------------------------------------------------------

    pub fn expr(&mut self, e: &E, cx: &mut Cx) -> R<CE> {
        let ce = self.expr_inner(e, cx)?;
        // Record the source line of calls for stack traces.
        if !cx.guard
            && e.line != 0
            && matches!(
                e.k,
                K::Call { .. }
                    | K::Remote { .. }
                    | K::AnonCall { .. }
                    | K::Bin { .. }
                    | K::Un { .. }
            )
            && matches!(
                ce,
                CE::Call(..) | CE::DynCall(..) | CE::Apply(..) | CE::Bif(..)
            )
        {
            if let CE::Bif(n, _) = &ce {
                if n.starts_with("__") || crate::bifs::nofail_sym(n) {
                    return Ok(ce);
                }
            }
            return Ok(CE::Bif("__at".into(), vec![CE::int(e.line as i64), ce]));
        }
        Ok(ce)
    }

    fn expr_inner(&mut self, e: &E, cx: &mut Cx) -> R<CE> {
        let line = e.line;
        match &e.k {
            K::Int(s) => Ok(CE::Lit(int_lit(s))),
            K::Float(f) => Ok(CE::Lit(Lit::Float(*f))),
            K::Atom(a) => Ok(CE::atom(a)),
            K::Str(b) => Ok(CE::Lit(Lit::Bin(b.clone()))),
            K::Interp(parts) => self.interp(parts, cx),
            K::Charlist(parts) => {
                if parts.iter().all(|p| matches!(p, IPart::Lit(_))) {
                    let mut bytes = Vec::new();
                    for p in parts {
                        if let IPart::Lit(b) = p {
                            bytes.extend(b);
                        }
                    }
                    Ok(charlist_lit(&bytes))
                } else {
                    let i = self.interp(parts, cx)?;
                    Ok(CE::bif("tn_str_to_charlist", vec![i]))
                }
            }
            K::AtomInterp(parts) => {
                let i = self.interp(parts, cx)?;
                Ok(CE::bif("tn_binary_to_atom", vec![i]))
            }
            K::Var(name) => self.var_expr(name, line, cx),
            K::Alias(a) => Ok(CE::atom(&resolve_alias(&cx.env, a))),
            K::Call { name, args, parens } => self.call(name, args, *parens, line, cx),
            K::Remote {
                recv,
                name,
                args,
                parens,
            } => self.remote(recv, name, args, *parens, line, cx),
            K::AnonCall { f, args } => {
                if cx.guard {
                    return err(cx, line, "cannot invoke anonymous functions inside guards");
                }
                let fe = self.expr(f, cx)?;
                let a = self.exprs(args, cx)?;
                Ok(CE::Apply(Box::new(fe), a))
            }
            K::Access { e: x, key } => {
                let xe = self.expr(x, cx)?;
                let k = self.expr(key, cx)?;
                Ok(CE::bif("tn_access_get", vec![xe, k]))
            }
            K::Bin { op, l, r } => self.binop(op, l, r, line, cx),
            K::Un { op, e: x } => self.unop(op, x, line, cx),
            K::Tuple(items) => Ok(CE::Tuple(self.exprs(items, cx)?)),
            K::List(items, tail) => {
                let mut all: Vec<E> = items.clone();
                if let Some(t) = tail {
                    all.push((**t).clone());
                }
                let mut items = self.exprs(&all, cx)?;
                let mut acc = if tail.is_some() {
                    items.pop().unwrap()
                } else {
                    CE::Lit(Lit::Nil)
                };
                for it in items.into_iter().rev() {
                    acc = CE::Cons(Box::new(it), Box::new(acc));
                }
                Ok(acc)
            }
            K::Map(pairs) => {
                let flat: Vec<E> = pairs
                    .iter()
                    .flat_map(|(k, v)| [k.clone(), v.clone()])
                    .collect();
                let mut it = self.exprs(&flat, cx)?.into_iter();
                let mut ps = Vec::new();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    ps.push((k, v));
                }
                Ok(CE::Map(ps))
            }
            K::MapUpd(base, pairs) => {
                let b = self.expr(base, cx)?;
                let mut ps = Vec::new();
                for (k, v) in pairs {
                    ps.push((self.expr(k, cx)?, self.expr(v, cx)?));
                }
                Ok(CE::MapUpdate(Box::new(b), ps, true))
            }
            K::Struct { name, base, pairs } => {
                self.struct_expr(name, base.as_deref(), pairs, line, cx)
            }
            K::Bits(segs) => {
                let mut out = Vec::new();
                for s in segs {
                    out.push(self.bin_seg_expr(s, cx)?);
                }
                Ok(CE::Bin(out))
            }
            K::Fn(clauses) => self.lambda(clauses, line, cx),
            K::Block(es) => {
                if es.is_empty() {
                    return Ok(CE::nil());
                }
                let mut out = Vec::new();
                for x in es {
                    out.push(self.expr(x, cx)?);
                }
                if out.len() == 1 {
                    Ok(out.pop().unwrap())
                } else {
                    Ok(CE::Block(out))
                }
            }
            K::Capture(inner) => self.capture(inner, line, cx),
            K::CapArg(n) => match &cx.cap {
                Some(ps) if (*n as usize) <= ps.len() => Ok(CE::Var(ps[*n as usize - 1])),
                _ => err(
                    cx,
                    line,
                    &format!(
                        "capture argument &{} must be used within the capture operator &",
                        n
                    ),
                ),
            },
            K::Attr(n) => err(cx, line, &format!("undefined module attribute @{}", n)),
            K::Sigil { ch, parts, mods } => self.sigil(*ch, parts, mods, line, cx),
            K::ParenArgs(_) => err(cx, line, "unexpected parentheses"),
            K::MultiAlias(..) => err(cx, line, "unexpected multi-alias"),
            K::Clauses(_) => err(cx, line, "unexpected -> clauses"),
        }
    }

    fn exprs(&mut self, es: &[E], cx: &mut Cx) -> R<Vec<CE>> {
        if es.len() < 2 || cx.guard || !es.iter().any(has_match) {
            return es.iter().map(|e| self.expr(e, cx)).collect();
        }
        // Sibling arguments don't see each other's bindings (`{a = 2, a}`
        // reads the outer `a`); all bindings are visible afterwards.
        let snap = cx.scopes.clone();
        let mut out = Vec::new();
        let mut acc: Vec<(String, VarId)> = Vec::new();
        for e in es {
            cx.scopes = snap.clone();
            out.push(self.expr(e, cx)?);
            let before = snap.last().unwrap();
            for (k, v) in cx.scopes.last().unwrap() {
                if before.get(k) != Some(v) {
                    acc.push((k.clone(), *v));
                }
            }
        }
        cx.scopes = snap;
        for (k, v) in acc {
            cx.scopes.last_mut().unwrap().insert(k, v);
        }
        Ok(out)
    }

    fn interp(&mut self, parts: &[IPart], cx: &mut Cx) -> R<CE> {
        let mut out = Vec::new();
        for p in parts {
            match p {
                IPart::Lit(b) => out.push(CE::Lit(Lit::Bin(b.clone()))),
                IPart::Expr(e) => out.push(self.expr(e, cx)?),
            }
        }
        Ok(CE::Interp(out))
    }

    fn var_expr(&mut self, name: &str, line: u32, cx: &mut Cx) -> R<CE> {
        match name {
            "__MODULE__" => {
                if cx.module() == "Elixir.Tonic.Script" {
                    return Ok(CE::nil());
                }
                return Ok(CE::atom(cx.module()));
            }
            "__DIR__" => {
                let dir = std::path::Path::new(cx.file())
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".".into());
                let dir = std::fs::canonicalize(&dir)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(dir);
                return Ok(CE::Lit(Lit::Bin(dir.into_bytes())));
            }
            "__STACKTRACE__" => {
                return Ok(match cx.stacktrace {
                    Some(v) => CE::Var(v),
                    None => CE::Lit(Lit::Nil),
                })
            }
            "__ENV__" => {
                // A static Macro.Env for the current position.
                let function = match cx.fun.rsplit_once('/') {
                    Some((n, a)) if !n.is_empty() && cx.module() != "Elixir.Tonic.Script" => {
                        CE::Tuple(vec![CE::atom(n), CE::int(a.parse().unwrap_or(0))])
                    }
                    _ => CE::nil(),
                };
                let module = if cx.module() == "Elixir.Tonic.Script" { CE::nil() } else { CE::atom(cx.module()) };
                let empty = || CE::Lit(Lit::Nil);
                let mut al: Vec<(&String, &String)> = cx.env.aliases.iter().collect();
                al.sort();
                let mut aliases = CE::Lit(Lit::Nil);
                for (short, full) in al.into_iter().rev() {
                    let full = if full.starts_with("Elixir.") || !full.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                        full.clone()
                    } else {
                        format!("Elixir.{}", full)
                    };
                    let pair = CE::Tuple(vec![CE::atom(&format!("Elixir.{}", short)), CE::atom(&full)]);
                    aliases = CE::Cons(Box::new(pair), Box::new(aliases));
                }
                return Ok(CE::Struct(
                    "Elixir.Macro.Env".into(),
                    vec![
                        (CE::atom("module"), module),
                        (CE::atom("file"), CE::Lit(Lit::Bin(expand_path(&cx.env.file).into_bytes()))),
                        (CE::atom("line"), CE::int(line as i64)),
                        (CE::atom("function"), function),
                        (CE::atom("context"), CE::nil()),
                        (CE::atom("aliases"), aliases),
                        (CE::atom("requires"), empty()),
                        (CE::atom("functions"), empty()),
                        (CE::atom("macros"), empty()),
                        (CE::atom("context_modules"), empty()),
                    ],
                ));
            }
            "__CALLER__" => return err(cx, line, &format!("{} is not supported by tonic", name)),
            "_" => return err(cx, line, "invalid use of _. \"_\" represents a value to be ignored in a pattern and cannot be used in expressions"),
            _ => {}
        }
        if let Some(v) = self.lookup(cx, name) {
            return Ok(CE::Var(v));
        }
        // zero-arity function call without parens
        match self.resolve_local(cx, name, 0) {
            Some(_) => self.call(name, &[], false, line, cx),
            None => err(cx, line, &format!("undefined variable \"{}\"", name)),
        }
    }

    // ------------------------------------------------------------------
    // call resolution
    // ------------------------------------------------------------------

    fn resolve_local(&self, cx: &Cx, name: &str, arity: usize) -> Option<Resolved> {
        let m = cx.module().to_string();
        if self
            .mods
            .get(&m)
            .map(|md| md.guards.contains_key(&(name.to_string(), arity)))
            .unwrap_or(false)
        {
            return Some(Resolved::Guard(m));
        }
        if self.fun_exists(&m, name, arity).is_some() {
            return Some(Resolved::Fun(FunKey::new(&m, name, arity)));
        }
        for imp in cx.env.imports.iter().rev() {
            let allowed = match &imp.only {
                Some(only) => only.iter().any(|(n, a)| n == name && *a == arity),
                None => !imp.except.iter().any(|(n, a)| n == name && *a == arity),
            };
            if !allowed {
                continue;
            }
            if self
                .mods
                .get(&imp.module)
                .map(|md| md.guards.contains_key(&(name.to_string(), arity)))
                .unwrap_or(false)
            {
                return Some(Resolved::Guard(imp.module.clone()));
            }
            if self.fun_exists(&imp.module, name, arity) == Some(true) {
                return Some(Resolved::Fun(FunKey::new(&imp.module, name, arity)));
            }
            if let Some(b) = bifs::lookup(&imp.module, name, arity) {
                return Some(Resolved::Bif(b));
            }
        }
        if let Some(b) = bifs::lookup("Elixir.Kernel", name, arity) {
            return Some(Resolved::Bif(b));
        }
        if self.fun_exists("Elixir.Kernel", name, arity) == Some(true) {
            return Some(Resolved::Fun(FunKey::new("Elixir.Kernel", name, arity)));
        }
        None
    }

    fn inline_guard(
        &mut self,
        module: &str,
        name: &str,
        args: &[E],
        line: u32,
        cx: &mut Cx,
    ) -> R<CE> {
        let (params, body) = self.mods[module].guards[&(name.to_string(), args.len())].clone();
        let map: HashMap<String, E> = params.iter().cloned().zip(args.iter().cloned()).collect();
        let sub = map_expr(&body, &mut |x: &E| {
            if let K::Var(v) = &x.k {
                if let Some(r) = map.get(v) {
                    return Some(r.clone());
                }
            }
            None
        });
        let _ = line;
        let saved = cx.env.clone();
        cx.env.module = module.to_string();
        if let Some(menv) = self.mods.get(module).and_then(|md| md.env.clone()) {
            // The guard body resolves imports/aliases where it was defined.
            cx.env.imports.extend(menv.imports.clone());
            cx.env.aliases.extend(menv.aliases.clone());
        }
        let r = self.expr(&sub, cx);
        cx.env = saved;
        r
    }

    fn call(&mut self, name: &str, args: &[E], parens: bool, line: u32, cx: &mut Cx) -> R<CE> {
        let _ = parens;
        if matches!(
            name,
            "assert"
                | "refute"
                | "assert_receive"
                | "assert_received"
                | "refute_receive"
                | "refute_received"
                | "catch_throw"
                | "catch_exit"
                | "catch_error"
        ) && self.resolve_local(cx, name, args.len()).is_none()
        {
            if let Some(r) = self.exunit_assertion(name, args, line, cx) {
                return r;
            }
        }
        match (name, args.len()) {
            ("if", 2) => return self.if_expr(args, false, line, cx),
            ("unless", 2) => return self.if_expr(args, true, line, cx),
            ("cond", 1) => return self.cond(args, line, cx),
            ("case", 2) => return self.case_expr(args, line, cx),
            ("receive", 1) => return self.receive(args, line, cx),
            ("try", 1) => return self.try_expr(args, line, cx),
            ("with", _) if !args.is_empty() => return self.with_expr(args, line, cx),
            ("for", _) if !args.is_empty() => return self.for_expr(args, line, cx),
            ("destructure", 2)
                if matches!(args[0].k, K::List(_, None))
                    && self.resolve_local(cx, "destructure", 2).is_none() =>
            {
                let n = match &args[0].k {
                    K::List(items, _) => items.len(),
                    _ => 0,
                };
                let rhs = E::new(
                    K::Remote {
                        recv: Box::new(E::new(K::Alias("Tonic.Internal".into()), line)),
                        name: "destructure".into(),
                        args: vec![args[1].clone(), E::new(K::Int(n.to_string()), line)],
                        parens: true,
                    },
                    line,
                );
                let m = E::new(
                    K::Bin {
                        op: "=".into(),
                        l: Box::new(args[0].clone()),
                        r: Box::new(rhs),
                    },
                    line,
                );
                return self.expr(&m, cx);
            }
            ("raise", 1 | 2) | ("reraise", 2 | 3)
                if self
                    .fun_exists(&cx.module().to_string(), name, args.len())
                    .is_none()
                    && cx.env.module != "Elixir.Kernel" =>
            {
                // Inline so the raising function stays on the stack trace.
                let (exc_args, stack) = if name == "raise" {
                    (&args[..], None)
                } else {
                    (&args[..args.len() - 1], Some(&args[args.len() - 1]))
                };
                let mut ces = Vec::new();
                for a in exc_args {
                    ces.push(self.expr(a, cx)?);
                }
                let exc = CE::Call(
                    FunKey::new("Elixir.Kernel", "__exception__", ces.len()),
                    ces,
                );
                return Ok(match stack {
                    None => CE::bif("tn_error", vec![exc]),
                    Some(st) => {
                        let st = self.expr(st, cx)?;
                        CE::bif("tn_raise3", vec![CE::atom("error"), exc, st])
                    }
                });
            }
            ("is_nil", 1) => {
                let x = self.expr(&args[0], cx)?;
                return Ok(CE::bif("tn_eqx", vec![x, CE::nil()]));
            }
            ("match?", 2) => {
                let v = self.expr(&args[1], cx)?;
                let (p, g) = match &args[0].k {
                    K::Bin { op, l, r } if op == "when" => ((**l).clone(), Some((**r).clone())),
                    _ => (args[0].clone(), None),
                };
                let clause = self.scoped(cx, |s, cx| {
                    let mut b = Bound::new();
                    let pat = s.pat(&p, cx, &mut b)?;
                    s.commit(cx, b);
                    let guard = match &g {
                        Some(g) => Some(s.guard(g, cx)?),
                        None => None,
                    };
                    Ok(Clause {
                        pats: vec![pat],
                        guard,
                        body: CE::atom("true"),
                    })
                })?;
                return Ok(CE::Case(
                    vec![v],
                    vec![
                        clause,
                        Clause {
                            pats: vec![Pat::Wild],
                            guard: None,
                            body: CE::atom("false"),
                        },
                    ],
                    Fail::CaseClause,
                ));
            }
            ("alias" | "import" | "require", _) => {
                let mut c = Collector::new();
                let mut env = cx.env.clone();
                c.env_stmt(name, args, &mut env, line)?;
                cx.env = env;
                return Ok(CE::atom("ok"));
            }
            ("quote", 1 | 2) => {
                let last = args.last().unwrap();
                if let Some(body) = last.kw_get("do") {
                    let opts = if args.len() == 2 {
                        Some(&args[0])
                    } else {
                        Some(last)
                    };
                    return self.quote(body, opts, cx);
                }
            }
            ("quote" | "unquote" | "unquote_splicing", _) => {
                return err(
                    cx,
                    line,
                    &format!(
                        "{} is only supported inside __using__ macros in tonic",
                        name
                    ),
                )
            }
            ("apply", 3) => {
                if let (K::Atom(f), K::List(items, None)) = (&args[1].k, &args[2].k) {
                    if !cx.guard {
                        let m = self.expr(&args[0], cx)?;
                        let a = self.exprs(items, cx)?;
                        if let CE::Lit(Lit::Atom(ma)) = &m {
                            let ma = ma.clone();
                            return self.remote_static(&ma, f, a, line, cx, items);
                        }
                        return Ok(CE::DynCall(Box::new(m), f.clone(), a));
                    }
                }
            }
            ("put_in" | "update_in" | "get_and_update_in", 2) | ("get_in" | "pop_in", 1) => {
                if let Some((root, keys)) = self.access_path(&args[0], cx)? {
                    let mut a = vec![root, keys];
                    if args.len() == 2 {
                        a.push(self.expr(&args[1], cx)?);
                    }
                    return Ok(CE::Call(FunKey::new("Elixir.Kernel", name, a.len()), a));
                }
            }
            ("dbg", 0..=2)
                if cx.module() != "Elixir.Kernel"
                    && self.fun_exists(cx.module(), "dbg", args.len()).is_none() =>
            {
                if cx.guard {
                    return err(cx, line, "invalid expression in guard, dbg is not allowed in guards. To learn more about guards, visit: https://hexdocs.pm/elixir/patterns-and-guards.html");
                }
                let dbg = dbg_expansion(args, line);
                return self.expr(&dbg, cx);
            }
            ("super", _) => {
                let (target, params) = cx
                    .super_call
                    .clone()
                    .ok_or_else(|| format!("{}:{}: super is not allowed here", cx.file(), line))?;
                if args.len() != params.len() {
                    return err(
                        cx,
                        line,
                        &format!("super must be called with {} arguments", params.len()),
                    );
                }
                let values = args
                    .iter()
                    .map(|a| self.expr(a, cx))
                    .collect::<R<Vec<_>>>()?;
                return Ok(CE::Call(
                    FunKey::new(cx.module(), &target, values.len()),
                    values,
                ));
            }
            ("var!", 1 | 2) => return self.expr(&ctx_var(args), cx),
            ("binding", 1) if !matches!(args[0].k, K::Atom(ref a) if a == "nil") => {
                // Variables of another context (var!(x, ctx)) are named "x@ctx".
                let ctxname = match &args[0].k {
                    K::Atom(a) => a.clone(),
                    K::Alias(a) => format!("Elixir.{}", a),
                    _ => return Ok(CE::Lit(Lit::Nil)),
                };
                let suffix = format!("@{}", ctxname);
                let mut names: Vec<(String, VarId)> = Vec::new();
                for s in &cx.scopes {
                    for (k, v) in s {
                        if let Some(base) = k.strip_suffix(&suffix) {
                            names.retain(|(n, _)| n != base);
                            names.push((base.to_string(), *v));
                        }
                    }
                }
                names.sort();
                let mut acc = CE::Lit(Lit::Nil);
                for (n, v) in names.into_iter().rev() {
                    acc = CE::Cons(
                        Box::new(CE::Tuple(vec![CE::atom(&n), CE::Var(v)])),
                        Box::new(acc),
                    );
                }
                return Ok(acc);
            }
            ("binding", _) => {
                let mut pairs = Vec::new();
                let mut names: Vec<(String, VarId)> = Vec::new();
                for s in &cx.scopes {
                    for (k, v) in s {
                        // Compiler-introduced (hygienic) variables are not part of the binding.
                        if [
                            "assert__",
                            "catch__",
                            "refute_receive__",
                            "dbg_value",
                            "tonic__",
                        ]
                        .iter()
                        .any(|p| k.starts_with(p))
                            || k.contains('@')
                            || is_hygienic_name(k)
                        {
                            continue;
                        }
                        names.retain(|(n, _)| n != k);
                        names.push((k.clone(), *v));
                    }
                }
                names.sort();
                let mut acc = CE::Lit(Lit::Nil);
                for (n, v) in names.into_iter().rev() {
                    acc = CE::Cons(
                        Box::new(CE::Tuple(vec![CE::atom(&n), CE::Var(v)])),
                        Box::new(acc),
                    );
                }
                pairs.push(acc.clone());
                return Ok(acc);
            }
            _ => {}
        }
        let arity = args.len();
        match self.resolve_local(cx, name, arity) {
            Some(Resolved::Guard(m)) => self.inline_guard(&m, name, args, line, cx),
            Some(Resolved::Bif(b)) => {
                if cx.guard && !b.guard {
                    return err(
                        cx,
                        line,
                        &format!("cannot invoke {}/{} inside guards", name, arity),
                    );
                }
                let a = self.exprs(args, cx)?;
                Ok(self.bif_call(b, a, cx))
            }
            Some(Resolved::Fun(k)) => {
                if cx.guard && k.module == "Elixir.Bitwise" {
                    let en = match name {
                        "&&&" | "band" => Some("band"),
                        "|||" | "bor" => Some("bor"),
                        "^^^" | "bxor" => Some("bxor"),
                        "<<<" | "bsl" => Some("bsl"),
                        ">>>" | "bsr" => Some("bsr"),
                        "~~~" | "bnot" => Some("bnot"),
                        _ => None,
                    };
                    if let Some(b) = en.and_then(|en| bifs::lookup("erlang", en, arity)) {
                        let a = self.exprs(args, cx)?;
                        return Ok(self.bif_call(b, a, cx));
                    }
                }
                if cx.guard {
                    return err(
                        cx,
                        line,
                        &format!("cannot invoke local {}/{} inside guards", name, arity),
                    );
                }
                let a = self.exprs(args, cx)?;
                Ok(CE::Call(k, a))
            }
            None => {
                if let Some(m) = self.macro_target(cx, name, arity) {
                    return self.expand_macro(&m, name, args, line, cx);
                }
                err(
                    cx,
                    line,
                    &format!(
                        "undefined function {}/{} (expected {} to define such a function or for it to be imported, but none are available)",
                        name,
                        arity,
                        cx.module().trim_start_matches("Elixir.")
                    ),
                )
            }
        }
    }

    /// Guard-aware BIF call (boolean operators are handled by codegen).
    fn bif_call(&self, b: bifs::Bif, args: Vec<CE>, cx: &Cx) -> CE {
        if cx.guard && b.sym == "tn_not" {
            return CE::bif("__not", args);
        }
        CE::Bif(b.sym.to_string(), args)
    }

    fn remote(
        &mut self,
        recv: &E,
        name: &str,
        args: &[E],
        parens: bool,
        line: u32,
        cx: &mut Cx,
    ) -> R<CE> {
        let m = match &recv.k {
            K::Alias(a) => Some(resolve_alias(&cx.env, a)),
            K::Atom(a) => Some(a.clone()),
            K::Var(v) if v == "__MODULE__" => Some(cx.module().to_string()),
            _ => None,
        };
        match m {
            Some(m) => {
                if m == "Elixir.Kernel" {
                    // Kernel.foo — like a local call without module-local lookup.
                    if let Some(b) = bifs::lookup("Elixir.Kernel", name, args.len()) {
                        if cx.guard && !b.guard {
                            return err(
                                cx,
                                line,
                                &format!(
                                    "cannot invoke Kernel.{}/{} inside guards",
                                    name,
                                    args.len()
                                ),
                            );
                        }
                        let a = self.exprs(args, cx)?;
                        return Ok(self.bif_call(b, a, cx));
                    }
                    match (name, args.len()) {
                        (
                            "if" | "unless" | "is_nil" | "match?" | "apply" | "put_in"
                            | "update_in" | "get_in" | "var!" | "binding",
                            _,
                        ) => return self.call(name, args, parens, line, cx),
                        _ => {}
                    }
                }
                if self
                    .mods
                    .get(&m)
                    .map(|md| md.guards.contains_key(&(name.to_string(), args.len())))
                    .unwrap_or(false)
                {
                    return self.inline_guard(&m, name, args, line, cx);
                }
                if cx.guard {
                    let mb = if m == "erlang"
                        || (m == "Elixir.Bitwise"
                            && matches!(name, "band" | "bor" | "bxor" | "bnot" | "bsl" | "bsr"))
                    {
                        "erlang"
                    } else {
                        m.as_str()
                    };
                    if let Some(b) = bifs::lookup(mb, name, args.len()) {
                        if b.guard {
                            let a = self.exprs(args, cx)?;
                            return Ok(self.bif_call(b, a, cx));
                        }
                    }
                    if m == "tonic" && bifs::guard_sym(&format!("tn_{}", name)) {
                        let a = self.exprs(args, cx)?;
                        return Ok(CE::bif(&format!("tn_{}", name), a));
                    }
                    return err(
                        cx,
                        line,
                        &format!(
                            "cannot invoke remote function {}.{}/{} inside guards",
                            m.trim_start_matches("Elixir."),
                            name,
                            args.len()
                        ),
                    );
                }
                if self
                    .macros
                    .contains_key(&(m.clone(), name.to_string(), args.len()))
                {
                    return self.expand_macro(&m, name, args, line, cx);
                }
                let a = self.exprs(args, cx)?;
                self.remote_static(&m, name, a, line, cx, args)
            }
            None => {
                if cx.guard {
                    if !parens && args.is_empty() {
                        let r = self.expr(recv, cx)?;
                        return Ok(CE::bif("tn_map_get", vec![CE::atom(name), r]));
                    }
                    return err(cx, line, "cannot invoke remote functions inside guards");
                }
                let r = self.expr(recv, cx)?;
                if !parens && args.is_empty() {
                    return Ok(CE::bif("tn_dot", vec![r, CE::atom(name)]));
                }
                let a = self.exprs(args, cx)?;
                Ok(CE::DynCall(Box::new(r), name.to_string(), a))
            }
        }
    }

    fn remote_static(
        &mut self,
        m: &str,
        name: &str,
        a: Vec<CE>,
        line: u32,
        cx: &mut Cx,
        _src: &[E],
    ) -> R<CE> {
        let arity = a.len();
        // elixir_rewrite inlining: `String.duplicate/2` is `:binary.copy/2`, etc.
        if let Some((em, ef)) = crate::codegen::inline_target(m, name, arity) {
            if self.mods.get(m).map(|md| md.is_prelude).unwrap_or(false)
                && bifs::lookup(em, ef, arity).is_some()
            {
                return self.remote_static(em, ef, a, line, cx, _src);
            }
        }
        match self.fun_exists(m, name, arity) {
            Some(true) => return Ok(CE::Call(FunKey::new(m, name, arity), a)),
            Some(false) => {
                if m == cx.module() {
                    return Ok(CE::Call(FunKey::new(m, name, arity), a));
                }
                return err(
                    cx,
                    line,
                    &format!(
                        "function {}.{}/{} is private",
                        m.trim_start_matches("Elixir."),
                        name,
                        arity
                    ),
                );
            }
            None => {}
        }
        if let Some(b) = bifs::lookup(m, name, arity) {
            return Ok(CE::Bif(b.sym.to_string(), a));
        }
        if m == "tonic" {
            let sym = format!("tn_{}", name);
            match bifs::runtime_syms().get(&sym) {
                Some(&n) if n == arity => return Ok(CE::Bif(sym, a)),
                Some(&n) => {
                    return err(
                        cx,
                        line,
                        &format!(":tonic.{} expects {} arguments, got {}", name, n, arity),
                    )
                }
                None => {
                    return err(
                        cx,
                        line,
                        &format!("unknown runtime function :tonic.{}/{}", name, arity),
                    )
                }
            }
        }
        let shown = if is_erlang_mod(m) {
            format!(":{}", m)
        } else {
            m.trim_start_matches("Elixir.").to_string()
        };
        if self.mods.contains_key(m) {
            self.warnings.push(format!(
                "{}:{}: warning: {}.{}/{} is undefined or private",
                cx.file(),
                line,
                shown,
                name,
                arity
            ));
        } else {
            self.warnings.push(format!(
                "{}:{}: warning: {}.{}/{} is undefined (module {} is not available or is yet to be defined)",
                cx.file(),
                line,
                shown,
                name,
                arity,
                shown
            ));
        }
        Ok(CE::DynCall(Box::new(CE::atom(m)), name.to_string(), a))
    }

    fn macro_target(&self, cx: &Cx, name: &str, arity: usize) -> Option<String> {
        let m = cx.module().to_string();
        if self
            .macros
            .contains_key(&(m.clone(), name.to_string(), arity))
        {
            return Some(m);
        }
        for imp in &cx.env.imports {
            if self
                .macros
                .contains_key(&(imp.module.clone(), name.to_string(), arity))
            {
                return Some(imp.module.clone());
            }
        }
        None
    }

    /// Simple template macros: the macro body must end in a `quote do ... end`
    /// block; `unquote(param)` is replaced by the caller's argument AST.
    fn expand_macro(&mut self, m: &str, name: &str, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        if cx.macro_depth > 64 {
            return err(cx, line, "macro expansion too deep");
        }
        let expanded = self.macro_ast(m, name, args, line, cx)?;
        cx.macro_depth += 1;
        let saved = cx.env.clone();
        if m != cx.module() {
            if let Some(menv) = self.mods.get(m).and_then(|md| md.env.clone()) {
                // Resolve names in the macro body relative to its definition
                // module, but keep the caller's variables.
                cx.env.aliases.extend(menv.aliases.clone());
            }
        }
        let r = self.expr(&expanded, cx);
        cx.env = saved;
        cx.macro_depth -= 1;
        r
    }

    fn macro_clause_count(&self, m: &str, name: &str, arity: usize) -> usize {
        self.mods
            .get(m)
            .map(|md| {
                md.defs
                    .iter()
                    .filter(|d| {
                        let ndef = d
                            .args
                            .iter()
                            .filter(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
                            .count();
                        d.is_macro && d.name == name && arity + ndef >= d.arity && arity <= d.arity
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Expands a template macro call to its AST.
    fn macro_ast(&mut self, m: &str, name: &str, args: &[E], line: u32, cx: &mut Cx) -> R<E> {
        let mut caller_unquotes = false;
        for argument in args {
            crate::collect::map_expr(argument, &mut |node| match &node.k {
                K::Call { name, .. } if name == "quote" => Some(node.clone()),
                K::Call { name, .. } if name == "unquote" || name == "unquote_splicing" => {
                    caller_unquotes = true;
                    None
                }
                _ => None,
            });
        }
        if caller_unquotes {
            return self.host_expand(m, name, args, line, cx);
        }
        let d = self.macros[&(m.to_string(), name.to_string(), args.len())].clone();
        let body = d.body.clone().unwrap_or(E::nil(line));
        let quoted = match &body.k {
            K::Call {
                name: q, args: qa, ..
            } if q == "quote" => qa.last().and_then(|kw| kw.kw_get("do").cloned()),
            K::Block(es) => es.last().and_then(|x| match &x.k {
                K::Call {
                    name: q, args: qa, ..
                } if q == "quote" => qa.last().and_then(|kw| kw.kw_get("do").cloned()),
                _ => None,
            }),
            _ => None,
        };
        let quoted = match quoted.or_else(|| literal_template(&body)) {
            Some(q)
                if d.guard.is_none()
                    && self.macro_clause_count(m, name, args.len()) == 1
                    && simple_params(&d.args) =>
            {
                q
            }
            _ => return self.host_expand(m, name, args, line, cx),
        };
        let quote_call = match &body.k {
            K::Block(es) => es.last().unwrap_or(&body),
            _ => &body,
        };
        if !self
            .mods
            .get(m)
            .map(|module| module.is_prelude)
            .unwrap_or(false)
            && matches!(&quote_call.k, K::Call { name, args, .. } if name == "quote" && args.iter().any(|a| a.kw_get("bind_quoted").is_some()))
        {
            return self.host_expand(m, name, args, line, cx);
        }
        let mut b = HashMap::new();
        // Fill default parameters left to right with the extra arguments.
        let ndef = d
            .args
            .iter()
            .filter(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
            .count();
        let mut use_defaults = d.args.len() - args.len();
        let mut supplied = ndef - use_defaults;
        let mut ai = args.iter();
        let mut pairs: Vec<(E, E)> = Vec::new();
        for p in &d.args {
            match &p.k {
                K::Bin { op, l, r } if op == "\\\\" => {
                    if supplied > 0 {
                        supplied -= 1;
                        pairs.push(((**l).clone(), ai.next().unwrap().clone()));
                    } else {
                        use_defaults -= 1;
                        pairs.push(((**l).clone(), (**r).clone()));
                    }
                }
                _ => pairs.push((p.clone(), ai.next().unwrap().clone())),
            }
        }
        let _ = use_defaults;
        for (p, a) in pairs.iter() {
            match &p.k {
                K::Var(v) => {
                    b.insert(v.clone(), a.clone());
                }
                _ => {
                    // keyword-pattern params like `do: body`
                    if let (Some(pk), Some(ak)) = (p.as_keyword(), a.as_keyword()) {
                        for (k, pv) in pk {
                            if let K::Var(v) = &pv.k {
                                if let Some((_, av)) = ak.iter().find(|(ak2, _)| *ak2 == k) {
                                    b.insert(v.clone(), (*av).clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut unresolved = false;
        map_expr(&quoted, &mut |x: &E| {
            if let K::Call { name, args, .. } = &x.k {
                if name == "unquote_splicing" {
                    unresolved = true;
                }
                if name == "unquote" || name == "unquote_splicing" {
                    if args.len() != 1 || !matches!(&args[0].k, K::Var(v) if b.contains_key(v)) {
                        unresolved = true;
                    }
                }
            }
            None
        });
        if !self
            .mods
            .get(m)
            .map(|module| module.is_prelude)
            .unwrap_or(false)
            && (unresolved
                || matches!(&body.k, K::Block(es) if es.len() > 1)
                || matches!(&body.k, K::Call { name, args, .. } if name == "quote" && (args.len() > 1 || args.last().and_then(|a| a.as_keyword()).map(|k| k.len() > 1).unwrap_or(false))))
        {
            return self.host_expand(m, name, args, line, cx);
        }
        // Hygiene: variables written in the template belong to the macro.
        self.hyg_counter += 1;
        let quoted = hygienize(&quoted, self.hyg_counter);
        Ok(unquote_subst(&quoted, &b))
    }

    /// For `put_in(a.b[:c], v)` style macros: returns (root, keys list).
    fn access_path(&mut self, e: &E, cx: &mut Cx) -> R<Option<(CE, CE)>> {
        let mut keys: Vec<CE> = Vec::new();
        let mut cur = e.clone();
        loop {
            match &cur.k {
                K::Access { e: inner, key } => {
                    keys.push(self.expr(key, cx)?);
                    cur = (**inner).clone();
                }
                K::Remote {
                    recv,
                    name,
                    args,
                    parens: false,
                } if args.is_empty() && !matches!(recv.k, K::Alias(_) | K::Atom(_)) => {
                    keys.push(CE::Call(
                        FunKey::new("Elixir.Access", "key!", 1),
                        vec![CE::atom(name)],
                    ));
                    cur = (**recv).clone();
                }
                _ => break,
            }
        }
        if keys.is_empty() {
            return Ok(None);
        }
        let root = self.expr(&cur, cx)?;
        let mut l = CE::Lit(Lit::Nil);
        for k in keys.into_iter() {
            // keys were collected outermost-first
            l = CE::Cons(Box::new(k), Box::new(l));
        }
        Ok(Some((root, l)))
    }

    // ------------------------------------------------------------------
    // operators
    // ------------------------------------------------------------------

    fn binop(&mut self, op: &str, l: &E, r: &E, line: u32, cx: &mut Cx) -> R<CE> {
        match op {
            "=" => {
                if cx.guard {
                    return err(cx, line, "cannot use = inside guards");
                }
                let v = self.expr(r, cx)?;
                let mut b = Bound::new();
                let p = self.pat(l, cx, &mut b)?;
                self.commit(cx, b);
                Ok(CE::Match(p, Box::new(v)))
            }
            "|>" => {
                let call = pipe_into(l, r)
                    .ok_or_else(|| format!("{}:{}: invalid pipe target", cx.file(), line))?;
                self.expr(&call, cx)
            }
            "&&" | "||" => {
                if cx.guard {
                    return err(
                        cx,
                        line,
                        &format!(
                            "invalid expression in guard, {} is not allowed in guards",
                            op
                        ),
                    );
                }
                let a = self.expr(l, cx)?;
                let b = self.body(r, cx)?;
                let t = self.new_var();
                let test = CE::Var(t);
                let (then, els) = if op == "&&" {
                    (b, test.clone())
                } else {
                    (test.clone(), b)
                };
                Ok(CE::Block(vec![
                    CE::Match(Pat::Bind(t), Box::new(a)),
                    CE::If(Box::new(test), Box::new(then), Box::new(els)),
                ]))
            }
            "and" | "or" => {
                let a = self.expr(l, cx)?;
                if cx.guard {
                    let b = self.expr(r, cx)?;
                    return Ok(CE::bif(
                        if op == "and" { "__and" } else { "__or" },
                        vec![a, b],
                    ));
                }
                let b = self.body(r, cx)?;
                let v = self.new_var();
                let (tb, fb) = if op == "and" {
                    (b, CE::atom("false"))
                } else {
                    (CE::atom("true"), b)
                };
                Ok(CE::Case(
                    vec![a],
                    vec![
                        Clause {
                            pats: vec![Pat::Lit(Lit::Atom("true".into()))],
                            guard: None,
                            body: tb,
                        },
                        Clause {
                            pats: vec![Pat::Lit(Lit::Atom("false".into()))],
                            guard: None,
                            body: fb,
                        },
                        Clause {
                            pats: vec![Pat::Bind(v)],
                            guard: None,
                            body: CE::bif("tn_raise_badbool", vec![CE::Var(v), CE::atom(op)]),
                        },
                    ],
                    Fail::CaseClause,
                ))
            }
            "in" | "not in" => {
                let r = self.in_expr(l, r, line, cx)?;
                if op == "not in" {
                    if cx.guard {
                        Ok(CE::bif("__not", vec![r]))
                    } else {
                        Ok(CE::bif("tn_not", vec![r]))
                    }
                } else {
                    Ok(r)
                }
            }
            ".." => {
                let (last, step) = match &r.k {
                    K::Bin { op, l: rl, r: rr } if op == "//" => {
                        ((**rl).clone(), Some((**rr).clone()))
                    }
                    _ => (r.clone(), None),
                };
                let f = self.expr(l, cx)?;
                let la = self.expr(&last, cx)?;
                let st = match &step {
                    Some(s) => {
                        let sv = self.expr(s, cx)?;
                        let all_lit = matches!(
                            (&f, &la, &sv),
                            (
                                CE::Lit(Lit::Int(_)),
                                CE::Lit(Lit::Int(_)),
                                CE::Lit(Lit::Int(_))
                            )
                        );
                        if !all_lit && !cx.guard {
                            // As Kernel.../3: non-literal ranges are validated by Range.new/3.
                            return Ok(CE::Call(
                                FunKey::new("Elixir.Range", "new", 3),
                                vec![f, la, sv],
                            ));
                        }
                        sv
                    }
                    None => match (&f, &la) {
                        (CE::Lit(Lit::Int(a)), CE::Lit(Lit::Int(b))) => {
                            CE::int(if b < a { -1 } else { 1 })
                        }
                        _ => {
                            return Ok(CE::Call(
                                FunKey::new("Elixir.Range", "new", 2),
                                vec![f, la],
                            ));
                        }
                    },
                };
                Ok(CE::Map(vec![
                    (CE::atom("__struct__"), CE::atom("Elixir.Range")),
                    (CE::atom("first"), f),
                    (CE::atom("last"), la),
                    (CE::atom("step"), st),
                ]))
            }
            "//" => err(
                cx,
                line,
                "the // operator can only be used with ranges (first..last//step)",
            ),
            "<-" | "\\\\" | "when" | "::" | "|" | "=>" => err(
                cx,
                line,
                &format!("unexpected operator {} outside of its valid context", op),
            ),
            ".alias" => {
                let base = self.expr(l, cx)?;
                let a = match &r.k {
                    K::Alias(a) => a.clone(),
                    _ => unreachable!(),
                };
                Ok(CE::Call(
                    FunKey::new("Elixir.Module", "concat", 2),
                    vec![base, CE::Lit(Lit::Bin(a.into_bytes()))],
                ))
            }
            _ => self.call(op, &[l.clone(), r.clone()], true, line, cx),
        }
    }

    fn in_expr(&mut self, l: &E, r: &E, line: u32, cx: &mut Cx) -> R<CE> {
        if let Some(lst) = literal_list(r) {
            return self.in_expr(l, &lst, line, cx);
        }
        if let K::List(items, Some(tail)) = &r.k {
            let t = literal_list(tail).unwrap_or_else(|| (**tail).clone());
            if let K::List(titems, ttail) = &t.k {
                let mut all = items.clone();
                all.extend(titems.iter().cloned());
                let flat = E::new(K::List(all, ttail.clone()), r.line);
                return self.in_expr(l, &flat, line, cx);
            }
        }
        match &r.k {
            K::List(items, None) => {
                if items.is_empty() {
                    return Ok(CE::atom("false"));
                }
                if cx.guard {
                    let x = self.expr(l, cx)?;
                    let mut acc: Option<CE> = None;
                    for it in items {
                        let v = self.expr(it, cx)?;
                        let t = CE::bif("tn_eqx", vec![x.clone(), v]);
                        acc = Some(match acc {
                            None => t,
                            Some(a) => CE::bif("__or", vec![a, t]),
                        });
                    }
                    return Ok(acc.unwrap());
                }
                let x = self.expr(l, cx)?;
                let list = self.expr(r, cx)?;
                Ok(CE::bif("tn_lists_member", vec![x, list]))
            }
            K::Bin { op, l: a, r: b } if op == ".." => {
                let (last, step) = match &b.k {
                    K::Bin { op, l: rl, r: rr } if op == "//" => {
                        ((**rl).clone(), Some((**rr).clone()))
                    }
                    _ => ((**b).clone(), None),
                };
                if step.is_none() && cx.guard {
                    let x = self.expr(l, cx)?;
                    let lo = self.expr(a, cx)?;
                    let hi = self.expr(&last, cx)?;
                    return Ok(CE::bif(
                        "__and",
                        vec![
                            CE::bif("tn_is_integer", vec![x.clone()]),
                            CE::bif(
                                "__and",
                                vec![
                                    CE::bif("tn_ge", vec![x.clone(), lo]),
                                    CE::bif("tn_le", vec![x, hi]),
                                ],
                            ),
                        ],
                    ));
                }
                if cx.guard {
                    let x = self.expr(l, cx)?;
                    let lo = self.expr(a, cx)?;
                    let hi = self.expr(&last, cx)?;
                    let st = self.expr(step.as_ref().unwrap(), cx)?;
                    let and = |p: CE, q: CE| CE::bif("__and", vec![p, q]);
                    let zero = CE::Lit(Lit::Int(0));
                    let up = and(
                        CE::bif("tn_gt", vec![st.clone(), zero.clone()]),
                        and(
                            CE::bif("tn_ge", vec![x.clone(), lo.clone()]),
                            CE::bif("tn_le", vec![x.clone(), hi.clone()]),
                        ),
                    );
                    let down = and(
                        CE::bif("tn_lt", vec![st.clone(), zero.clone()]),
                        and(
                            CE::bif("tn_ge", vec![x.clone(), hi]),
                            CE::bif("tn_le", vec![x.clone(), lo.clone()]),
                        ),
                    );
                    let modok = CE::bif(
                        "tn_eqx",
                        vec![
                            CE::bif("tn_rem", vec![CE::bif("tn_sub", vec![x.clone(), lo]), st]),
                            zero,
                        ],
                    );
                    return Ok(and(
                        CE::bif("tn_is_integer", vec![x]),
                        and(CE::bif("__or", vec![up, down]), modok),
                    ));
                }
                let x = self.expr(l, cx)?;
                let range = self.expr(r, cx)?;
                Ok(CE::Call(
                    FunKey::new("Elixir.Enum", "member?", 2),
                    vec![range, x],
                ))
            }
            _ => {
                if cx.guard {
                    // `x in @list` where attribute expanded to a list was handled
                    // above; anything else must be a compile-time list.
                    return err(cx, line, "invalid right argument for operator \"in\" in guards, it expects a compile-time proper list or compile-time range");
                }
                let x = self.expr(l, cx)?;
                let coll = self.expr(r, cx)?;
                Ok(CE::Call(
                    FunKey::new("Elixir.Enum", "member?", 2),
                    vec![coll, x],
                ))
            }
        }
    }

    fn unop(&mut self, op: &str, x: &E, line: u32, cx: &mut Cx) -> R<CE> {
        match op {
            "!" => {
                if cx.guard {
                    return err(
                        cx,
                        line,
                        "invalid expression in guard, ! is not allowed in guards",
                    );
                }
                let v = self.expr(x, cx)?;
                Ok(CE::If(
                    Box::new(v),
                    Box::new(CE::atom("false")),
                    Box::new(CE::atom("true")),
                ))
            }
            "^" => err(cx, line, "cannot use ^ outside of match clauses"),
            "@" => err(cx, line, "module attributes cannot be set inside functions"),
            "-" => {
                if let K::Int(s) = &x.k {
                    let neg = if let Some(p) = s.strip_prefix('-') {
                        p.to_string()
                    } else {
                        format!("-{}", s)
                    };
                    return Ok(CE::Lit(int_lit(&neg)));
                }
                if let K::Float(f) = &x.k {
                    return Ok(CE::Lit(Lit::Float(-f)));
                }
                self.call("-", &[x.clone()], true, line, cx)
            }
            _ => self.call(op, &[x.clone()], true, line, cx),
        }
    }

    // ------------------------------------------------------------------
    // structs, binaries, sigils
    // ------------------------------------------------------------------

    fn struct_module(&mut self, name: &E, cx: &mut Cx) -> R<String> {
        match &name.k {
            K::Alias(a) => Ok(resolve_alias(&cx.env, a)),
            K::Var(v) if v == "__MODULE__" => Ok(cx.module().to_string()),
            K::Atom(a) => Ok(a.clone()),
            K::Bin { op, l, r } if op == ".alias" => {
                let base = self.struct_module(l, cx)?;
                if let K::Alias(a) = &r.k {
                    Ok(format!("{}.{}", base, a))
                } else {
                    err(cx, name.line, "invalid struct name")
                }
            }
            _ => err(cx, name.line, "invalid struct name"),
        }
    }

    fn struct_expr(
        &mut self,
        name: &E,
        base: Option<&E>,
        pairs: &[(E, E)],
        line: u32,
        cx: &mut Cx,
    ) -> R<CE> {
        let m = self.struct_module(name, cx)?;
        let fields: Vec<String> = match self.mods.get(&m).and_then(|md| md.struct_fields.clone()) {
            Some(f) => f.into_iter().map(|(n, _)| n).collect(),
            None => {
                return err(
                    cx,
                    line,
                    &format!("{}.__struct__/1 is undefined, cannot expand struct {}. Make sure the struct name is correct.", m.trim_start_matches("Elixir."), m.trim_start_matches("Elixir.")),
                )
            }
        };
        let mut ps = Vec::new();
        for (k, v) in pairs {
            if let K::Atom(a) = &k.k {
                if !fields.contains(a) {
                    return err(
                        cx,
                        line,
                        &format!(
                            "unknown key :{} for struct {}",
                            a,
                            m.trim_start_matches("Elixir.")
                        ),
                    );
                }
            }
            ps.push((self.expr(k, cx)?, self.expr(v, cx)?));
        }
        match base {
            Some(b) => {
                let be = self.expr(b, cx)?;
                Ok(CE::MapUpdate(Box::new(be), ps, true))
            }
            None => {
                let enforce = self.mods[&m].enforce.clone();
                for k in &enforce {
                    if !pairs
                        .iter()
                        .any(|(pk, _)| matches!(&pk.k, K::Atom(a) if a == k))
                    {
                        return err(
                            cx,
                            line,
                            &format!(
                                "the following keys must also be given when building struct {}: [:{}]",
                                m.trim_start_matches("Elixir."),
                                k
                            ),
                        );
                    }
                }
                Ok(CE::Struct(m, ps))
            }
        }
    }

    fn seg_spec(
        &mut self,
        spec: Option<&E>,
        cx: &mut Cx,
        is_pat: bool,
        bound: Option<&Bound>,
    ) -> R<(Option<BinType>, Option<CE>, u32, bool, bool, bool)> {
        // returns (type, size, unit, little, signed, explicit_size)
        let mut ty = None;
        let mut size = None;
        let mut unit = None;
        let mut little = false;
        let mut signed = false;
        let mut items = Vec::new();
        if let Some(s) = spec {
            flatten_dash(s, &mut items);
        }
        for it in items {
            match &it.k {
                K::Var(n)
                | K::Call {
                    name: n, args: _, ..
                } if matches!(&it.k, K::Var(_))
                    || matches!(&it.k, K::Call { args, .. } if args.is_empty()) =>
                {
                    match n.as_str() {
                        "integer" => ty = Some(BinType::Int),
                        "float" => ty = Some(BinType::Float),
                        "binary" | "bytes" => ty = Some(BinType::Bin),
                        "bitstring" | "bits" => ty = Some(BinType::Bits),
                        "utf8" => ty = Some(BinType::Utf8),
                        "utf16" => ty = Some(BinType::Utf16),
                        "utf32" => ty = Some(BinType::Utf32),
                        "signed" => signed = true,
                        "unsigned" => signed = false,
                        "big" => little = false,
                        "little" => little = true,
                        "native" => little = cfg!(target_endian = "little"),
                        other => {
                            return err(
                                cx,
                                it.line,
                                &format!("unknown bitstring specifier: {}", other),
                            )
                        }
                    }
                }
                K::Call { name, args, .. } if name == "size" && args.len() == 1 => {
                    size = Some(self.size_expr(&args[0], cx, is_pat, bound)?);
                }
                K::Call { name, args, .. } if name == "unit" && args.len() == 1 => {
                    if let K::Int(n) = &args[0].k {
                        unit = Some(n.parse::<u32>().unwrap_or(1));
                    }
                }
                K::Int(n) => size = Some(CE::int(n.parse::<i64>().unwrap_or(8))),
                _ => return err(cx, it.line, "invalid bitstring specifier"),
            }
        }
        let explicit = size.is_some();
        Ok((ty, size, unit.unwrap_or(0), little, signed, explicit))
    }

    fn size_expr(&mut self, e: &E, cx: &mut Cx, is_pat: bool, bound: Option<&Bound>) -> R<CE> {
        if is_pat {
            if let K::Var(v) = &e.k {
                if let Some(b) = bound {
                    if let Some(id) = b.get(v) {
                        return Ok(CE::Var(*id));
                    }
                }
                if let Some(id) = self.lookup(cx, v) {
                    return Ok(CE::Var(id));
                }
                return err(
                    cx,
                    e.line,
                    &format!("undefined variable \"{}\" in bitstring size", v),
                );
            }
            if let K::Un { op, e: inner } = &e.k {
                if op == "^" {
                    return self.size_expr(inner, cx, is_pat, bound);
                }
            }
        }
        self.expr(e, cx)
    }

    fn finish_seg<T>(
        &self,
        val: T,
        is_str: bool,
        spec: (Option<BinType>, Option<CE>, u32, bool, bool, bool),
    ) -> BinSeg<T> {
        let (ty, size, unit, little, signed, _) = spec;
        let ty = ty.unwrap_or(if is_str { BinType::Bin } else { BinType::Int });
        // Normalise sizes to bits.
        let size = match (&ty, size) {
            (BinType::Int, Some(s)) | (BinType::Float, Some(s)) | (BinType::Bits, Some(s)) => {
                let u = if unit == 0 { 1 } else { unit } as i64;
                Some(mul_const(s, u))
            }
            (BinType::Bin, Some(s)) => {
                let u = if unit == 0 { 8 } else { unit } as i64;
                Some(mul_const(s, u))
            }
            (BinType::Int, None) => Some(CE::int(8)),
            (BinType::Float, None) => Some(CE::int(64)),
            (_, s) => s,
        };
        BinSeg {
            val,
            ty,
            size,
            little,
            signed,
            unit,
        }
    }

    fn bin_seg_expr(&mut self, s: &E, cx: &mut Cx) -> R<BinSeg<CE>> {
        let (v, spec) = match &s.k {
            K::Bin { op, l, r } if op == "::" => ((**l).clone(), Some((**r).clone())),
            _ => (s.clone(), None),
        };
        let is_str = matches!(v.k, K::Str(_) | K::Interp(_) | K::Bits(_));
        let sp = self.seg_spec(spec.as_ref(), cx, false, None)?;
        if let (K::Str(bytes), Some(t @ (BinType::Utf8 | BinType::Utf16 | BinType::Utf32))) =
            (&v.k, &sp.0)
        {
            let enc = encode_str_seg(bytes, t, sp.3);
            return Ok(BinSeg {
                val: CE::Lit(Lit::Bin(enc)),
                ty: BinType::Bin,
                size: None,
                little: false,
                signed: false,
                unit: 8,
            });
        }
        let val = self.expr(&v, cx)?;
        Ok(self.finish_seg(val, is_str, sp))
    }

    fn sigil(&mut self, ch: char, parts: &[IPart], mods: &str, line: u32, cx: &mut Cx) -> R<CE> {
        let literal: Option<Vec<u8>> = if parts.iter().all(|p| matches!(p, IPart::Lit(_))) {
            let mut b = Vec::new();
            for p in parts {
                if let IPart::Lit(x) = p {
                    b.extend(x);
                }
            }
            Some(b)
        } else {
            None
        };
        match ch {
            's' | 'S' => match literal {
                Some(b) => Ok(CE::Lit(Lit::Bin(b))),
                None => self.interp(parts, cx),
            },
            'c' | 'C' => match literal {
                Some(b) => Ok(charlist_lit(&b)),
                None => {
                    let i = self.interp(parts, cx)?;
                    Ok(CE::bif("tn_str_to_charlist", vec![i]))
                }
            },
            'w' | 'W' => {
                let kind = mods.chars().next().unwrap_or('s');
                match literal {
                    Some(b) => {
                        let s = String::from_utf8_lossy(&b).into_owned();
                        let mut acc = CE::Lit(Lit::Nil);
                        for w in s.split_whitespace().collect::<Vec<_>>().into_iter().rev() {
                            let item = match kind {
                                'a' => CE::atom(w),
                                'c' => charlist_lit(w.as_bytes()),
                                _ => CE::Lit(Lit::Bin(w.as_bytes().to_vec())),
                            };
                            acc = CE::Cons(Box::new(item), Box::new(acc));
                        }
                        Ok(acc)
                    }
                    None => {
                        let i = self.interp(parts, cx)?;
                        let split = CE::bif("tn_str_split_ws", vec![i]);
                        Ok(match kind {
                            'a' => CE::Call(
                                FunKey::new("Elixir.Enum", "map", 2),
                                vec![
                                    split,
                                    CE::FunRef(
                                        FunKey::new("Elixir.String", "to_atom", 1),
                                        Target::Fun(FunKey::new("Elixir.String", "to_atom", 1)),
                                    ),
                                ],
                            ),
                            _ => split,
                        })
                    }
                }
            }
            'r' | 'R' => {
                let src = match literal {
                    Some(b) => CE::Lit(Lit::Bin(b)),
                    None => self.interp(parts, cx)?,
                };
                // Same option list Regex.compile!/2 builds (each modifier
                // prepends its options).
                let mut atoms: Vec<CE> = Vec::new();
                for m in mods.chars() {
                    let add: Vec<CE> = match m {
                        'i' => vec![CE::atom("caseless")],
                        'm' => vec![CE::atom("multiline")],
                        's' => vec![
                            CE::atom("dotall"),
                            CE::Tuple(vec![CE::atom("newline"), CE::atom("anycrlf")]),
                        ],
                        'x' => vec![CE::atom("extended")],
                        'u' => vec![CE::atom("unicode"), CE::atom("ucp")],
                        'U' => vec![CE::atom("ungreedy")],
                        'f' => vec![CE::atom("firstline")],
                        _ => return err(cx, line, &format!("unknown regex modifier {}", m)),
                    };
                    let mut n = add;
                    n.extend(atoms);
                    atoms = n;
                }
                let mut opts = CE::Lit(Lit::Nil);
                for a in atoms.into_iter().rev() {
                    opts = CE::Cons(Box::new(a), Box::new(opts));
                }
                Ok(CE::Map(vec![
                    (CE::atom("__struct__"), CE::atom("Elixir.Regex")),
                    (CE::atom("source"), src),
                    (CE::atom("opts"), opts),
                    (CE::atom("re_pattern"), CE::nil()),
                    (CE::atom("re_version"), CE::nil()),
                ]))
            }
            'D' | 'T' | 'N' | 'U'
                if literal.is_some()
                    && self
                        .resolve_local(cx, &format!("sigil_{}", ch), 2)
                        .is_none() =>
            {
                let text = String::from_utf8_lossy(literal.as_ref().unwrap())
                    .trim()
                    .to_string();
                match calendar_sigil(ch, &text, line) {
                    Some(e) => self.expr(&e, cx),
                    None => err(
                        cx,
                        line,
                        &format!(
                            "cannot parse {} sigil: {:?}",
                            match ch {
                                'D' => "date",
                                'T' => "time",
                                'N' => "naive datetime",
                                _ => "UTC datetime",
                            },
                            text
                        ),
                    ),
                }
            }
            _ => {
                let name = format!("sigil_{}", ch);
                let s = match literal {
                    Some(b) => E::new(K::Str(b), line),
                    None => E::new(K::Interp(parts.to_vec()), line),
                };
                let m = E::new(
                    K::Charlist(vec![IPart::Lit(mods.as_bytes().to_vec())]),
                    line,
                );
                if self.resolve_local(cx, &name, 2).is_none() {
                    return err(
                        cx,
                        line,
                        &format!("sigil ~{} is not supported by tonic", ch),
                    );
                }
                self.call(&name, &[s, m], true, line, cx)
            }
        }
    }

    // ------------------------------------------------------------------
    // functions and captures
    // ------------------------------------------------------------------

    fn lambda(&mut self, clauses: &[crate::ast::Clause], line: u32, cx: &mut Cx) -> R<CE> {
        if cx.guard {
            return err(cx, line, "cannot define anonymous functions inside guards");
        }
        let arity = clauses[0].args.len();
        if clauses.iter().any(|c| c.args.len() != arity) {
            return err(
                cx,
                line,
                "cannot mix clauses with different arities in anonymous functions",
            );
        }
        let params: Vec<VarId> = (0..arity).map(|_| self.new_var()).collect();
        let mut cs = Vec::new();
        for c in clauses {
            let cl = self.scoped(cx, |s, cx| {
                let mut b = Bound::new();
                let mut pats = Vec::new();
                for a in &c.args {
                    pats.push(s.pat(a, cx, &mut b)?);
                }
                s.commit(cx, b);
                let guard = match &c.guard {
                    Some(g) => Some(s.guard(g, cx)?),
                    None => None,
                };
                let body = s.expr(&c.body, cx)?;
                Ok(Clause { pats, guard, body })
            })?;
            cs.push(cl);
        }
        self.next_lambda += 1;
        cx.lambda_index += 1;
        Ok(CE::Fn(Rc::new(Lambda {
            id: self.next_lambda,
            arity,
            clauses: cs,
            params,
            free: vec![],
            module: cx.module().to_string(),
            parent: cx.fun.clone(),
            line,
        })))
    }

    fn capture(&mut self, inner: &E, line: u32, cx: &mut Cx) -> R<CE> {
        if let K::Bin { op, l, r } = &inner.k {
            if op == "/" {
                if let K::Int(n) = &r.k {
                    let arity: usize = n.parse().unwrap_or(0);
                    match &l.k {
                        K::Var(name) | K::Call { name, .. }
                            if matches!(&l.k, K::Var(_))
                                || matches!(&l.k, K::Call { args, .. } if args.is_empty()) =>
                        {
                            return match self.resolve_local(cx, name, arity) {
                                Some(Resolved::Fun(k)) => Ok(CE::FunRef(k.clone(), Target::Fun(k))),
                                Some(Resolved::Bif(b)) => Ok(CE::FunRef(
                                    FunKey::new("Elixir.Kernel", name, arity),
                                    Target::Bif(b.sym.to_string()),
                                )),
                                _ if arity > 0
                                    && matches!(
                                        name.as_str(),
                                        "is_nil"
                                            | "to_string"
                                            | "to_charlist"
                                            | "if"
                                            | "unless"
                                            | "match?"
                                            | "inspect"
                                    ) =>
                                {
                                    // Kernel macros: &is_nil/1 is &is_nil(&1)
                                    let args = (1..=arity)
                                        .map(|i| E::new(K::CapArg(i as _), line))
                                        .collect();
                                    self.capture(&E::call(name, args, line), line, cx)
                                }
                                _ => {
                                    err(cx, line, &format!("undefined function {}/{}", name, arity))
                                }
                            };
                        }
                        K::Remote {
                            recv, name, args, ..
                        } if args.is_empty() => {
                            let m = match &recv.k {
                                K::Alias(a) => Some(resolve_alias(&cx.env, a)),
                                K::Atom(a) => Some(a.clone()),
                                K::Var(v) if v == "__MODULE__" => Some(cx.module().to_string()),
                                _ => None,
                            };
                            if let Some(m) = m {
                                let key = FunKey::new(&m, name, arity);
                                if self.fun_exists(&m, name, arity).is_some() {
                                    return Ok(CE::FunRef(key.clone(), Target::Fun(key)));
                                }
                                if let Some(b) = bifs::lookup(&m, name, arity) {
                                    return Ok(CE::FunRef(key, Target::Bif(b.sym.to_string())));
                                }
                                if m == "tonic" {
                                    return Ok(CE::FunRef(
                                        key,
                                        Target::Bif(format!("tn_{}", name)),
                                    ));
                                }
                                return Ok(CE::FunRef(key, Target::Dynamic));
                            }
                            // &mod.fun/arity with a variable module
                            let params: Vec<VarId> = (0..arity).map(|_| self.new_var()).collect();
                            let me = self.expr(recv, cx)?;
                            let mv = self.new_var();
                            let body = CE::DynCall(
                                Box::new(CE::Var(mv)),
                                name.clone(),
                                params.iter().map(|p| CE::Var(*p)).collect(),
                            );
                            self.next_lambda += 1;
                            let lam = CE::Fn(Rc::new(Lambda {
                                id: self.next_lambda,
                                arity,
                                clauses: vec![Clause {
                                    pats: params.iter().map(|p| Pat::Bind(*p)).collect(),
                                    guard: None,
                                    body,
                                }],
                                params: (0..arity).map(|_| self.new_var()).collect(),
                                free: vec![],
                                module: cx.module().to_string(),
                                parent: cx.fun.clone(),
                                line,
                            }));
                            return Ok(CE::Block(vec![
                                CE::Match(Pat::Bind(mv), Box::new(me)),
                                lam,
                            ]));
                        }
                        _ => {}
                    }
                }
            }
        }
        // &(expr with &1 ...)
        let n = max_cap_arg(inner);
        if n == 0 {
            return err(cx, line, "invalid args for &, expected one of:\n\n  * &Mod.fun/arity to capture a remote function, such as &Enum.map/2\n  * &fun/arity to capture a local or imported function, such as &is_atom/1\n  * &some_code(&1, ...) containing at least one argument as &1, such as &List.flatten(&1)");
        }
        let cap_vars: Vec<VarId> = (0..n).map(|_| self.new_var()).collect();
        let saved = cx.cap.replace(cap_vars.clone());
        let body = self.expr(inner, cx);
        cx.cap = saved;
        let body = body?;
        self.next_lambda += 1;
        cx.lambda_index += 1;
        Ok(CE::Fn(Rc::new(Lambda {
            id: self.next_lambda,
            arity: n as usize,
            clauses: vec![Clause {
                pats: cap_vars.iter().map(|v| Pat::Bind(*v)).collect(),
                guard: None,
                body,
            }],
            params: (0..n).map(|_| self.new_var()).collect(),
            free: vec![],
            module: cx.module().to_string(),
            parent: cx.fun.clone(),
            line,
        })))
    }

    // ------------------------------------------------------------------
    // control flow
    // ------------------------------------------------------------------

    fn kw_section<'a>(args: &'a [E], key: &str) -> Option<&'a E> {
        args.last().and_then(|kw| kw.kw_get(key))
    }

    fn if_expr(&mut self, args: &[E], negate: bool, line: u32, cx: &mut Cx) -> R<CE> {
        if cx.guard {
            return err(cx, line, "if/unless are not allowed in guards");
        }
        let c = self.expr(&args[0], cx)?;
        let then_e = Self::kw_section(args, "do")
            .cloned()
            .ok_or_else(|| format!("{}:{}: missing do in if", cx.file(), line))?;
        let else_e = Self::kw_section(args, "else").cloned();
        let t = self.body(&then_e, cx)?;
        let f = match else_e {
            Some(e) => self.body(&e, cx)?,
            None => CE::nil(),
        };
        let (t, f) = if negate { (f, t) } else { (t, f) };
        Ok(CE::If(Box::new(c), Box::new(t), Box::new(f)))
    }

    fn clauses_of<'a>(&self, e: &'a E, cx: &Cx, what: &str) -> R<Vec<crate::ast::Clause>> {
        match &e.k {
            K::Clauses(cs) => Ok(cs.clone()),
            K::Block(b) if b.is_empty() => Ok(vec![]),
            _ => err(cx, e.line, &format!("expected -> clauses for {}", what)),
        }
    }

    fn cond(&mut self, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        let body = Self::kw_section(args, "do")
            .ok_or_else(|| format!("{}:{}: missing do in cond", cx.file(), line))?;
        let cs = self.clauses_of(body, cx, "cond")?;
        let mut acc = CE::bif("tn_raise_cond_clause", vec![]);
        for c in cs.iter().rev() {
            if c.args.len() != 1 {
                return err(cx, c.line, "cond clauses expect exactly one condition");
            }
            let t = self.expr(&c.args[0], cx)?;
            let b = self.body(&c.body, cx)?;
            if matches!(&t, CE::Lit(Lit::Atom(a)) if a == "true") {
                acc = b;
                continue;
            }
            acc = CE::If(Box::new(t), Box::new(b), Box::new(acc));
        }
        Ok(acc)
    }

    fn match_clause(&mut self, c: &crate::ast::Clause, cx: &mut Cx) -> R<Clause> {
        self.scoped(cx, |s, cx| {
            let mut b = Bound::new();
            let mut pats = Vec::new();
            for a in &c.args {
                pats.push(s.pat(a, cx, &mut b)?);
            }
            s.commit(cx, b);
            let guard = match &c.guard {
                Some(g) => Some(s.guard(g, cx)?),
                None => None,
            };
            let body = s.expr(&c.body, cx)?;
            Ok(Clause { pats, guard, body })
        })
    }

    fn case_expr(&mut self, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        let subj = self.expr(&args[0], cx)?;
        let body = Self::kw_section(&args[1..], "do")
            .ok_or_else(|| format!("{}:{}: missing do in case", cx.file(), line))?;
        let cs = self.clauses_of(body, cx, "case")?;
        let mut out = Vec::new();
        for c in &cs {
            if c.args.len() != 1 {
                return err(cx, c.line, "case clauses expect exactly one pattern");
            }
            out.push(self.match_clause(c, cx)?);
        }
        Ok(CE::Case(vec![subj], out, Fail::CaseClause))
    }

    fn receive(&mut self, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        let mut out = Vec::new();
        if let Some(body) = Self::kw_section(args, "do") {
            for c in self.clauses_of(body, cx, "receive")? {
                out.push(self.match_clause(&c, cx)?);
            }
        }
        let after = match Self::kw_section(args, "after") {
            Some(a) => {
                let cs = self.clauses_of(a, cx, "after")?;
                if cs.len() != 1 || cs[0].args.len() != 1 {
                    return err(cx, line, "expected a single timeout clause in after");
                }
                let t = self.expr(&cs[0].args[0], cx)?;
                let b = self.body(&cs[0].body, cx)?;
                Some((Box::new(t), Box::new(b)))
            }
            None => None,
        };
        Ok(CE::Receive(out, after))
    }

    fn try_expr(&mut self, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        let kw = &args[0];
        let body_e = kw.kw_get("do").cloned().unwrap_or(E::nil(line));
        let body = self.body(&body_e, cx)?;
        let mut catches = Vec::new();
        if let Some(r) = kw.kw_get("rescue") {
            for c in self.clauses_of(r, cx, "rescue")? {
                catches.push(self.rescue_clause(&c, cx)?);
            }
        }
        if let Some(r) = kw.kw_get("catch") {
            for c in self.clauses_of(r, cx, "catch")? {
                catches.push(self.catch_clause(&c, cx)?);
            }
        }
        let mut else_clauses = Vec::new();
        if let Some(r) = kw.kw_get("else") {
            for c in self.clauses_of(r, cx, "else")? {
                else_clauses.push(self.match_clause(&c, cx)?);
            }
        }
        let after = match kw.kw_get("after") {
            Some(a) => Some(self.body(a, cx)?),
            None => None,
        };
        Ok(CE::Try(Box::new(TryE {
            body,
            catches,
            else_clauses,
            after,
        })))
    }

    fn rescue_clause(&mut self, c: &crate::ast::Clause, cx: &mut Cx) -> R<Clause> {
        if c.args.len() != 1 {
            return err(cx, c.line, "rescue clauses expect one argument");
        }
        self.scoped(cx, |s, cx| {
            let raw = s.new_var();
            let st = s.new_var();
            let (var, mods): (Option<String>, Vec<E>) = match &c.args[0].k {
                K::Var(v) => (Some(v.clone()), vec![]),
                K::Bin { op, l, r } if op == "in" => {
                    let v = match &l.k {
                        K::Var(v) => v.clone(),
                        _ => return err(cx, c.line, "invalid rescue clause"),
                    };
                    let mods = match &r.k {
                        K::List(items, None) => items.clone(),
                        _ => vec![(**r).clone()],
                    };
                    (Some(v), mods)
                }
                K::Alias(_) | K::Atom(_) => (None, vec![c.args[0].clone()]),
                K::List(items, None) => (None, items.clone()),
                _ => return err(cx, c.line, "invalid rescue clause"),
            };
            let mut guard: Option<CE> = None;
            for m in &mods {
                let ma = match &m.k {
                    K::Alias(a) => resolve_alias(&cx.env, a),
                    K::Atom(a) => a.clone(),
                    _ => return err(cx, c.line, "invalid rescue clause"),
                };
                // Raw Erlang reasons (:badarg, {:badmatch, v}, ...) rescue as
                // the exception they normalize to.
                let test = CE::bif(
                    "tn_eqx",
                    vec![CE::bif("tn_exc_module", vec![CE::Var(raw)]), CE::atom(&ma)],
                );
                guard = Some(match guard {
                    None => test,
                    Some(g) => CE::bif("__or", vec![g, test]),
                });
            }
            let mut body_parts = Vec::new();
            if let Some(v) = var {
                if v != "_" {
                    let id = s.new_var();
                    cx.scopes.last_mut().unwrap().insert(v, id);
                    body_parts.push(CE::Match(
                        Pat::Bind(id),
                        Box::new(CE::Call(
                            FunKey::new("Elixir.Exception", "normalize", 3),
                            vec![CE::atom("error"), CE::Var(raw), CE::Var(st)],
                        )),
                    ));
                }
            }
            let saved = cx.stacktrace.replace(st);
            let b = s.expr(&c.body, cx);
            cx.stacktrace = saved;
            body_parts.push(b?);
            Ok(Clause {
                pats: vec![Pat::Tuple(vec![
                    Pat::Lit(Lit::Atom("error".into())),
                    Pat::Bind(raw),
                    Pat::Bind(st),
                ])],
                guard,
                body: CE::Block(body_parts),
            })
        })
    }

    fn catch_clause(&mut self, c: &crate::ast::Clause, cx: &mut Cx) -> R<Clause> {
        self.scoped(cx, |s, cx| {
            let st = s.new_var();
            let mut b = Bound::new();
            let (kind, val) = match c.args.len() {
                1 => (
                    Pat::Lit(Lit::Atom("throw".into())),
                    s.pat(&c.args[0], cx, &mut b)?,
                ),
                2 => {
                    let k = s.pat(&c.args[0], cx, &mut b)?;
                    let v = s.pat(&c.args[1], cx, &mut b)?;
                    (k, v)
                }
                _ => return err(cx, c.line, "catch clauses expect one or two arguments"),
            };
            s.commit(cx, b);
            let guard = match &c.guard {
                Some(g) => Some(s.guard(g, cx)?),
                None => None,
            };
            let saved = cx.stacktrace.replace(st);
            let body = s.expr(&c.body, cx);
            cx.stacktrace = saved;
            Ok(Clause {
                pats: vec![Pat::Tuple(vec![kind, val, Pat::Bind(st)])],
                guard,
                body: body?,
            })
        })
    }

    fn with_expr(&mut self, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        let (kw, items) = match args.last() {
            Some(k) if k.kw_get("do").is_some() => (k.clone(), &args[..args.len() - 1]),
            _ => return err(cx, line, "missing do in with"),
        };
        // Allow keyword do: after other keyword entries (e.g. `else:` inline)
        let body = kw.kw_get("do").cloned().unwrap();
        // `else` clauses only see the variables bound before the `with`.
        let else_cs: Option<Vec<Clause>> = match kw.kw_get("else") {
            Some(e) => {
                let cs = self.clauses_of(e, cx, "else")?;
                let mut out = Vec::new();
                for c in &cs {
                    out.push(self.match_clause(c, cx)?);
                }
                Some(out)
            }
            None => None,
        };
        self.scoped(cx, |s, cx| {
            s.with_chain(items, &body, else_cs.as_deref(), cx)
        })
    }

    fn with_chain(
        &mut self,
        items: &[E],
        body: &E,
        else_cs: Option<&[Clause]>,
        cx: &mut Cx,
    ) -> R<CE> {
        if items.is_empty() {
            return self.expr(body, cx);
        }
        let item = &items[0];
        if let K::Bin { op, l, r } = &item.k {
            if op == "<-" {
                let v = self.expr(r, cx)?;
                let (p, g) = match &l.k {
                    K::Bin { op, l: pl, r: pr } if op == "when" => {
                        ((**pl).clone(), Some((**pr).clone()))
                    }
                    _ => ((**l).clone(), None),
                };
                let ok_clause = self.scoped(cx, |s, cx| {
                    let mut b = Bound::new();
                    let pat = s.pat(&p, cx, &mut b)?;
                    s.commit(cx, b);
                    let guard = match &g {
                        Some(g) => Some(s.guard(g, cx)?),
                        None => None,
                    };
                    let rest = s.with_chain(&items[1..], body, else_cs, cx)?;
                    Ok(Clause {
                        pats: vec![pat],
                        guard,
                        body: rest,
                    })
                })?;
                let o = self.new_var();
                let else_body = match else_cs {
                    Some(cs) => CE::Case(vec![CE::Var(o)], cs.to_vec(), Fail::WithClause),
                    None => CE::Var(o),
                };
                return Ok(CE::Case(
                    vec![v],
                    vec![
                        ok_clause,
                        Clause {
                            pats: vec![Pat::Bind(o)],
                            guard: None,
                            body: else_body,
                        },
                    ],
                    Fail::CaseClause,
                ));
            }
        }
        let first = self.expr(item, cx)?;
        let rest = self.with_chain(&items[1..], body, else_cs, cx)?;
        Ok(CE::Block(vec![first, rest]))
    }

    fn for_expr(&mut self, args: &[E], line: u32, cx: &mut Cx) -> R<CE> {
        // Split generators/filters from trailing keyword options.
        let mut items: Vec<E> = Vec::new();
        let mut opts: Vec<(String, E)> = Vec::new();
        for a in args {
            if let Some(kw) = a.as_keyword() {
                if !kw.is_empty()
                    && kw
                        .iter()
                        .all(|(k, _)| matches!(k.as_str(), "do" | "into" | "uniq" | "reduce"))
                {
                    for (k, v) in kw {
                        opts.push((k, v.clone()));
                    }
                    continue;
                }
            }
            items.push(a.clone());
        }
        let get = |k: &str| opts.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let body = get("do").ok_or_else(|| format!("{}:{}: missing do in for", cx.file(), line))?;
        let into = get("into");
        let uniq = matches!(get("uniq").map(|e| e.k), Some(K::Atom(a)) if a == "true");
        let reduce = get("reduce");
        self.scoped(cx, |s, cx| {
            if let Some(init) = reduce {
                let init_ce = s.expr(&init, cx)?;
                let cs = s.clauses_of(&body, cx, "for reduce")?;
                let acc0 = s.new_var();
                let r = s.for_loop(&items, cx, CE::Var(acc0), &mut |s2, cx2, acc| {
                    let mut out = Vec::new();
                    for c in &cs {
                        out.push(s2.match_clause(c, cx2)?);
                    }
                    Ok(CE::Case(vec![acc], out, Fail::CaseClause))
                })?;
                return Ok(CE::Block(vec![
                    CE::Match(Pat::Bind(acc0), Box::new(init_ce)),
                    r,
                ]));
            }
            let acc0 = s.new_var();
            let r = s.for_loop(&items, cx, CE::Var(acc0), &mut |s2, cx2, acc| {
                let b = s2.expr(&body, cx2)?;
                Ok(CE::Cons(Box::new(b), Box::new(acc)))
            })?;
            let mut res = CE::Block(vec![
                CE::Match(Pat::Bind(acc0), Box::new(CE::Lit(Lit::Nil))),
                CE::bif("tn_reverse", vec![r]),
            ]);
            if uniq {
                res = CE::Call(FunKey::new("Elixir.Enum", "uniq", 1), vec![res]);
            }
            if let Some(i) = into {
                let ie = s.expr(&i, cx)?;
                res = CE::Call(FunKey::new("Elixir.Enum", "into", 2), vec![res, ie]);
            }
            Ok(res)
        })
    }

    /// Build nested reductions. `inner` produces the new accumulator from the
    /// current one.
    fn for_loop(
        &mut self,
        items: &[E],
        cx: &mut Cx,
        acc: CE,
        inner: &mut dyn FnMut(&mut Self, &mut Cx, CE) -> R<CE>,
    ) -> R<CE> {
        if items.is_empty() {
            return inner(self, cx, acc);
        }
        let it = &items[0];
        // generator?
        let gen: Option<(E, CE)> = match &it.k {
            K::Bin { op, l, r } if op == "<-" => {
                let coll = self.expr(r, cx)?;
                Some(((**l).clone(), coll))
            }
            K::Bits(segs) if matches!(segs.last().map(|s| &s.k), Some(K::Bin { op, .. }) if op == "<-") =>
            {
                let (l, r) = match &segs.last().unwrap().k {
                    K::Bin { l, r, .. } => ((**l).clone(), (**r).clone()),
                    _ => unreachable!(),
                };
                let (v, spec) = match &l.k {
                    K::Bin { op, l: vl, r: vr } if op == "::" => {
                        ((**vl).clone(), Some((**vr).clone()))
                    }
                    _ => (l.clone(), None),
                };
                let simple_var = matches!(&v.k, K::Var(_));
                let utf8 = matches!(&spec, Some(E { k: K::Var(n), .. }) if n == "utf8");
                let byte =
                    spec.is_none() || matches!(&spec, Some(E { k: K::Int(n), .. }) if n == "8");
                if segs.len() == 1 && simple_var && (utf8 || byte) {
                    let bin = self.expr(&r, cx)?;
                    let coll = if utf8 {
                        CE::bif("tn_str_to_charlist", vec![bin])
                    } else {
                        CE::bif("tn_bin_to_list", vec![bin])
                    };
                    Some((v, coll))
                } else {
                    // General case: split the bitstring into chunks matching the
                    // segments, then match each chunk against them.
                    let line = it.line;
                    let mut pat_segs: Vec<E> = segs[..segs.len() - 1].to_vec();
                    pat_segs.push(l.clone());
                    let rest = E::new(K::Var("tonic__bin_gen_rest".into()), line);
                    let mut split_segs = pat_segs.clone();
                    split_segs.push(E::new(
                        K::Bin {
                            op: "::".into(),
                            l: Box::new(rest.clone()),
                            r: Box::new(E::new(K::Var("bitstring".into()), line)),
                        },
                        line,
                    ));
                    let splitter = E::new(
                        K::Fn(vec![
                            crate::ast::Clause {
                                args: vec![E::new(K::Bits(split_segs), line)],
                                guard: None,
                                body: rest.clone(),
                                line,
                            },
                            crate::ast::Clause {
                                args: vec![E::new(K::Var("_".into()), line)],
                                guard: None,
                                body: E::new(K::Atom("nil".into()), line),
                                line,
                            },
                        ]),
                        line,
                    );
                    let bin = self.expr(&r, cx)?;
                    let sp = self.expr(&splitter, cx)?;
                    let coll = CE::Call(
                        FunKey::new("Elixir.Tonic.Internal", "bin_chunks", 2),
                        vec![bin, sp],
                    );
                    Some((E::new(K::Bits(pat_segs), line), coll))
                }
            }
            _ => None,
        };
        match gen {
            Some((pat_e, coll)) => {
                let (pat_e, guard_e) = match &pat_e.k {
                    K::Bin { op, l, r } if op == "when" => ((**l).clone(), Some((**r).clone())),
                    _ => (pat_e, None),
                };
                let accv = self.new_var();
                let elem = self.new_var();
                let accv2 = self.new_var();
                let clause = self.scoped(cx, |s, cx| {
                    let mut b = Bound::new();
                    let p = s.pat(&pat_e, cx, &mut b)?;
                    s.commit(cx, b);
                    let guard = match &guard_e {
                        Some(g) => Some(s.guard(g, cx)?),
                        None => None,
                    };
                    let rest = s.for_loop(&items[1..], cx, CE::Var(accv), inner)?;
                    Ok(Clause {
                        pats: vec![p, Pat::Bind(accv)],
                        guard,
                        body: rest,
                    })
                })?;
                let skip = Clause {
                    pats: vec![Pat::Bind(elem), Pat::Bind(accv2)],
                    guard: None,
                    body: CE::Var(accv2),
                };
                self.next_lambda += 1;
                let lam = CE::Fn(Rc::new(Lambda {
                    id: self.next_lambda,
                    arity: 2,
                    clauses: vec![clause, skip],
                    params: vec![self.new_var(), self.new_var()],
                    free: vec![],
                    module: cx.module().to_string(),
                    parent: cx.fun.clone(),
                    line: 0,
                }));
                Ok(CE::Call(
                    FunKey::new("Elixir.Enum", "reduce", 3),
                    vec![coll, acc, lam],
                ))
            }
            None => {
                // filter
                let f = self.expr(it, cx)?;
                let accv = self.new_var();
                let rest = self.for_loop(&items[1..], cx, CE::Var(accv), inner)?;
                Ok(CE::Block(vec![
                    CE::Match(Pat::Bind(accv), Box::new(acc)),
                    CE::If(Box::new(f), Box::new(rest), Box::new(CE::Var(accv))),
                ]))
            }
        }
    }

    // ------------------------------------------------------------------
    // patterns
    // ------------------------------------------------------------------

    pub fn pat(&mut self, e: &E, cx: &mut Cx, b: &mut Bound) -> R<Pat> {
        let line = e.line;
        match &e.k {
            K::Var(name) => {
                if name == "_" {
                    return Ok(Pat::Wild);
                }
                if name == "__MODULE__" {
                    return Ok(Pat::Lit(Lit::Atom(cx.module().to_string())));
                }
                if let Some(v) = b.get(name) {
                    return Ok(Pat::Eq(*v));
                }
                let v = self.new_var();
                b.insert(name.clone(), v);
                Ok(Pat::Bind(v))
            }
            K::Un { op, e: x } if op == "^" => match &x.k {
                K::Var(n) => match self.lookup(cx, n) {
                    Some(v) => Ok(Pat::Eq(v)),
                    None => err(cx, line, &format!("undefined variable ^{}", n)),
                },
                _ => err(cx, line, "invalid argument for unary operator ^"),
            },
            K::Int(s) => Ok(Pat::Lit(int_lit(s))),
            K::Float(f) => Ok(Pat::Lit(Lit::Float(*f))),
            K::Atom(a) => Ok(Pat::Lit(Lit::Atom(a.clone()))),
            K::Str(bs) => Ok(Pat::Lit(Lit::Bin(bs.clone()))),
            K::Alias(a) => Ok(Pat::Lit(Lit::Atom(resolve_alias(&cx.env, a)))),
            K::Un { op, e: x } if op == "-" => match &x.k {
                K::Int(s) => Ok(Pat::Lit(int_lit(&format!("-{}", s)))),
                K::Float(f) => Ok(Pat::Lit(Lit::Float(-f))),
                _ => err(cx, line, "invalid pattern"),
            },
            K::Charlist(parts) => {
                let mut bytes = Vec::new();
                for p in parts {
                    match p {
                        IPart::Lit(x) => bytes.extend(x),
                        _ => return err(cx, line, "interpolation is not allowed in patterns"),
                    }
                }
                let mut acc = Pat::Lit(Lit::Nil);
                for ch in String::from_utf8_lossy(&bytes).chars().rev() {
                    acc = Pat::Cons(Box::new(Pat::Lit(Lit::Int(ch as i64))), Box::new(acc));
                }
                Ok(acc)
            }
            K::Sigil { ch, parts, mods } if matches!(ch, 's' | 'S' | 'c' | 'C' | 'w' | 'W') => {
                let ce = self.sigil(*ch, parts, mods, line, cx)?;
                ce_to_pat(&ce).ok_or_else(|| format!("{}:{}: invalid pattern", cx.file(), line))
            }
            K::Tuple(items) => {
                let mut ps = Vec::new();
                for it in items {
                    ps.push(self.pat(it, cx, b)?);
                }
                Ok(Pat::Tuple(ps))
            }
            K::List(items, tail) => {
                let mut acc = match tail {
                    Some(t) => self.pat(t, cx, b)?,
                    None => Pat::Lit(Lit::Nil),
                };
                let mut ps = Vec::new();
                for it in items {
                    ps.push(self.pat(it, cx, b)?);
                }
                for p in ps.into_iter().rev() {
                    acc = Pat::Cons(Box::new(p), Box::new(acc));
                }
                Ok(acc)
            }
            K::Bin { op, l, r } if op == "=" => {
                let a = self.pat(l, cx, b)?;
                let c = self.pat(r, cx, b)?;
                Ok(Pat::Alias(Box::new(a), Box::new(c)))
            }
            K::Bin { op, l, r } if op == "++" => {
                // literal list prefix ++ rest (a charlist literal counts)
                let chars: Option<Vec<E>> = match &l.k {
                    K::Charlist(parts) | K::Sigil { ch: 'c', parts, .. }
                        if parts.iter().all(|p| matches!(p, IPart::Lit(_))) =>
                    {
                        let mut bytes = Vec::new();
                        for p in parts {
                            if let IPart::Lit(bs) = p {
                                bytes.extend_from_slice(bs);
                            }
                        }
                        Some(
                            String::from_utf8_lossy(&bytes)
                                .chars()
                                .map(|c| E::new(K::Int((c as u32).to_string()), line))
                                .collect(),
                        )
                    }
                    _ => None,
                };
                if let Some(items) = chars {
                    let mut acc = self.pat(r, cx, b)?;
                    let mut ps = Vec::new();
                    for it in &items {
                        ps.push(self.pat(it, cx, b)?);
                    }
                    for p in ps.into_iter().rev() {
                        acc = Pat::Cons(Box::new(p), Box::new(acc));
                    }
                    return Ok(acc);
                }
                if let K::List(items, None) = &l.k {
                    let mut acc = self.pat(r, cx, b)?;
                    let mut ps = Vec::new();
                    for it in items {
                        ps.push(self.pat(it, cx, b)?);
                    }
                    for p in ps.into_iter().rev() {
                        acc = Pat::Cons(Box::new(p), Box::new(acc));
                    }
                    return Ok(acc);
                }
                err(
                    cx,
                    line,
                    "invalid pattern: the left side of ++ must be a literal list",
                )
            }
            K::Bin { op, .. } if op == "<>" => {
                let mut segs: Vec<BinSeg<Pat>> = Vec::new();
                let mut cur = e.clone();
                loop {
                    match &cur.k {
                        K::Bin { op, l, r } if op == "<>" => {
                            match &l.k {
                                K::Str(bs) => segs.push(BinSeg {
                                    val: Pat::Lit(Lit::Bin(bs.clone())),
                                    ty: BinType::Bin,
                                    size: None,
                                    little: false,
                                    signed: false,
                                    unit: 8,
                                }),
                                K::Bits(inner) => {
                                    for s in inner {
                                        segs.push(self.bin_seg_pat(s, cx, b)?);
                                    }
                                }
                                _ => {
                                    return err(
                                        cx,
                                        line,
                                        "the left argument of <> operator inside a match should always be a literal binary",
                                    )
                                }
                            }
                            cur = (**r).clone();
                        }
                        _ => break,
                    }
                }
                let rest = self.pat(&cur, cx, b)?;
                match rest {
                    Pat::Lit(Lit::Bin(bs)) => segs.push(BinSeg {
                        val: Pat::Lit(Lit::Bin(bs)),
                        ty: BinType::Bin,
                        size: None,
                        little: false,
                        signed: false,
                        unit: 8,
                    }),
                    Pat::Bin(inner) => segs.extend(inner),
                    p => segs.push(BinSeg {
                        val: p,
                        ty: BinType::Bin,
                        size: None,
                        little: false,
                        signed: false,
                        unit: 8,
                    }),
                }
                Ok(Pat::Bin(segs))
            }
            K::Bin { op, l, r } if op == ".." => {
                let f = self.pat(l, cx, b)?;
                let (last, step) = match &r.k {
                    K::Bin { op, l: rl, r: rr } if op == "//" => {
                        ((**rl).clone(), Some((**rr).clone()))
                    }
                    _ => ((**r).clone(), None),
                };
                let la = self.pat(&last, cx, b)?;
                let mut ps = vec![
                    (
                        CE::atom("__struct__"),
                        Pat::Lit(Lit::Atom("Elixir.Range".into())),
                    ),
                    (CE::atom("first"), f),
                    (CE::atom("last"), la),
                ];
                if let Some(s) = step {
                    ps.push((CE::atom("step"), self.pat(&s, cx, b)?));
                }
                Ok(Pat::Map(ps))
            }
            K::Map(pairs) => {
                let mut ps = Vec::new();
                for (k, v) in pairs {
                    let kc = self.pat_key(k, cx)?;
                    ps.push((kc, self.pat(v, cx, b)?));
                }
                Ok(Pat::Map(ps))
            }
            K::Struct {
                name,
                base: None,
                pairs,
            } => {
                let sp = match &name.k {
                    K::Var(v) if v == "_" => Pat::Wild,
                    K::Un { op, .. } if op == "^" => self.pat(name, cx, b)?,
                    K::Var(v) if v != "__MODULE__" => {
                        let p = self.pat(name, cx, b)?;
                        let _ = v;
                        p
                    }
                    _ => {
                        let m = self.struct_module(name, cx)?;
                        if let Some(fields) =
                            self.mods.get(&m).and_then(|md| md.struct_fields.clone())
                        {
                            for (k, _) in pairs {
                                if let K::Atom(a) = &k.k {
                                    if !fields.iter().any(|(f, _)| f == a) {
                                        return err(
                                            cx,
                                            line,
                                            &format!(
                                                "unknown key :{} for struct {}",
                                                a,
                                                m.trim_start_matches("Elixir.")
                                            ),
                                        );
                                    }
                                }
                            }
                        } else if !self.mods.contains_key(&m) {
                            return err(
                                cx,
                                line,
                                &format!(
                                    "{}.__struct__/1 is undefined, cannot expand struct {}",
                                    m.trim_start_matches("Elixir."),
                                    m.trim_start_matches("Elixir.")
                                ),
                            );
                        }
                        Pat::Lit(Lit::Atom(m))
                    }
                };
                let mut ps = vec![(
                    CE::atom("__struct__"),
                    match sp {
                        Pat::Wild => Pat::Atom(Box::new(Pat::Wild)),
                        p @ Pat::Lit(_) => p,
                        p => Pat::Atom(Box::new(p)),
                    },
                )];
                for (k, v) in pairs {
                    let kc = self.pat_key(k, cx)?;
                    ps.push((kc, self.pat(v, cx, b)?));
                }
                Ok(Pat::Map(ps))
            }
            K::Bits(segs) => {
                let mut out = Vec::new();
                for s in segs {
                    // A nested literal binary (e.g. an expanded `@attr`) is spliced.
                    if let K::Bits(_) = &s.k {
                        if let Pat::Bin(inner) = self.pat(s, cx, b)? {
                            out.extend(inner);
                            continue;
                        }
                    }
                    out.push(self.bin_seg_pat(s, cx, b)?);
                }
                Ok(Pat::Bin(out))
            }
            K::Call { name, args, .. }
                if self.resolve_local(cx, name, args.len()).is_none()
                    && self.macro_target(cx, name, args.len()).is_some() =>
            {
                // macro used in pattern: expand the template and match on it
                let m = self.macro_target(cx, name, args.len()).unwrap();
                let saved_context = cx.macro_context;
                cx.macro_context = Some("match");
                let result = self.macro_ast(&m, name, args, line, cx);
                cx.macro_context = saved_context;
                self.pat(&result?, cx, b)
            }
            K::Block(es) if es.len() == 1 => self.pat(&es[0], cx, b),
            K::Call { name, args, .. }
                if name == "var!" && (args.len() == 1 || args.len() == 2) =>
            {
                self.pat(&ctx_var(args), cx, b)
            }
            K::Remote {
                recv, name, args, ..
            } if name == "var!"
                && matches!(&recv.k, K::Alias(a) if a == "Kernel")
                && (args.len() == 1 || args.len() == 2) =>
            {
                self.pat(&args[0], cx, b)
            }
            K::Interp(_) => err(cx, line, "interpolation is not allowed in patterns"),
            _ => match const_fold(e) {
                Some(f) if !matches!(f.k, K::Bin { .. } | K::Un { .. }) => self.pat(&f, cx, b),
                _ => err(cx, line, &format!("invalid pattern: {}", describe_expr(e))),
            },
        }
    }

    fn pat_key(&mut self, k: &E, cx: &mut Cx) -> R<CE> {
        match &k.k {
            K::Un { op, e } if op == "^" => {
                if let K::Var(n) = &e.k {
                    return match self.lookup(cx, n) {
                        Some(v) => Ok(CE::Var(v)),
                        None => err(cx, k.line, &format!("undefined variable ^{}", n)),
                    };
                }
                err(cx, k.line, "invalid map key")
            }
            K::Var(_) => err(cx, k.line, "illegal use of variable in map key inside match, variables can only be used as map keys in a match if they are pinned with ^"),
            _ => {
                let saved = cx.guard;
                cx.guard = true;
                let r = self.expr(k, cx);
                cx.guard = saved;
                r
            }
        }
    }

    fn bin_seg_pat(&mut self, s: &E, cx: &mut Cx, b: &mut Bound) -> R<BinSeg<Pat>> {
        let (v, spec) = match &s.k {
            K::Bin { op, l, r } if op == "::" => ((**l).clone(), Some((**r).clone())),
            _ => (s.clone(), None),
        };
        let is_str = matches!(v.k, K::Str(_));
        let sp = self.seg_spec(spec.as_ref(), cx, true, Some(b))?;
        if let (K::Str(bytes), Some(t @ (BinType::Utf8 | BinType::Utf16 | BinType::Utf32))) =
            (&v.k, &sp.0)
        {
            let enc = encode_str_seg(bytes, t, sp.3);
            return Ok(BinSeg {
                val: Pat::Lit(Lit::Bin(enc)),
                ty: BinType::Bin,
                size: None,
                little: false,
                signed: false,
                unit: 8,
            });
        }
        let p = self.pat(&v, cx, b)?;
        Ok(self.finish_seg(p, is_str, sp))
    }
}

fn mul_const(e: CE, k: i64) -> CE {
    if k == 1 {
        return e;
    }
    match e {
        CE::Lit(Lit::Int(v)) => CE::int(v * k),
        e => CE::bif("tn_mul", vec![e, CE::int(k)]),
    }
}

fn div_const(e: CE, k: i64) -> CE {
    match e {
        CE::Lit(Lit::Int(v)) => CE::int(v / k),
        e => CE::bif("tn_div", vec![e, CE::int(k)]),
    }
}

fn flatten_dash(e: &E, out: &mut Vec<E>) {
    match &e.k {
        K::Bin { op, l, r } if op == "-" => {
            flatten_dash(l, out);
            flatten_dash(r, out);
        }
        _ => out.push(e.clone()),
    }
}

pub fn charlist_lit(bytes: &[u8]) -> CE {
    let s = String::from_utf8_lossy(bytes);
    let mut acc = CE::Lit(Lit::Nil);
    for ch in s.chars().rev() {
        acc = CE::Cons(Box::new(CE::int(ch as i64)), Box::new(acc));
    }
    acc
}

fn ce_to_pat(c: &CE) -> Option<Pat> {
    match c {
        CE::Lit(l) => Some(Pat::Lit(l.clone())),
        CE::Cons(h, t) => Some(Pat::Cons(Box::new(ce_to_pat(h)?), Box::new(ce_to_pat(t)?))),
        CE::Tuple(items) => Some(Pat::Tuple(
            items.iter().map(ce_to_pat).collect::<Option<Vec<_>>>()?,
        )),
        _ => None,
    }
}

fn max_cap_arg(e: &E) -> u32 {
    let mut m = 0;
    let root = e as *const E;
    map_expr(e, &mut |x: &E| {
        if let K::CapArg(n) = &x.k {
            m = m.max(*n);
        }
        if !std::ptr::eq(x, root) {
            if let K::Capture(_) = &x.k {
                return Some(x.clone());
            }
        }
        None
    });
    m
}

/// Insert `l` as the first argument of the call `r`.
pub fn pipe_into(l: &E, r: &E) -> Option<E> {
    let line = r.line;
    match &r.k {
        K::Call { name, args, parens } => {
            let mut a = vec![l.clone()];
            a.extend(args.iter().cloned());
            Some(E::new(
                K::Call {
                    name: name.clone(),
                    args: a,
                    parens: *parens,
                },
                line,
            ))
        }
        K::Remote {
            recv, name, args, ..
        } => {
            let mut a = vec![l.clone()];
            a.extend(args.iter().cloned());
            Some(E::new(
                K::Remote {
                    recv: recv.clone(),
                    name: name.clone(),
                    args: a,
                    parens: true,
                },
                line,
            ))
        }
        K::AnonCall { f, args } => {
            let mut a = vec![l.clone()];
            a.extend(args.iter().cloned());
            Some(E::new(
                K::AnonCall {
                    f: f.clone(),
                    args: a,
                },
                line,
            ))
        }
        // `x |> (a |> b)` pipes x into the leftmost call: `(x |> a) |> b`.
        K::Bin { op, l: il, r: ir } if op == "|>" => {
            let inner = pipe_into(l, il)?;
            Some(E::bin("|>", inner, (**ir).clone(), line))
        }
        K::Block(es) if es.len() == 1 => pipe_into(l, &es[0]),
        K::Var(name) => Some(E::new(
            K::Call {
                name: name.clone(),
                args: vec![l.clone()],
                parens: true,
            },
            line,
        )),
        K::Bin { op, l: bl, r: br } if op != "|>" => {
            // `x |> a + b` is not valid Elixir either; treat as call of operator
            let _ = (bl, br);
            None
        }
        _ => None,
    }
}

pub fn describe_expr(e: &E) -> String {
    match &e.k {
        K::Call { name, args, .. } => format!("{}/{}", name, args.len()),
        K::Remote { name, .. } => format!("remote call .{}", name),
        K::Bin { op, .. } => format!("operator {}", op),
        K::Interp(_) => "string interpolation".into(),
        K::Fn(_) => "fn".into(),
        _ => "expression".into(),
    }
}

/// Fold constant integer/float arithmetic (after attribute substitution).
pub fn const_fold(e: &E) -> Option<E> {
    match &e.k {
        K::Int(_) | K::Float(_) => Some(e.clone()),
        K::Block(es) if es.len() == 1 => const_fold(&es[0]),
        K::Un { op, e: x } if op == "-" || op == "+" => {
            let v = const_fold(x)?;
            match (&v.k, op.as_str()) {
                (K::Int(n), "-") => Some(E::new(K::Int(neg_int_str(n)), e.line)),
                (K::Float(f), "-") => Some(E::new(K::Float(-f), e.line)),
                _ => Some(v),
            }
        }
        K::Bin { op, l, r } if matches!(op.as_str(), "+" | "-" | "*" | "div" | "rem") => {
            let a = const_fold(l)?;
            let b = const_fold(r)?;
            match (&a.k, &b.k) {
                (K::Int(x), K::Int(y)) => {
                    let x: i128 = x.replace('_', "").parse().ok()?;
                    let y: i128 = y.replace('_', "").parse().ok()?;
                    let v = match op.as_str() {
                        "+" => x.checked_add(y)?,
                        "-" => x.checked_sub(y)?,
                        "*" => x.checked_mul(y)?,
                        _ => return None,
                    };
                    Some(E::new(K::Int(v.to_string()), e.line))
                }
                _ => None,
            }
        }
        K::Call { name, args, .. } if (name == "div" || name == "rem") && args.len() == 2 => {
            let a = const_fold(&args[0])?;
            let b = const_fold(&args[1])?;
            match (&a.k, &b.k) {
                (K::Int(x), K::Int(y)) => {
                    let x: i128 = x.replace('_', "").parse().ok()?;
                    let y: i128 = y.replace('_', "").parse().ok()?;
                    if y == 0 {
                        return None;
                    }
                    let v = if name == "div" { x / y } else { x % y };
                    Some(E::new(K::Int(v.to_string()), e.line))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn neg_int_str(n: &str) -> String {
    match n.strip_prefix('-') {
        Some(r) => r.to_string(),
        None => format!("-{}", n),
    }
}

/// `~c"abc"`, `~w(a b)a`, `'abc'`-style literals as a list expression.
fn literal_list(e: &E) -> Option<E> {
    let lit_parts = |parts: &Vec<IPart>| -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for p in parts {
            match p {
                IPart::Lit(b) => out.extend_from_slice(b),
                _ => return None,
            }
        }
        Some(out)
    };
    match &e.k {
        K::Charlist(parts) | K::Sigil { ch: 'c', parts, .. } => {
            let b = lit_parts(parts)?;
            let s = String::from_utf8(b).ok()?;
            Some(E::new(
                K::List(
                    s.chars()
                        .map(|c| E::new(K::Int((c as u32).to_string()), e.line))
                        .collect(),
                    None,
                ),
                e.line,
            ))
        }
        K::Sigil {
            ch: 'w',
            parts,
            mods,
        } => {
            let b = lit_parts(parts)?;
            let s = String::from_utf8(b).ok()?;
            let items = s
                .split_whitespace()
                .map(|w| match mods.as_str() {
                    "a" => E::new(K::Atom(w.to_string()), e.line),
                    _ => E::new(K::Str(w.as_bytes().to_vec()), e.line),
                })
                .collect();
            Some(E::new(K::List(items, None), e.line))
        }
        _ => None,
    }
}

fn parse_iso_date(s: &str) -> Option<(i64, i64, i64)> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let parts: Vec<&str> = body.split('-').collect();
    if parts.len() != 3 || parts[0].len() < 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    let y = if neg { -y } else { y };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let dim = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&m) || d < 1 || d > dim[(m - 1) as usize] {
        return None;
    }
    Some((y, m, d))
}

/// HH:MM:SS(.ffffff)? -> (h, m, s, usec, precision)
fn parse_iso_time(s: &str) -> Option<(i64, i64, i64, i64, i64)> {
    let (main, frac) = match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let parts: Vec<&str> = main.split(':').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.len() != 2) {
        return None;
    }
    let h: i64 = parts[0].parse().ok()?;
    let mi: i64 = parts[1].parse().ok()?;
    let se: i64 = parts[2].parse().ok()?;
    if h > 23 || mi > 59 || se > 59 {
        return None;
    }
    let (us, prec) = match frac {
        None => (0, 0),
        Some(f) => {
            if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let p = f.len().min(6);
            let mut digits = f[..p].to_string();
            while digits.len() < 6 {
                digits.push('0');
            }
            (digits.parse().ok()?, p as i64)
        }
    };
    Some((h, mi, se, us, prec))
}

fn calendar_sigil(ch: char, text: &str, line: u32) -> Option<E> {
    let int = |v: i64| E::new(K::Int(v.to_string()), line);
    let key = |k: &str| E::new(K::Atom(k.to_string()), line);
    let iso = E::new(K::Alias("Calendar.ISO".into()), line);
    let mk = |name: &str, pairs: Vec<(E, E)>| {
        E::new(
            K::Struct {
                name: Box::new(E::new(K::Alias(name.into()), line)),
                base: None,
                pairs,
            },
            line,
        )
    };
    let usec = |us: i64, p: i64| E::new(K::Tuple(vec![int(us), int(p)]), line);
    match ch {
        'D' => {
            let (y, m, d) = parse_iso_date(text)?;
            Some(mk(
                "Date",
                vec![
                    (key("year"), int(y)),
                    (key("month"), int(m)),
                    (key("day"), int(d)),
                    (key("calendar"), iso),
                ],
            ))
        }
        'T' => {
            let (h, mi, s, us, p) = parse_iso_time(text)?;
            Some(mk(
                "Time",
                vec![
                    (key("hour"), int(h)),
                    (key("minute"), int(mi)),
                    (key("second"), int(s)),
                    (key("microsecond"), usec(us, p)),
                    (key("calendar"), iso),
                ],
            ))
        }
        'N' | 'U' => {
            let sep = text.find(|c| c == ' ' || c == 'T')?;
            let (date, rest) = (&text[..sep], &text[sep + 1..]);
            let (y, m, d) = parse_iso_date(date)?;
            let time = if ch == 'U' {
                if let Some(t) = rest.strip_suffix('Z') {
                    t
                } else if let Some(t) = rest.strip_suffix("+00:00") {
                    t
                } else {
                    return None;
                }
            } else {
                rest
            };
            let (h, mi, s, us, p) = parse_iso_time(time)?;
            let mut pairs = vec![
                (key("year"), int(y)),
                (key("month"), int(m)),
                (key("day"), int(d)),
                (key("hour"), int(h)),
                (key("minute"), int(mi)),
                (key("second"), int(s)),
                (key("microsecond"), usec(us, p)),
                (key("calendar"), iso),
            ];
            if ch == 'N' {
                Some(mk("NaiveDateTime", pairs))
            } else {
                pairs.push((key("time_zone"), E::new(K::Str(b"Etc/UTC".to_vec()), line)));
                pairs.push((key("zone_abbr"), E::new(K::Str(b"UTC".to_vec()), line)));
                pairs.push((key("utc_offset"), int(0)));
                pairs.push((key("std_offset"), int(0)));
                Some(mk("DateTime", pairs))
            }
        }
        _ => None,
    }
}

/// A string literal segment with a utf8/utf16/utf32 type, encoded at compile time.
fn encode_str_seg(bytes: &[u8], t: &BinType, little: bool) -> Vec<u8> {
    let s = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for ch in s.chars() {
        match t {
            BinType::Utf8 => {
                let mut b = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
            }
            BinType::Utf16 => {
                let mut b = [0u16; 2];
                for u in ch.encode_utf16(&mut b).iter() {
                    out.extend_from_slice(&if little {
                        u.to_le_bytes()
                    } else {
                        u.to_be_bytes()
                    });
                }
            }
            _ => {
                let u = ch as u32;
                out.extend_from_slice(&if little {
                    u.to_le_bytes()
                } else {
                    u.to_be_bytes()
                });
            }
        }
    }
    out
}

/// A macro body built only from its parameters and literal data (atoms,
/// numbers, strings, lists, 2-tuples) evaluates to code equal to itself with
/// the parameters' ASTs spliced in; returns that template.
fn literal_template(e: &E) -> Option<E> {
    let l = e.line;
    Some(match &e.k {
        K::Var(v) => E::call("unquote", vec![E::var(v, l)], l),
        K::Int(_) | K::Float(_) | K::Atom(_) | K::Str(_) => e.clone(),
        K::Tuple(items) if items.len() == 2 => E::new(
            K::Tuple(
                items
                    .iter()
                    .map(literal_template)
                    .collect::<Option<Vec<_>>>()?,
            ),
            l,
        ),
        K::List(items, tail) => {
            let items = items
                .iter()
                .map(literal_template)
                .collect::<Option<Vec<_>>>()?;
            let tail = match tail {
                Some(t) => Some(Box::new(literal_template(t)?)),
                None => None,
            };
            E::new(K::List(items, tail), l)
        }
        K::Block(es) if es.len() == 1 => literal_template(&es[0])?,
        _ => return None,
    })
}

/// Absolute form of a source path (as __ENV__.file / __DIR__ report it).
fn expand_path(f: &str) -> String {
    let p = std::path::Path::new(f);
    if p.is_absolute() {
        return f.to_string();
    }
    match std::env::current_dir() {
        Ok(d) => d.join(p).to_string_lossy().into_owned(),
        Err(_) => f.to_string(),
    }
}

/// Kernel.dbg/0,1,2 as Macro.dbg/3 expands it: build the `to_debug` term
/// (`{:pipe, asts, values}` or `{:value, ast, value}`) inside a closure so
/// the helper variables stay out of the caller's scope, then hand it to
/// Macro.__dbg__/3 with the caller's __ENV__.
fn dbg_expansion(args: &[E], line: u32) -> E {
    let code = args
        .get(0)
        .cloned()
        .unwrap_or_else(|| E::call("binding", vec![], line));
    let opts = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| E::list(vec![], line));
    let quote = |e: &E| {
        E::call(
            "quote",
            vec![E::list(
                vec![E::tuple(vec![E::atom("do", line), e.clone()], line)],
                line,
            )],
            line,
        )
    };
    let mut steps = Vec::new();
    let mut cur = code.clone();
    loop {
        match &cur.k {
            K::Bin { op, l, r } if op == "|>" => {
                steps.push((**r).clone());
                cur = (**l).clone();
            }
            _ => break,
        }
    }
    let to_debug = if steps.is_empty() {
        E::tuple(
            vec![E::atom("value", line), quote(&code), code.clone()],
            line,
        )
    } else {
        steps.reverse();
        let v = E::var("dbg_value__", line);
        let acc = E::var("dbg_values__", line);
        let mut stmts = vec![
            E::bin("=", v.clone(), cur.clone(), line),
            E::bin("=", acc.clone(), E::list(vec![v.clone()], line), line),
        ];
        let mut asts = vec![quote(&cur)];
        for s in &steps {
            asts.push(quote(s));
            let call = pipe_into(&v, s).unwrap_or_else(|| s.clone());
            stmts.push(E::bin("=", v.clone(), call, line));
            stmts.push(E::bin(
                "=",
                acc.clone(),
                E::new(K::List(vec![v.clone()], Some(Box::new(acc.clone()))), line),
                line,
            ));
        }
        stmts.push(E::tuple(
            vec![
                E::atom("pipe", line),
                E::list(asts, line),
                E::remote("Enum", "reverse", vec![acc], line),
            ],
            line,
        ));
        let f = E::new(
            K::Fn(vec![crate::ast::Clause {
                args: vec![],
                guard: None,
                body: E::new(K::Block(stmts), line),
                line,
            }]),
            line,
        );
        E::new(
            K::AnonCall {
                f: Box::new(f),
                args: vec![],
            },
            line,
        )
    };
    let header = E::remote(
        "Tonic.Internal",
        "dbg_header",
        vec![E::var("__ENV__", line)],
        line,
    );
    E::remote("Macro", "__dbg__", vec![header, to_debug, opts], line)
}

/// Does evaluating `e` bind variables at this level (a `=` outside closures)?
fn has_match(e: &E) -> bool {
    let mut found = false;
    fn walk(e: &E, found: &mut bool) {
        if *found {
            return;
        }
        match &e.k {
            K::Bin { op, .. } if op == "=" => *found = true,
            K::Fn(_) | K::Capture(_) => {}
            K::Call { name, args, .. } => {
                if !matches!(
                    name.as_str(),
                    "quote" | "case" | "cond" | "receive" | "try" | "for" | "with" | "fn"
                ) {
                    for a in args {
                        walk(a, found);
                    }
                }
            }
            K::Bin { l, r, .. } => {
                walk(l, found);
                walk(r, found);
            }
            K::Un { e, .. } => walk(e, found),
            K::Tuple(xs) => xs.iter().for_each(|x| walk(x, found)),
            K::List(xs, t) => {
                xs.iter().for_each(|x| walk(x, found));
                if let Some(t) = t {
                    walk(t, found);
                }
            }
            K::Remote { recv, args, .. } => {
                walk(recv, found);
                args.iter().for_each(|x| walk(x, found));
            }
            K::Map(ps) => ps.iter().for_each(|(a, b)| {
                walk(a, found);
                walk(b, found);
            }),
            K::Block(xs) => xs.iter().for_each(|x| walk(x, found)),
            _ => {}
        }
    }
    walk(e, &mut found);
    found
}

/// Macro parameters the template expander binds: variables, defaults and
/// keyword lists of variables (`do: body`).
fn simple_params(ps: &[E]) -> bool {
    ps.iter().all(|p| match &p.k {
        K::Var(_) => true,
        K::Bin { op, l, .. } if op == "\\\\" => matches!(l.k, K::Var(_)),
        _ => p
            .as_keyword()
            .map(|kw| kw.iter().all(|(_, v)| matches!(v.k, K::Var(_))))
            .unwrap_or(false),
    })
}

/// Renames the variables of a macro template (outside unquote/var!) so
/// they cannot clash with the caller's.
fn hygienize(e: &E, n: usize) -> E {
    crate::collect::map_expr(e, &mut |x: &E| match &x.k {
        K::Call { name, .. }
            if name == "unquote"
                || name == "var!"
                || name == "unquote_splicing"
                || name == "quote" =>
        {
            Some(x.clone())
        }
        K::Bin { op, l, r } if op == "::" => {
            let left = hygienize(l, n);
            let right = crate::collect::map_expr(r, &mut |spec| match &spec.k {
                K::Call { name, .. } if matches!(name.as_str(), "size" | "unit") => Some(hygienize(spec, n)),
                K::Var(name) if matches!(name.as_str(), "integer" | "float" | "binary" | "bitstring" | "bytes" | "bits" | "utf8" | "utf16" | "utf32" | "signed" | "unsigned" | "native" | "big" | "little") => Some(spec.clone()),
                K::Var(_) => Some(hygienize(spec, n)),
                _ => None,
            });
            Some(E::new(K::Bin { op: op.clone(), l: Box::new(left), r: Box::new(right) }, x.line))
        }
        K::Var(v) if v != "_" && !(v.starts_with("__") && v.ends_with("__")) => {
            Some(E::new(K::Var(format!("{}__h{}", v, n)), x.line))
        }
        _ => None,
    })
}

/// var!(x) / var!(x, ctx): a variable of context `ctx` is named "x@ctx".
fn ctx_var(args: &[E]) -> E {
    match (&args[0].k, args.get(1).map(|c| &c.k)) {
        (K::Var(v), Some(K::Atom(c))) if c != "nil" => {
            E::new(K::Var(format!("{}@{}", v, c)), args[0].line)
        }
        (K::Var(v), Some(K::Alias(c))) => {
            E::new(K::Var(format!("{}@Elixir.{}", v, c)), args[0].line)
        }
        _ => args[0].clone(),
    }
}

/// Variables renamed by macro hygiene ("x__h12").
fn is_hygienic_name(k: &str) -> bool {
    match k.rfind("__h") {
        Some(i) => !k[i + 3..].is_empty() && k[i + 3..].chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}
