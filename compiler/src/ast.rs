//! Surface AST produced by the parser.

#[derive(Clone, Debug)]
pub struct E {
    pub k: K,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum IPart {
    Lit(Vec<u8>),
    Expr(E),
}

#[derive(Clone, Debug)]
pub struct Clause {
    pub args: Vec<E>,
    pub guard: Option<E>,
    pub body: E,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum K {
    Int(String),
    Float(f64),
    Atom(String),
    Str(Vec<u8>),
    Interp(Vec<IPart>),
    Charlist(Vec<IPart>),
    AtomInterp(Vec<IPart>),
    Var(String),
    /// Alias segments joined with '.', without the `Elixir.` prefix
    Alias(String),
    Call {
        name: String,
        args: Vec<E>,
        parens: bool,
    },
    Remote {
        recv: Box<E>,
        name: String,
        args: Vec<E>,
        parens: bool,
    },
    AnonCall {
        f: Box<E>,
        args: Vec<E>,
    },
    Access {
        e: Box<E>,
        key: Box<E>,
    },
    Bin {
        op: String,
        l: Box<E>,
        r: Box<E>,
    },
    Un {
        op: String,
        e: Box<E>,
    },
    Tuple(Vec<E>),
    List(Vec<E>, Option<Box<E>>),
    Map(Vec<(E, E)>),
    MapUpd(Box<E>, Vec<(E, E)>),
    Struct {
        name: Box<E>,
        base: Option<Box<E>>,
        pairs: Vec<(E, E)>,
    },
    Bits(Vec<E>),
    Fn(Vec<Clause>),
    Block(Vec<E>),
    Clauses(Vec<Clause>),
    Capture(Box<E>),
    CapArg(u32),
    Attr(String),
    Sigil {
        ch: char,
        parts: Vec<IPart>,
        mods: String,
    },
    ParenArgs(Vec<E>),
    /// Foo.{Bar, Baz}
    MultiAlias(Box<E>, Vec<E>),
}

impl E {
    pub fn new(k: K, line: u32) -> E {
        E { k, line }
    }
    pub fn atom(s: &str, line: u32) -> E {
        E::new(K::Atom(s.to_string()), line)
    }
    pub fn nil(line: u32) -> E {
        E::atom("nil", line)
    }
    pub fn tuple(v: Vec<E>, line: u32) -> E {
        E::new(K::Tuple(v), line)
    }
    pub fn list(v: Vec<E>, line: u32) -> E {
        E::new(K::List(v, None), line)
    }
    pub fn call(name: &str, args: Vec<E>, line: u32) -> E {
        E::new(
            K::Call {
                name: name.to_string(),
                args,
                parens: true,
            },
            line,
        )
    }
    pub fn remote(m: &str, name: &str, args: Vec<E>, line: u32) -> E {
        E::new(
            K::Remote {
                recv: Box::new(if m.starts_with(':') {
                    E::atom(&m[1..], line)
                } else {
                    E::new(K::Alias(m.to_string()), line)
                }),
                name: name.to_string(),
                args,
                parens: true,
            },
            line,
        )
    }
    pub fn var(name: &str, line: u32) -> E {
        E::new(K::Var(name.to_string()), line)
    }
    pub fn bin(op: &str, l: E, r: E, line: u32) -> E {
        E::new(
            K::Bin {
                op: op.to_string(),
                l: Box::new(l),
                r: Box::new(r),
            },
            line,
        )
    }

    /// If this is a keyword list literal, return its (key, value) pairs.
    pub fn as_keyword(&self) -> Option<Vec<(String, &E)>> {
        if let K::List(items, None) = &self.k {
            let mut out = Vec::new();
            for it in items {
                if let K::Tuple(t) = &it.k {
                    if t.len() == 2 {
                        if let K::Atom(a) = &t[0].k {
                            out.push((a.clone(), &t[1]));
                            continue;
                        }
                    }
                }
                return None;
            }
            return Some(out);
        }
        None
    }

    pub fn kw_get<'a>(&'a self, key: &str) -> Option<&'a E> {
        self.as_keyword()
            .and_then(|kw| kw.into_iter().find(|(k, _)| k == key).map(|(_, v)| v))
    }
}
