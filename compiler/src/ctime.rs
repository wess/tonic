//! A small compile-time evaluator for module-level code: variables bound to
//! literal values and `for` comprehensions that generate definitions with
//! `unquote/1` (a common pattern in Elixir's own standard library).

use crate::ast::*;
use num_bigint::BigInt;
use std::collections::HashMap;

pub type Vars = HashMap<String, E>;

fn int(v: i128, line: u32) -> E {
    E::new(K::Int(v.to_string()), line)
}

fn as_int(e: &E) -> Option<i128> {
    match &e.k {
        K::Int(s) => s.replace('_', "").parse().ok(),
        _ => None,
    }
}

fn as_big(e: &E) -> Option<BigInt> {
    match &e.k {
        K::Int(s) => s.replace('_', "").parse().ok(),
        _ => None,
    }
}
fn big(v: BigInt, line: u32) -> E {
    E::new(K::Int(v.to_string()), line)
}

fn as_str(e: &E) -> Option<Vec<u8>> {
    match &e.k {
        K::Str(b) => Some(b.clone()),
        _ => None,
    }
}

/// Elements of a list-like value (list literal or integer range).
pub fn elements(e: &E) -> Option<Vec<E>> {
    match &e.k {
        K::List(items, None) => Some(items.clone()),
        K::Bin { op, l, r } if op == ".." => {
            let (last, step) = match &r.k {
                K::Bin { op, l: rl, r: rr } if op == "//" => (as_int(rl)?, as_int(rr)?),
                _ => {
                    let a = as_int(l)?;
                    let b = as_int(r)?;
                    (b, if b >= a { 1 } else { -1 })
                }
            };
            let first = as_int(l)?;
            let mut v = Vec::new();
            let mut x = first;
            if step > 0 {
                while x <= last {
                    v.push(int(x, e.line));
                    x += step;
                }
            } else if step < 0 {
                while x >= last {
                    v.push(int(x, e.line));
                    x += step;
                }
            }
            Some(v)
        }
        _ => None,
    }
}

fn kw_pair(k: &str, v: E, line: u32) -> E {
    E::new(
        K::Tuple(vec![E::new(K::Atom(k.to_string()), line), v]),
        line,
    )
}

fn apply_literal(
    function: &E,
    arguments: &[E],
    vars: &Vars,
    attrs: &HashMap<String, E>,
) -> Option<E> {
    match &function.k {
        K::Capture(body) => {
            let body = crate::collect::map_expr(body, &mut |node| match &node.k {
                K::CapArg(index) => arguments.get(*index as usize - 1).cloned(),
                _ => None,
            });
            eval(&body, vars, attrs)
        }
        K::Fn(clauses) => {
            for clause in clauses {
                if clause.args.len() != arguments.len() {
                    continue;
                }
                let mut local = vars.clone();
                if !clause
                    .args
                    .iter()
                    .zip(arguments)
                    .all(|(pattern, value)| bind(pattern, value, &mut local))
                {
                    continue;
                }
                if let Some(guard) = &clause.guard {
                    if !eval_bool(guard, &local, attrs)? {
                        continue;
                    }
                }
                return eval(&clause.body, &local, attrs);
            }
            None
        }
        _ => None,
    }
}

pub fn eval(e: &E, vars: &Vars, attrs: &HashMap<String, E>) -> Option<E> {
    let line = e.line;
    match &e.k {
        K::Int(_) | K::Float(_) | K::Atom(_) | K::Str(_) | K::Alias(_) => Some(e.clone()),
        K::Capture(body)
            if matches!(&body.k, K::Bin { op, l, r } if op == "/"
                && matches!(&l.k, K::Remote { recv, args, .. } if args.is_empty() && matches!(recv.k, K::Atom(_) | K::Alias(_)))
                && matches!(r.k, K::Int(_))) =>
        {
            Some(e.clone())
        }
        K::Var(n) => vars.get(n).cloned(),
        K::Attr(n) => attrs.get(n).and_then(|v| eval(v, vars, attrs)),
        K::Block(es) if es.len() == 1 => eval(&es[0], vars, attrs),
        K::Charlist(parts) | K::Sigil { ch: 'c', parts, .. } => {
            let mut bytes = Vec::new();
            for p in parts {
                match p {
                    IPart::Lit(b) => bytes.extend_from_slice(b),
                    _ => return None,
                }
            }
            let s = String::from_utf8(bytes).ok()?;
            Some(E::new(
                K::List(s.chars().map(|c| int(c as i128, line)).collect(), None),
                line,
            ))
        }
        K::Sigil {
            ch: 'w',
            parts,
            mods,
        } => {
            let mut bytes = Vec::new();
            for p in parts {
                match p {
                    IPart::Lit(b) => bytes.extend_from_slice(b),
                    _ => return None,
                }
            }
            let s = String::from_utf8(bytes).ok()?;
            Some(E::new(
                K::List(
                    s.split_whitespace()
                        .map(|w| {
                            if mods == "a" {
                                E::new(K::Atom(w.to_string()), line)
                            } else {
                                E::new(K::Str(w.as_bytes().to_vec()), line)
                            }
                        })
                        .collect(),
                    None,
                ),
                line,
            ))
        }
        K::Interp(parts) | K::AtomInterp(parts) => {
            let mut out = Vec::new();
            for p in parts {
                match p {
                    IPart::Lit(b) => out.extend_from_slice(b),
                    IPart::Expr(x) => {
                        let v = eval(x, vars, attrs)?;
                        out.extend_from_slice(&to_string_bytes(&v)?);
                    }
                }
            }
            if matches!(e.k, K::AtomInterp(_)) {
                Some(E::new(K::Atom(String::from_utf8(out).ok()?), line))
            } else {
                Some(E::new(K::Str(out), line))
            }
        }
        K::List(items, tail) => {
            let mut v = Vec::new();
            for i in items {
                v.push(eval(i, vars, attrs)?);
            }
            let t = match tail {
                Some(t) => Some(Box::new(eval(t, vars, attrs)?)),
                None => None,
            };
            Some(E::new(K::List(v, t), line))
        }
        K::Tuple(items) => {
            let mut v = Vec::new();
            for i in items {
                v.push(eval(i, vars, attrs)?);
            }
            Some(E::new(K::Tuple(v), line))
        }
        K::Map(pairs) => Some(E::new(
            K::Map(
                pairs
                    .iter()
                    .map(|(key, value)| Some((eval(key, vars, attrs)?, eval(value, vars, attrs)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            line,
        )),
        K::Bits(parts) => {
            let mut bytes = Vec::new();
            for part in parts {
                let (value, utf8) = match &part.k {
                    K::Bin { op, l, r }
                        if op == "::" && matches!(&r.k, K::Var(name) if name == "utf8") =>
                    {
                        (eval(l, vars, attrs)?, true)
                    }
                    _ => (eval(part, vars, attrs)?, false),
                };
                if utf8 {
                    let character = char::from_u32(u32::try_from(as_int(&value)?).ok()?)?;
                    let mut buf = [0; 4];
                    bytes.extend_from_slice(character.encode_utf8(&mut buf).as_bytes());
                } else if let Some(binary) = as_str(&value) {
                    bytes.extend(binary);
                } else {
                    bytes.push(u8::try_from(as_int(&value)?).ok()?);
                }
            }
            Some(E::new(K::Str(bytes), line))
        }
        K::Un { op, e: x } if op == "-" || op == "+" => {
            let v = eval(x, vars, attrs)?;
            match (&v.k, op.as_str()) {
                (K::Int(_), "-") => Some(big(-as_big(&v)?, line)),
                (K::Float(f), "-") => Some(E::new(K::Float(-f), line)),
                _ => Some(v),
            }
        }
        K::Bin { op, l, r } => {
            let a = eval(l, vars, attrs)?;
            let b = eval(r, vars, attrs)?;
            match op.as_str() {
                "+" => Some(big(as_big(&a)? + as_big(&b)?, line)),
                "-" => Some(big(as_big(&a)? - as_big(&b)?, line)),
                "*" => Some(big(as_big(&a)? * as_big(&b)?, line)),
                "<<<" | ">>>" => {
                    let shift = as_int(&b)?;
                    let magnitude = usize::try_from(shift.checked_abs()?).ok()?;
                    if magnitude > 1_000_000 {
                        return None;
                    }
                    let left = (op == "<<<") == (shift >= 0);
                    Some(big(
                        if left {
                            as_big(&a)? << magnitude
                        } else {
                            as_big(&a)? >> magnitude
                        },
                        line,
                    ))
                }
                "&&&" => Some(big(as_big(&a)? & as_big(&b)?, line)),
                "|||" => Some(big(as_big(&a)? | as_big(&b)?, line)),
                "^^^" => Some(big(as_big(&a)? ^ as_big(&b)?, line)),
                "<>" => {
                    let mut s = as_str(&a)?;
                    s.extend(as_str(&b)?);
                    Some(E::new(K::Str(s), line))
                }
                "++" => {
                    let mut x = elements(&a)?;
                    x.extend(elements(&b)?);
                    Some(E::new(K::List(x, None), line))
                }
                ".." | "//" => Some(E::new(
                    K::Bin {
                        op: op.clone(),
                        l: Box::new(a),
                        r: Box::new(b),
                    },
                    line,
                )),
                _ => None,
            }
        }
        K::Call { name, args, .. } => {
            let ev: Option<Vec<E>> = args.iter().map(|a| eval(a, vars, attrs)).collect();
            let ev = ev?;
            match (name.as_str(), ev.len()) {
                ("div", 2) => Some(int(as_int(&ev[0])?.checked_div(as_int(&ev[1])?)?, line)),
                ("rem", 2) => Some(int(as_int(&ev[0])?.checked_rem(as_int(&ev[1])?)?, line)),
                ("length", 1) => Some(int(elements(&ev[0])?.len() as i128, line)),
                _ => None,
            }
        }
        K::Remote {
            recv, name, args, ..
        } => {
            let m = match &recv.k {
                K::Alias(a) => a.clone(),
                K::Atom(a) => format!(":{}", a),
                _ => return None,
            };
            match (m.as_str(), name.as_str(), args.len()) {
                ("Module", "has_attribute?", 2) => {
                    if let K::Atom(attr) = &args[1].k {
                        return Some(E::atom(
                            if attrs.contains_key(attr) {
                                "true"
                            } else {
                                "false"
                            },
                            line,
                        ));
                    }
                }
                // a literal value is its own AST
                ("Macro", "escape", 1) => return eval(&args[0], vars, attrs),
                _ => {}
            }
            if m == "Enum" {
                match (name.as_str(), args.len()) {
                    ("map", 2) | ("map_join", 3) => {
                        let values = elements(&eval(&args[0], vars, attrs)?)?;
                        let function = args.last()?;
                        let mapped = values
                            .iter()
                            .map(|value| apply_literal(function, &[value.clone()], vars, attrs))
                            .collect::<Option<Vec<_>>>()?;
                        if name == "map" {
                            return Some(E::list(mapped, line));
                        }
                        let separator = as_str(&eval(&args[1], vars, attrs)?)?;
                        let mut bytes = Vec::new();
                        for (index, value) in mapped.iter().enumerate() {
                            if index > 0 {
                                bytes.extend_from_slice(&separator);
                            }
                            bytes.extend(to_string_bytes(value)?);
                        }
                        return Some(E::new(K::Str(bytes), line));
                    }
                    ("reduce", 2) | ("reduce", 3) => {
                        let mut values = elements(&eval(&args[0], vars, attrs)?)?.into_iter();
                        let mut accumulator = if args.len() == 3 {
                            eval(&args[1], vars, attrs)?
                        } else {
                            values.next()?
                        };
                        for value in values {
                            accumulator =
                                apply_literal(args.last()?, &[value, accumulator], vars, attrs)?;
                        }
                        return Some(accumulator);
                    }
                    _ => {}
                }
            }
            let ev: Option<Vec<E>> = args.iter().map(|a| eval(a, vars, attrs)).collect();
            let ev = ev?;
            match (m.as_str(), name.as_str(), ev.len()) {
                ("Enum", "with_index", 1) | ("Enum", "with_index", 2) => {
                    let start = if ev.len() == 2 { as_int(&ev[1])? } else { 0 };
                    let v = elements(&ev[0])?
                        .into_iter()
                        .enumerate()
                        .map(|(i, x)| E::new(K::Tuple(vec![x, int(start + i as i128, line)]), line))
                        .collect();
                    Some(E::new(K::List(v, None), line))
                }
                ("IO.ANSI", "color", 1)
                | ("IO.ANSI", "color_background", 1)
                | ("IO.ANSI", "color", 3)
                | ("IO.ANSI", "color_background", 3) => {
                    let code = if ev.len() == 1 {
                        let code = as_int(&ev[0])?;
                        if !(0..=255).contains(&code) {
                            return None;
                        }
                        code
                    } else {
                        let rgb = ev.iter().map(as_int).collect::<Option<Vec<_>>>()?;
                        if rgb.iter().any(|value| !(0..=5).contains(value)) {
                            return None;
                        }
                        16 + 36 * rgb[0] + 6 * rgb[1] + rgb[2]
                    };
                    let prefix = if name == "color" { 38 } else { 48 };
                    Some(E::new(
                        K::Str(format!("\x1b[{prefix};5;{code}m").into_bytes()),
                        line,
                    ))
                }
                ("Enum", "to_list", 1) => Some(E::new(K::List(elements(&ev[0])?, None), line)),
                ("Enum", "reverse", 1) => {
                    let mut v = elements(&ev[0])?;
                    v.reverse();
                    Some(E::new(K::List(v, None), line))
                }
                ("Enum", "zip", 2) => {
                    let a = elements(&ev[0])?;
                    let b = elements(&ev[1])?;
                    Some(E::new(
                        K::List(
                            a.into_iter()
                                .zip(b)
                                .map(|(x, y)| E::new(K::Tuple(vec![x, y]), line))
                                .collect(),
                            None,
                        ),
                        line,
                    ))
                }
                ("Enum", "concat", 2) | ("Kernel", "++", 2) => {
                    let mut a = elements(&ev[0])?;
                    a.extend(elements(&ev[1])?);
                    Some(E::new(K::List(a, None), line))
                }
                ("Atom", "to_string", 1) => match &ev[0].k {
                    K::Atom(a) => Some(E::new(K::Str(a.as_bytes().to_vec()), line)),
                    _ => None,
                },
                (":calendar", "date_to_gregorian_days", 1) => {
                    let parts = match &ev[0].k {
                        K::Tuple(parts) if parts.len() == 3 => parts,
                        _ => return None,
                    };
                    let (year, month, day) =
                        (as_int(&parts[0])?, as_int(&parts[1])?, as_int(&parts[2])?);
                    if year < 0 || !(1..=12).contains(&month) {
                        return None;
                    }
                    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
                    let months = [
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
                    if day < 1 || day > months[month as usize - 1] {
                        return None;
                    }
                    let years =
                        365 * year + (year + 3) / 4 - (year + 99) / 100 + (year + 399) / 400;
                    Some(int(
                        years + months[..month as usize - 1].iter().sum::<i128>() + day - 1,
                        line,
                    ))
                }
                ("String", "duplicate", 2) => {
                    let count = usize::try_from(as_int(&ev[1])?).ok()?;
                    if count > 1000000 {
                        return None;
                    }
                    Some(E::new(K::Str(as_str(&ev[0])?.repeat(count)), line))
                }
                (":erlang", "binary_to_integer", 1) | ("String", "to_integer", 1) => Some(big(
                    String::from_utf8(as_str(&ev[0])?).ok()?.parse().ok()?,
                    line,
                )),
                ("String", "to_atom", 1) => Some(E::new(
                    K::Atom(String::from_utf8(as_str(&ev[0])?).ok()?),
                    line,
                )),
                ("String", "upcase", 1) => Some(E::new(
                    K::Str(
                        String::from_utf8(as_str(&ev[0])?)
                            .ok()?
                            .to_uppercase()
                            .into_bytes(),
                    ),
                    line,
                )),
                ("String", "downcase", 1) => Some(E::new(
                    K::Str(
                        String::from_utf8(as_str(&ev[0])?)
                            .ok()?
                            .to_lowercase()
                            .into_bytes(),
                    ),
                    line,
                )),
                ("Integer", "to_string", 1) => Some(E::new(
                    K::Str(as_int(&ev[0])?.to_string().into_bytes()),
                    line,
                )),
                (":binary", "bin_to_list", 1) | (":erlang", "binary_to_list", 1) => {
                    let b = as_str(&ev[0])?;
                    Some(E::new(
                        K::List(b.into_iter().map(|c| int(c as i128, line)).collect(), None),
                        line,
                    ))
                }
                ("String", "to_charlist", 1) => {
                    let s = String::from_utf8(as_str(&ev[0])?).ok()?;
                    Some(E::new(
                        K::List(s.chars().map(|c| int(c as i128, line)).collect(), None),
                        line,
                    ))
                }
                ("List", "duplicate", 2) => {
                    let n = as_int(&ev[1])? as usize;
                    Some(E::new(K::List(vec![ev[0].clone(); n], None), line))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn to_string_bytes(v: &E) -> Option<Vec<u8>> {
    match &v.k {
        K::Str(b) => Some(b.clone()),
        K::Atom(a) => Some(a.as_bytes().to_vec()),
        K::Int(_) => Some(as_int(v)?.to_string().into_bytes()),
        _ => None,
    }
}

/// Bind a generator/assignment pattern against a literal value.
pub fn bind(pat: &E, val: &E, out: &mut Vars) -> bool {
    match (&pat.k, &val.k) {
        (K::Var(n), _) => {
            if n != "_" && !n.starts_with('_') {
                out.insert(n.clone(), val.clone());
            } else if n.len() > 1 && n.starts_with('_') {
                out.insert(n.clone(), val.clone());
            }
            true
        }
        (K::Tuple(ps), K::Tuple(vs)) if ps.len() == vs.len() => {
            ps.iter().zip(vs).all(|(p, v)| bind(p, v, out))
        }
        (K::List(ps, None), K::List(vs, None)) if ps.len() == vs.len() => {
            ps.iter().zip(vs).all(|(p, v)| bind(p, v, out))
        }
        (K::List(ps, Some(t)), K::List(vs, None)) if vs.len() >= ps.len() => {
            ps.iter().zip(vs).all(|(p, v)| bind(p, v, out))
                && bind(
                    t,
                    &E::new(K::List(vs[ps.len()..].to_vec(), None), val.line),
                    out,
                )
        }
        (K::Int(_), K::Int(_)) => as_int(pat) == as_int(val),
        (K::Atom(a), K::Atom(b)) => a == b,
        (K::Str(a), K::Str(b)) => a == b,
        _ => false,
    }
}

/// Replace `unquote(x)` / `unquote_splicing(x)` with literal values.
pub fn subst_unquote(e: &E, vars: &Vars, attrs: &HashMap<String, E>) -> E {
    let line = e.line;
    let splice = |items: &[E]| -> Vec<E> {
        let mut out = Vec::new();
        for i in items {
            if let K::Call { name, args, .. } = &i.k {
                if name == "unquote_splicing" && args.len() == 1 {
                    if let Some(v) = eval(&args[0], vars, attrs).and_then(|v| elements(&v)) {
                        out.extend(v);
                        continue;
                    }
                }
            }
            out.push(subst_unquote(i, vars, attrs));
        }
        out
    };
    match &e.k {
        K::Call { name, args, parens } if name == "__unquote_call__" && !args.is_empty() => {
            let head = subst_unquote(&args[0], vars, attrs);
            let rest = splice(&args[1..]);
            match head.k {
                K::Atom(a) => K::Call {
                    name: a,
                    args: rest,
                    parens: true,
                }
                .with_line(line),
                _ => {
                    let mut all = vec![head];
                    all.extend(rest);
                    K::Call {
                        name: name.clone(),
                        args: all,
                        parens: *parens,
                    }
                    .with_line(line)
                }
            }
        }
        // unquote fragments inside a quote belong to that quote
        K::Call { name, .. } if name == "quote" => e.clone(),
        K::Call { name, args, parens } => {
            if name == "unquote" && args.len() == 1 {
                if let Some(v) = eval(&args[0], vars, attrs) {
                    if matches!(&v.k, K::Bin { op, .. } if op == ".." || op == "//") {
                        return e.clone();
                    }
                    return v;
                }
            }
            K::Call {
                name: name.clone(),
                args: splice(args),
                parens: *parens,
            }
            .with_line(line)
        }
        K::Remote {
            recv,
            name,
            args,
            parens,
        } => K::Remote {
            recv: Box::new(subst_unquote(recv, vars, attrs)),
            name: name.clone(),
            args: splice(args),
            parens: *parens,
        }
        .with_line(line),
        K::AnonCall { f, args } => K::AnonCall {
            f: Box::new(subst_unquote(f, vars, attrs)),
            args: splice(args),
        }
        .with_line(line),
        K::Access { e: x, key } => K::Access {
            e: Box::new(subst_unquote(x, vars, attrs)),
            key: Box::new(subst_unquote(key, vars, attrs)),
        }
        .with_line(line),
        K::Bin { op, l, r } => K::Bin {
            op: op.clone(),
            l: Box::new(subst_unquote(l, vars, attrs)),
            r: Box::new(subst_unquote(r, vars, attrs)),
        }
        .with_line(line),
        K::Un { op, e: x } => K::Un {
            op: op.clone(),
            e: Box::new(subst_unquote(x, vars, attrs)),
        }
        .with_line(line),
        K::Tuple(items) => K::Tuple(splice(items)).with_line(line),
        K::List(items, tail) => K::List(
            splice(items),
            tail.as_ref()
                .map(|t| Box::new(subst_unquote(t, vars, attrs))),
        )
        .with_line(line),
        K::Map(ps) => K::Map(
            ps.iter()
                .map(|(a, b)| (subst_unquote(a, vars, attrs), subst_unquote(b, vars, attrs)))
                .collect(),
        )
        .with_line(line),
        K::MapUpd(b, ps) => K::MapUpd(
            Box::new(subst_unquote(b, vars, attrs)),
            ps.iter()
                .map(|(a, x)| (subst_unquote(a, vars, attrs), subst_unquote(x, vars, attrs)))
                .collect(),
        )
        .with_line(line),
        K::Struct { name, base, pairs } => K::Struct {
            name: name.clone(),
            base: base
                .as_ref()
                .map(|b| Box::new(subst_unquote(b, vars, attrs))),
            pairs: pairs
                .iter()
                .map(|(a, b)| (subst_unquote(a, vars, attrs), subst_unquote(b, vars, attrs)))
                .collect(),
        }
        .with_line(line),
        K::Bits(items) => K::Bits(
            items
                .iter()
                .map(|i| subst_unquote(i, vars, attrs))
                .collect(),
        )
        .with_line(line),
        K::Fn(cls) => {
            K::Fn(cls.iter().map(|c| subst_clause(c, vars, attrs)).collect()).with_line(line)
        }
        K::Clauses(cls) => {
            K::Clauses(cls.iter().map(|c| subst_clause(c, vars, attrs)).collect()).with_line(line)
        }
        K::Block(es) => {
            K::Block(es.iter().map(|x| subst_unquote(x, vars, attrs)).collect()).with_line(line)
        }
        K::Capture(x) => K::Capture(Box::new(subst_unquote(x, vars, attrs))).with_line(line),
        K::ParenArgs(items) => K::ParenArgs(splice(items)).with_line(line),
        K::Interp(parts) => K::Interp(
            parts
                .iter()
                .map(|p| match p {
                    IPart::Expr(x) => IPart::Expr(subst_unquote(x, vars, attrs)),
                    l => l.clone(),
                })
                .collect(),
        )
        .with_line(line),
        _ => e.clone(),
    }
}

fn subst_clause(c: &Clause, vars: &Vars, attrs: &HashMap<String, E>) -> Clause {
    Clause {
        args: c
            .args
            .iter()
            .map(|a| subst_unquote(a, vars, attrs))
            .collect(),
        guard: c.guard.as_ref().map(|g| subst_unquote(g, vars, attrs)),
        body: subst_unquote(&c.body, vars, attrs),
        line: c.line,
    }
}

trait WithLine {
    fn with_line(self, line: u32) -> E;
}

impl WithLine for K {
    fn with_line(self, line: u32) -> E {
        E::new(self, line)
    }
}

/// Expands definitions emitted by a module-level Enum.reduce, retaining its value.
pub fn unroll_reduce(
    e: &E,
    vars: &Vars,
    attrs: &HashMap<String, E>,
) -> Option<(E, Vec<(E, Vars)>)> {
    let args = match &e.k {
        K::Remote {
            recv, name, args, ..
        } if matches!(&recv.k, K::Alias(m) if m == "Enum")
            && name == "reduce"
            && args.len() == 3 =>
        {
            args
        }
        _ => return None,
    };
    let items = elements(&eval(&args[0], vars, attrs)?)?;
    let mut acc = eval(&args[1], vars, attrs)?;
    let clause = match &args[2].k {
        K::Fn(cs) if cs.len() == 1 && cs[0].args.len() == 2 && cs[0].guard.is_none() => &cs[0],
        _ => return None,
    };
    let statements = match &clause.body.k {
        K::Block(es) => es.clone(),
        _ => vec![clause.body.clone()],
    };
    let (result, body) = statements.split_last()?;
    let mut emitted = Vec::new();
    for item in items {
        let mut env = vars.clone();
        if !bind(&clause.args[0], &item, &mut env) || !bind(&clause.args[1], &acc, &mut env) {
            return None;
        }
        for statement in body {
            if let K::Bin { op, l, r } = &statement.k {
                if op == "=" {
                    let value = eval(r, &env, attrs)?;
                    if !bind(l, &value, &mut env) {
                        return None;
                    }
                    continue;
                }
            }
            if !matches!(&statement.k, K::Call { name, .. } if matches!(name.as_str(), "def" | "defp" | "defmacro" | "defmacrop"))
            {
                return None;
            }
            emitted.push((statement.clone(), env.clone()));
        }
        acc = eval(result, &env, attrs)?;
    }
    Some((acc, emitted))
}

/// Unroll a module-level `for` into the list of (statement, vars) to process.
pub fn unroll_for(args: &[E], vars: &Vars, attrs: &HashMap<String, E>) -> Option<Vec<(E, Vars)>> {
    let (last, quals) = args.split_last()?;
    let body = last.kw_get("do")?.clone();
    let mut envs: Vec<Vars> = vec![vars.clone()];
    for q in quals {
        let mut next = Vec::new();
        for env in &envs {
            match &q.k {
                K::Bin { op, l, r } if op == "<-" => {
                    let coll = eval(r, env, attrs)?;
                    for item in elements(&coll)? {
                        let mut e2 = env.clone();
                        if bind(l, &item, &mut e2) {
                            next.push(e2);
                        }
                    }
                }
                _ => {
                    // filter
                    let v = eval_bool(q, env, attrs)?;
                    if v {
                        next.push(env.clone());
                    }
                }
            }
        }
        envs = next;
    }
    let stmts: Vec<E> = match &body.k {
        K::Block(es) => es.clone(),
        _ => vec![body.clone()],
    };
    let mut out = Vec::new();
    for mut env in envs {
        for s in &stmts {
            if let K::Bin { op, l, r } = &s.k {
                if op == "=" {
                    let v = eval(r, &env, attrs)?;
                    if !bind(l, &v, &mut env) {
                        return None;
                    }
                    continue;
                }
            }
            out.push((s.clone(), env.clone()));
        }
    }
    Some(out)
}

pub fn eval_bool(e: &E, vars: &Vars, attrs: &HashMap<String, E>) -> Option<bool> {
    match &e.k {
        K::Bin { op, l, r }
            if matches!(
                op.as_str(),
                "==" | "!=" | "<" | ">" | "<=" | ">=" | "===" | "!=="
            ) =>
        {
            let a = eval(l, vars, attrs)?;
            let b = eval(r, vars, attrs)?;
            if let (Some(x), Some(y)) = (as_big(&a), as_big(&b)) {
                return Some(match op.as_str() {
                    "==" | "===" => x == y,
                    "!=" | "!==" => x != y,
                    "<" => x < y,
                    ">" => x > y,
                    "<=" => x <= y,
                    _ => x >= y,
                });
            }
            let same = format!("{:?}", a.k) == format!("{:?}", b.k);
            match op.as_str() {
                "==" | "===" => Some(same),
                "!=" | "!==" => Some(!same),
                _ => None,
            }
        }
        K::Bin { op, l, r } if op == "and" || op == "&&" => {
            Some(eval_bool(l, vars, attrs)? && eval_bool(r, vars, attrs)?)
        }
        K::Bin { op, l, r } if op == "or" || op == "||" => {
            Some(eval_bool(l, vars, attrs)? || eval_bool(r, vars, attrs)?)
        }
        K::Un { op, e: x } if op == "not" || op == "!" => Some(!eval_bool(x, vars, attrs)?),
        K::Bin { op, l, r } if op == "in" => {
            let a = eval(l, vars, attrs)?;
            let b = eval(r, vars, attrs)?;
            let items = elements(&b)?;
            Some(
                items
                    .iter()
                    .any(|i| format!("{:?}", i.k) == format!("{:?}", a.k)),
            )
        }
        _ => match eval(e, vars, attrs)?.k {
            K::Atom(a) if a == "true" => Some(true),
            K::Atom(a) if a == "false" || a == "nil" => Some(false),
            _ => None,
        },
    }
}
