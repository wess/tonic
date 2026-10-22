//! Constant guard expressions used by Erlang's conditional preprocessor.

use crate::erl_lex::{Tok, T};
use crate::erl_parse::{Form, Parser, X, XK};
use num_bigint::BigInt;
use std::cmp::Ordering;

#[derive(Clone)]
enum Value {
    Int(BigInt),
    Float(f64),
    Atom(String),
    Tuple(Vec<Value>),
    List(Vec<Value>),
}

fn boolean(value: bool) -> Value {
    Value::Atom(if value { "true" } else { "false" }.into())
}

fn truth(value: Value) -> Result<bool, ()> {
    match value {
        Value::Atom(a) if a == "true" => Ok(true),
        Value::Atom(a) if a == "false" => Ok(false),
        _ => Err(()),
    }
}

fn integer(value: Value) -> Result<BigInt, ()> {
    if let Value::Int(n) = value {
        Ok(n)
    } else {
        Err(())
    }
}

fn float(value: &Value) -> Result<f64, ()> {
    let n = match value {
        Value::Int(n) => n.to_string().parse().map_err(|_| ())?,
        Value::Float(n) => *n,
        _ => return Err(()),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(())
    }
}

fn int_float_cmp(n: &BigInt, f: f64) -> Ordering {
    let bits = f.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mut mantissa =
        BigInt::from((bits & ((1 << 52) - 1)) | if exponent == 0 { 0 } else { 1 << 52 });
    if bits >> 63 != 0 {
        mantissa = -mantissa;
    }
    let shift = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    };
    if shift < 0 {
        (n << -shift as usize).cmp(&mantissa)
    } else {
        n.cmp(&(mantissa << shift as usize))
    }
}

fn compare(a: &Value, b: &Value, exact: bool) -> Ordering {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap().then(if exact {
            a.is_sign_positive().cmp(&b.is_sign_positive())
        } else {
            Ordering::Equal
        }),
        (Value::Int(a), Value::Float(b)) => int_float_cmp(a, *b).then(if exact {
            Ordering::Less
        } else {
            Ordering::Equal
        }),
        (Value::Float(a), Value::Int(b)) => int_float_cmp(b, *a).reverse().then(if exact {
            Ordering::Greater
        } else {
            Ordering::Equal
        }),
        (Value::Atom(a), Value::Atom(b)) => a.cmp(b),
        (Value::Tuple(a), Value::Tuple(b)) => a
            .len()
            .cmp(&b.len())
            .then_with(|| sequence_cmp(a, b, exact)),
        (Value::List(a), Value::List(b)) => sequence_cmp(a, b, exact),
        _ => {
            let rank = |v: &Value| match v {
                Value::Int(_) | Value::Float(_) => 0,
                Value::Atom(_) => 1,
                Value::Tuple(_) => 2,
                Value::List(v) if v.is_empty() => 3,
                Value::List(_) => 4,
            };
            rank(a).cmp(&rank(b))
        }
    }
}

fn sequence_cmp(a: &[Value], b: &[Value], exact: bool) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(a, b)| compare(a, b, exact))
        .find(|o| *o != Ordering::Equal)
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}

fn function(x: &X) -> Option<&str> {
    match &x.k {
        XK::Atom(name) => Some(name),
        XK::Remote(module, name) if matches!(&module.k, XK::Atom(m) if m == "erlang") => {
            if let XK::Atom(name) = &name.k {
                Some(name)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn validate(x: &X) -> Result<(), String> {
    match &x.k {
        XK::Int(_) | XK::Float(_) | XK::Atom(_) | XK::Str(_) => Ok(()),
        XK::Tuple(items) | XK::List(items, None) => items.iter().try_for_each(validate),
        XK::UnOp(op, x) if matches!(op.as_str(), "+" | "-" | "not" | "bnot") => validate(x),
        XK::BinOp(op, a, b)
            if matches!(
                op.as_str(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "div"
                    | "rem"
                    | "band"
                    | "bor"
                    | "bxor"
                    | "bsl"
                    | "bsr"
                    | "and"
                    | "or"
                    | "xor"
                    | "andalso"
                    | "orelse"
                    | "=="
                    | "=:="
                    | "/="
                    | "=/="
                    | "<"
                    | "=<"
                    | ">"
                    | ">="
            ) =>
        {
            validate(a)?;
            validate(b)
        }
        XK::Call(callee, args) => {
            let name = function(callee).ok_or("unsupported call in -if expression")?;
            if name == "defined" && args.len() == 1 && matches!(args[0].k, XK::Var(_) | XK::Atom(_))
            {
                return Ok(());
            }
            let supported = match (name, args.len()) {
                (
                    "is_atom" | "is_boolean" | "is_float" | "is_integer" | "is_list" | "is_number"
                    | "is_tuple" | "abs" | "float" | "round" | "trunc" | "ceil" | "floor"
                    | "length" | "hd" | "tl" | "tuple_size",
                    1,
                ) => true,
                ("min" | "max" | "element", 2) => true,
                _ => false,
            };
            if !supported {
                return Err(format!(
                    "unsupported guard function {}/{} in -if expression",
                    name,
                    args.len()
                ));
            }
            args.iter().try_for_each(validate)
        }
        _ => Err("unsupported expression in -if condition".into()),
    }
}

fn eval(x: &X, defined: &impl Fn(&str) -> bool) -> Result<Value, ()> {
    Ok(match &x.k {
        XK::Int(n) => Value::Int(BigInt::from(*n)),
        XK::Float(n) if n.is_finite() => Value::Float(*n),
        XK::Atom(a) => Value::Atom(a.clone()),
        XK::Str(s) => Value::List(s.iter().map(|c| Value::Int(BigInt::from(*c))).collect()),
        XK::Tuple(items) => Value::Tuple(
            items
                .iter()
                .map(|x| eval(x, defined))
                .collect::<Result<_, _>>()?,
        ),
        XK::List(items, None) => Value::List(
            items
                .iter()
                .map(|x| eval(x, defined))
                .collect::<Result<_, _>>()?,
        ),
        XK::UnOp(op, x) => {
            let x = eval(x, defined)?;
            match (op.as_str(), x) {
                ("+", n @ (Value::Int(_) | Value::Float(_))) => n,
                ("-", Value::Int(n)) => Value::Int(-n),
                ("-", Value::Float(n)) => Value::Float(-n),
                ("bnot", Value::Int(n)) => Value::Int(!n),
                ("not", n) => boolean(!truth(n)?),
                _ => return Err(()),
            }
        }
        XK::BinOp(op, a, b) => {
            let a = eval(a, defined)?;
            if op == "andalso" && !truth(a.clone())? {
                return Ok(boolean(false));
            }
            if op == "orelse" && truth(a.clone())? {
                return Ok(boolean(true));
            }
            let b = eval(b, defined)?;
            match op.as_str() {
                "andalso" | "orelse" => b,
                "and" => boolean(truth(a)? & truth(b)?),
                "or" => boolean(truth(a)? | truth(b)?),
                "xor" => boolean(truth(a)? ^ truth(b)?),
                "==" | "=:=" => boolean(compare(&a, &b, op == "=:=") == Ordering::Equal),
                "/=" | "=/=" => boolean(compare(&a, &b, op == "=/=") != Ordering::Equal),
                "<" => boolean(compare(&a, &b, false) == Ordering::Less),
                "=<" => boolean(compare(&a, &b, false) != Ordering::Greater),
                ">" => boolean(compare(&a, &b, false) == Ordering::Greater),
                ">=" => boolean(compare(&a, &b, false) != Ordering::Less),
                "+" | "-" | "*" if matches!((&a, &b), (Value::Int(_), Value::Int(_))) => {
                    let (a, b) = (integer(a)?, integer(b)?);
                    Value::Int(match op.as_str() {
                        "+" => a + b,
                        "-" => a - b,
                        _ => a * b,
                    })
                }
                "+" | "-" | "*" | "/" => {
                    let (a, b) = (float(&a)?, float(&b)?);
                    let n = match op.as_str() {
                        "+" => a + b,
                        "-" => a - b,
                        "*" => a * b,
                        _ => a / b,
                    };
                    if !n.is_finite() {
                        return Err(());
                    }
                    Value::Float(n)
                }
                _ => {
                    let (a, b) = (integer(a)?, integer(b)?);
                    Value::Int(match op.as_str() {
                        "div" if b != BigInt::from(0) => a / b,
                        "rem" if b != BigInt::from(0) => a % b,
                        "band" => a & b,
                        "bor" => a | b,
                        "bxor" => a ^ b,
                        "bsl" | "bsr" => {
                            let shift = i64::try_from(&b).map_err(|_| ())?;
                            let shift = if op == "bsr" {
                                shift.checked_neg().ok_or(())?
                            } else {
                                shift
                            };
                            if shift >= 0 {
                                a << shift as usize
                            } else {
                                a >> -shift as usize
                            }
                        }
                        _ => return Err(()),
                    })
                }
            }
        }
        XK::Call(callee, args) => {
            let name = function(callee).ok_or(())?;
            if name == "defined" {
                let name = match &args[0].k {
                    XK::Var(n) | XK::Atom(n) => n,
                    _ => return Err(()),
                };
                return Ok(boolean(defined(name)));
            }
            let args = args
                .iter()
                .map(|x| eval(x, defined))
                .collect::<Result<Vec<_>, _>>()?;
            match (name, args.as_slice()) {
                ("is_atom", [v]) => boolean(matches!(v, Value::Atom(_))),
                ("is_boolean", [v]) => {
                    boolean(matches!(v, Value::Atom(a) if a == "true" || a == "false"))
                }
                ("is_integer", [v]) => boolean(matches!(v, Value::Int(_))),
                ("is_float", [v]) => boolean(matches!(v, Value::Float(_))),
                ("is_number", [v]) => boolean(matches!(v, Value::Int(_) | Value::Float(_))),
                ("is_tuple", [v]) => boolean(matches!(v, Value::Tuple(_))),
                ("is_list", [v]) => boolean(matches!(v, Value::List(_))),
                ("abs", [Value::Int(n)]) => {
                    Value::Int(if n < &BigInt::from(0) { -n } else { n.clone() })
                }
                ("abs", [Value::Float(n)]) => Value::Float(n.abs()),
                ("float", [v]) => Value::Float(float(v)?),
                ("trunc" | "round" | "ceil" | "floor", [Value::Int(n)]) => Value::Int(n.clone()),
                ("trunc" | "round" | "ceil" | "floor", [Value::Float(n)]) => {
                    let n = match name {
                        "trunc" => n.trunc(),
                        "round" => n.round(),
                        "ceil" => n.ceil(),
                        _ => n.floor(),
                    };
                    Value::Int(format!("{:.0}", n).parse().map_err(|_| ())?)
                }
                ("length", [Value::List(v)]) | ("tuple_size", [Value::Tuple(v)]) => {
                    Value::Int(BigInt::from(v.len()))
                }
                ("hd", [Value::List(v)]) => v.first().ok_or(())?.clone(),
                ("tl", [Value::List(v)]) if !v.is_empty() => Value::List(v[1..].to_vec()),
                ("element", [Value::Int(i), Value::Tuple(v)]) => v
                    .get(
                        usize::try_from(i)
                            .map_err(|_| ())?
                            .checked_sub(1)
                            .ok_or(())?,
                    )
                    .ok_or(())?
                    .clone(),
                ("min" | "max", [a, b]) => {
                    if (name == "min" && compare(a, b, false) == Ordering::Greater)
                        || (name == "max" && compare(a, b, false) == Ordering::Less)
                    {
                        b.clone()
                    } else {
                        a.clone()
                    }
                }
                _ => return Err(()),
            }
        }
        _ => return Err(()),
    })
}

pub fn condition(
    toks: Vec<Tok>,
    file: &str,
    defined: impl Fn(&str) -> bool,
) -> Result<bool, String> {
    let line = toks.first().map(|t| t.line).unwrap_or(1);
    let mut source = vec![
        Tok {
            t: T::Atom("condition".into()),
            line,
        },
        Tok {
            t: T::P("(".into()),
            line,
        },
        Tok {
            t: T::P(")".into()),
            line,
        },
        Tok {
            t: T::P("->".into()),
            line,
        },
    ];
    source.extend(toks);
    source.push(Tok { t: T::Dot, line });
    source.push(Tok { t: T::Eof, line });
    let forms = Parser::new(source, file).forms()?;
    let body = match forms.as_slice() {
        [Form::Function(_, _, clauses, _)] if clauses.len() == 1 && clauses[0].body.len() == 1 => {
            &clauses[0].body[0]
        }
        _ => return Err(format!("{}:{}: invalid -if expression", file, line)),
    };
    validate(body).map_err(|e| format!("{}:{}: {}", file, line, e))?;
    Ok(matches!(eval(body, &defined), Ok(Value::Atom(a)) if a == "true"))
}
