//! Erlang preprocessor (epp subset): -define/-undef, -ifdef/-ifndef/-if/
//! -elif/-else/-endif, -include/-include_lib and macro expansion.

use crate::erl_lex::{tokenize, Tok, T};
use std::collections::HashMap;

#[derive(Clone)]
struct Macro {
    params: Option<Vec<String>>,
    body: Vec<Tok>,
}

pub struct Pp<'a> {
    file: String,
    module: String,
    macros: HashMap<(String, Option<usize>), Macro>,
    includes: &'a dyn Fn(&str) -> Option<String>,
}

fn p(t: &Tok, s: &str) -> bool {
    matches!(&t.t, T::P(x) if x == s)
}

/// Split the token stream into forms (each ending with its Dot token).
fn split_forms(toks: Vec<Tok>) -> Vec<Vec<Tok>> {
    let mut forms = Vec::new();
    let mut cur = Vec::new();
    for t in toks {
        match t.t {
            T::Eof => break,
            T::Dot => {
                cur.push(t);
                forms.push(std::mem::take(&mut cur));
            }
            _ => cur.push(t),
        }
    }
    if !cur.is_empty() {
        forms.push(cur);
    }
    forms
}

impl<'a> Pp<'a> {
    pub fn new(file: &str, includes: &'a dyn Fn(&str) -> Option<String>) -> Pp<'a> {
        Pp { file: file.to_string(), module: String::new(), macros: HashMap::new(), includes }
    }

    pub fn run(&mut self, src: &str) -> Result<Vec<Tok>, String> {
        let toks = tokenize(src, &self.file)?;
        let mut out = Vec::new();
        let mut conds: Vec<(bool, bool)> = Vec::new(); // (active, any branch taken)
        self.process(split_forms(toks), &mut out, &mut conds)?;
        out.push(Tok { t: T::Eof, line: out.last().map(|t| t.line).unwrap_or(1) });
        Ok(out)
    }

    fn active(conds: &[(bool, bool)]) -> bool {
        conds.iter().all(|c| c.0)
    }

    fn process(&mut self, forms: Vec<Vec<Tok>>, out: &mut Vec<Tok>, conds: &mut Vec<(bool, bool)>) -> Result<(), String> {
        for form in forms {
            if form.len() >= 2 && p(&form[0], "-") {
                let dname = match &form[1].t {
                    T::Atom(a) => Some(a.clone()),
                    T::P(k) if k == "if" || k == "else" => Some(k.clone()),
                    _ => None,
                };
                if let Some(d) = dname {
                    let line = form[0].line;
                    match d.as_str() {
                        "ifdef" | "ifndef" => {
                            let name = self.directive_name(&form)?;
                            let def = self.macros.keys().any(|(n, _)| *n == name) || self.predefined(&name);
                            let v = if d == "ifdef" { def } else { !def };
                            let on = Self::active(conds);
                            conds.push((v && on, v));
                            continue;
                        }
                        "if" | "elif" => {
                            if d == "elif" {
                                let (_, taken) = conds.pop().ok_or("elif without if")?;
                                let on = Self::active(conds);
                                if taken {
                                    conds.push((false, true));
                                } else {
                                    let v = self.eval_cond(&form[2..form.len() - 1])?;
                                    conds.push((v && on, v));
                                }
                            } else {
                                let on = Self::active(conds);
                                let v = if on { self.eval_cond(&form[2..form.len() - 1])? } else { false };
                                conds.push((v && on, v));
                            }
                            continue;
                        }
                        "else" => {
                            let (_, taken) = conds.pop().ok_or("else without if")?;
                            let on = Self::active(conds);
                            conds.push((!taken && on, true));
                            continue;
                        }
                        "endif" => {
                            conds.pop();
                            continue;
                        }
                        _ => {}
                    }
                    if !Self::active(conds) {
                        continue;
                    }
                    match d.as_str() {
                        "define" => {
                            self.define(&form)?;
                            continue;
                        }
                        "undef" => {
                            let name = self.directive_name(&form)?;
                            self.macros.retain(|(n, _), _| *n != name);
                            continue;
                        }
                        "include" | "include_lib" => {
                            let path = match form.get(3).map(|t| &t.t) {
                                Some(T::Str(cps)) => cps.iter().filter_map(|c| char::from_u32(*c)).collect::<String>(),
                                _ => return Err(format!("{}:{}: bad include", self.file, line)),
                            };
                            let base = path.rsplit('/').next().unwrap_or(&path).to_string();
                            let src = (self.includes)(&base)
                                .ok_or_else(|| format!("{}:{}: can't find include file {:?}", self.file, line, path))?;
                            let saved = std::mem::replace(&mut self.file, base.clone());
                            let toks = tokenize(&src, &base)?;
                            let r = self.process(split_forms(toks), out, conds);
                            self.file = saved;
                            r?;
                            continue;
                        }
                        "error" => return Err(format!("{}:{}: -error directive", self.file, line)),
                        "warning" => continue,
                        "module" => {
                            if let Some(T::Atom(m)) = form.get(3).map(|t| &t.t) {
                                self.module = m.clone();
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !Self::active(conds) {
                continue;
            }
            let (fname, farity) = function_info(&form);
            let expanded = self.expand(&form, &fname, farity, 0)?;
            out.extend(expanded);
        }
        Ok(())
    }

    fn predefined(&self, n: &str) -> bool {
        matches!(n, "MODULE" | "MODULE_STRING" | "FILE" | "LINE" | "MACHINE" | "FUNCTION_NAME" | "FUNCTION_ARITY" | "OTP_RELEASE" | "BEAM")
    }

    fn directive_name(&self, form: &[Tok]) -> Result<String, String> {
        match form.get(3).map(|t| &t.t) {
            Some(T::Atom(a)) | Some(T::Var(a)) => Ok(a.clone()),
            _ => Err(format!("{}:{}: bad directive", self.file, form[0].line)),
        }
    }

    fn define(&mut self, form: &[Tok]) -> Result<(), String> {
        // - define ( Name [ ( Params ) ] , Body ) .
        let line = form[0].line;
        let name = match form.get(3).map(|t| &t.t) {
            Some(T::Atom(a)) | Some(T::Var(a)) => a.clone(),
            _ => return Err(format!("{}:{}: bad -define", self.file, line)),
        };
        let mut i = 4;
        let mut params = None;
        if form.get(i).map(|t| p(t, "(")).unwrap_or(false) {
            let mut ps = Vec::new();
            i += 1;
            while i < form.len() && !p(&form[i], ")") {
                if let T::Var(v) = &form[i].t {
                    ps.push(v.clone());
                }
                i += 1;
            }
            i += 1;
            params = Some(ps);
        }
        if !form.get(i).map(|t| p(t, ",")).unwrap_or(false) {
            return Err(format!("{}:{}: bad -define body", self.file, line));
        }
        // body: up to the final ')' before the Dot
        let end = form.len() - 2;
        let body = form[i + 1..end].to_vec();
        let arity = params.as_ref().map(|p| p.len());
        self.macros.insert((name, arity), Macro { params, body });
        Ok(())
    }

    fn eval_cond(&mut self, toks: &[Tok]) -> Result<bool, String> {
        let toks = self.expand(toks, "", 0, 0)?;
        // tiny evaluator: [not] A [op B] with ints/atoms; `defined(X)` unsupported -> false
        let vals: Vec<&T> = toks.iter().map(|t| &t.t).collect();
        fn num(t: &T) -> Option<i128> {
            match t {
                T::Int(v) => Some(*v),
                _ => None,
            }
        }
        // strip outer parens
        let mut v: Vec<&T> = vals;
        while v.len() >= 2 && matches!(v[0], T::P(x) if x == "(") && matches!(v[v.len() - 1], T::P(x) if x == ")") {
            v = v[1..v.len() - 1].to_vec();
        }
        match v.as_slice() {
            [T::Atom(a)] => Ok(a == "true"),
            [a, T::P(op), b] => {
                let (x, y) = match (num(a), num(b)) {
                    (Some(x), Some(y)) => (x, y),
                    _ => return Ok(false),
                };
                Ok(match op.as_str() {
                    ">=" => x >= y,
                    ">" => x > y,
                    "<" => x < y,
                    "=<" => x <= y,
                    "==" | "=:=" => x == y,
                    "/=" | "=/=" => x != y,
                    _ => false,
                })
            }
            _ => Ok(false),
        }
    }

    fn expand(&mut self, toks: &[Tok], fname: &str, farity: usize, depth: usize) -> Result<Vec<Tok>, String> {
        if depth > 50 {
            return Err(format!("{}: macro expansion too deep", self.file));
        }
        let mut out = Vec::with_capacity(toks.len());
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            if p(t, "?") || p(t, "??") {
                let stringify = p(t, "??");
                let line = t.line;
                let name = match toks.get(i + 1).map(|t| &t.t) {
                    Some(T::Atom(a)) | Some(T::Var(a)) => a.clone(),
                    Some(T::P(k)) if k == "if" => k.clone(),
                    _ => return Err(format!("{}:{}: bad macro", self.file, line)),
                };
                if stringify {
                    // ??Arg is only meaningful inside macro bodies (handled at substitution)
                    out.push(Tok { t: T::Str(name.chars().map(|c| c as u32).collect()), line });
                    i += 2;
                    continue;
                }
                i += 2;
                // predefined
                let pre = match name.as_str() {
                    "MODULE" => Some(T::Atom(self.module.clone())),
                    "MODULE_STRING" => Some(T::Str(self.module.chars().map(|c| c as u32).collect())),
                    "FILE" => Some(T::Str(self.file.chars().map(|c| c as u32).collect())),
                    "LINE" => Some(T::Int(line as i128)),
                    "MACHINE" => Some(T::Atom("BEAM".into())),
                    "FUNCTION_NAME" => Some(T::Atom(fname.to_string())),
                    "FUNCTION_ARITY" => Some(T::Int(farity as i128)),
                    "OTP_RELEASE" => Some(T::Int(27)),
                    _ => None,
                };
                if let Some(pt) = pre {
                    if !self.macros.contains_key(&(name.clone(), None)) {
                        out.push(Tok { t: pt, line });
                        continue;
                    }
                }
                // with arguments?
                if toks.get(i).map(|t| p(t, "(")).unwrap_or(false) {
                    let (args, ni) = collect_args(toks, i)?;
                    if let Some(m) = self.macros.get(&(name.clone(), Some(args.len()))).cloned() {
                        i = ni;
                        let params = m.params.unwrap_or_default();
                        let mut body = Vec::new();
                        let mut k = 0;
                        while k < m.body.len() {
                            let bt = &m.body[k];
                            if p(bt, "??") {
                                if let Some(T::Var(v)) = m.body.get(k + 1).map(|t| &t.t) {
                                    if let Some(pi) = params.iter().position(|x| x == v) {
                                        let s = tokens_to_string(&args[pi]);
                                        body.push(Tok { t: T::Str(s.chars().map(|c| c as u32).collect()), line });
                                        k += 2;
                                        continue;
                                    }
                                }
                            }
                            if let T::Var(v) = &bt.t {
                                if let Some(pi) = params.iter().position(|x| x == v) {
                                    body.extend(args[pi].iter().cloned());
                                    k += 1;
                                    continue;
                                }
                            }
                            body.push(Tok { t: bt.t.clone(), line });
                            k += 1;
                        }
                        let e = self.expand(&body, fname, farity, depth + 1)?;
                        out.extend(e);
                        continue;
                    }
                }
                match self.macros.get(&(name.clone(), None)).cloned() {
                    Some(m) => {
                        let body: Vec<Tok> = m.body.iter().map(|bt| Tok { t: bt.t.clone(), line }).collect();
                        let e = self.expand(&body, fname, farity, depth + 1)?;
                        out.extend(e);
                    }
                    None => return Err(format!("{}:{}: undefined macro '{}'", self.file, line, name)),
                }
                continue;
            }
            out.push(t.clone());
            i += 1;
        }
        Ok(out)
    }
}

/// Collect macro call arguments starting at `(` (index i). Returns args and
/// the index after the closing `)`.
fn collect_args(toks: &[Tok], i: usize) -> Result<(Vec<Vec<Tok>>, usize), String> {
    let mut depth = 0i32;
    let mut args: Vec<Vec<Tok>> = Vec::new();
    let mut cur = Vec::new();
    let mut j = i + 1;
    if toks.get(j).map(|t| p(t, ")")).unwrap_or(false) {
        return Ok((vec![], j + 1));
    }
    while j < toks.len() {
        let t = &toks[j];
        if let T::P(x) = &t.t {
            match x.as_str() {
                "(" | "[" | "{" | "<<" | "begin" | "case" | "if" | "receive" | "try" | "maybe" => depth += 1,
                "fun" => {
                    // `fun (` / `fun Name(` opens a block; `fun f/1` does not
                    if toks.get(j + 1).map(|t| p(t, "(")).unwrap_or(false)
                        || (matches!(toks.get(j + 1).map(|t| &t.t), Some(T::Var(_))) && toks.get(j + 2).map(|t| p(t, "(")).unwrap_or(false))
                    {
                        depth += 1;
                    }
                }
                ")" | "]" | "}" | ">>" | "end" => {
                    if depth == 0 {
                        args.push(std::mem::take(&mut cur));
                        return Ok((args, j + 1));
                    }
                    depth -= 1;
                }
                "," if depth == 0 => {
                    args.push(std::mem::take(&mut cur));
                    j += 1;
                    continue;
                }
                _ => {}
            }
        }
        cur.push(t.clone());
        j += 1;
    }
    Err("unterminated macro call".into())
}

fn tokens_to_string(ts: &[Tok]) -> String {
    let mut s = String::new();
    for t in ts {
        let x = match &t.t {
            T::Atom(a) => a.clone(),
            T::Var(v) => v.clone(),
            T::Int(i) => i.to_string(),
            T::Float(f) => f.to_string(),
            T::Char(c) => format!("${}", char::from_u32(*c).unwrap_or('?')),
            T::Str(cps) => format!("\"{}\"", cps.iter().filter_map(|c| char::from_u32(*c)).collect::<String>()),
            T::P(p) => p.clone(),
            _ => String::new(),
        };
        s.push_str(&x);
    }
    s
}

/// Name and arity of a function form (for ?FUNCTION_NAME/?FUNCTION_ARITY).
fn function_info(form: &[Tok]) -> (String, usize) {
    if let (Some(T::Atom(n)), Some(t1)) = (form.first().map(|t| &t.t), form.get(1)) {
        if p(t1, "(") {
            let mut depth = 0;
            let mut arity = 0;
            let mut any = false;
            for t in &form[2..] {
                if let T::P(x) = &t.t {
                    match x.as_str() {
                        "(" | "[" | "{" | "<<" => depth += 1,
                        ")" | "]" | "}" | ">>" => {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                        }
                        "," if depth == 0 => {
                            arity += 1;
                            continue;
                        }
                        _ => {}
                    }
                }
                any = true;
            }
            return (n.clone(), if any { arity + 1 } else { 0 });
        }
    }
    (String::new(), 0)
}
