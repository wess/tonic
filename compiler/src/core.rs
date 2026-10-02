//! Core IR: the desugared, name-resolved language that codegen consumes.

use std::rc::Rc;

pub type VarId = u32;

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    /// Decimal string of an integer outside the small range.
    Big(String),
    Float(f64),
    Atom(String),
    Bin(Vec<u8>),
    Nil,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunKey {
    pub module: String,
    pub name: String,
    pub arity: usize,
}

impl FunKey {
    pub fn new(m: &str, n: &str, a: usize) -> FunKey {
        FunKey {
            module: m.to_string(),
            name: n.to_string(),
            arity: a,
        }
    }
    pub fn symbol(&self) -> String {
        format!("{}.{}/{}", self.module, self.name, self.arity)
    }
}

#[derive(Clone, Debug)]
pub enum BinType {
    Int,
    Float,
    Bin,
    /// `bitstring` / `bits`
    Bits,
    Utf8,
    Utf16,
    Utf32,
}

#[derive(Clone, Debug)]
pub struct BinSeg<T> {
    pub val: T,
    pub ty: BinType,
    /// size in bits (None = default / rest)
    pub size: Option<CE>,
    pub little: bool,
    pub signed: bool,
    /// size given in units? (`unit` multiplier applied already)
    pub unit: u32,
}

#[derive(Clone, Debug)]
pub enum Pat {
    Wild,
    Bind(VarId),
    /// Must equal the value of an already-bound variable (pin or repeated var).
    Eq(VarId),
    Lit(Lit),
    Tuple(Vec<Pat>),
    Cons(Box<Pat>, Box<Pat>),
    /// Map pattern: keys are literal or pinned expressions.
    Map(Vec<(CE, Pat)>),
    Bin(Vec<BinSeg<Pat>>),
    Alias(Box<Pat>, Box<Pat>),
    /// Value must be an atom, then match the inner pattern (`%name{}`).
    Atom(Box<Pat>),
}

#[derive(Clone, Debug)]
pub struct Clause {
    pub pats: Vec<Pat>,
    pub guard: Option<CE>,
    pub body: CE,
}

#[derive(Clone, Debug)]
pub enum Fail {
    CaseClause,
    FunctionClause { module: String, name: String, line: u32 },
    /// Return the unmatched value (used by `with` without else).
    Passthrough,
    WithClause,
    TryClause,
    /// Raise via the given runtime error (for `=` matches).
    Match,
}

#[derive(Clone, Debug)]
pub struct Lambda {
    pub id: usize,
    pub arity: usize,
    pub clauses: Vec<Clause>,
    /// Parameter variables
    pub params: Vec<VarId>,
    /// Filled by free variable analysis.
    pub free: Vec<VarId>,
    pub module: String,
    pub parent: String,
    /// Source line of the `fn` (0 when unknown)
    pub line: u32,
}

#[derive(Clone, Debug)]
pub struct TryE {
    pub body: CE,
    /// clauses over a single subject {kind, reason, stack}
    pub catches: Vec<Clause>,
    pub else_clauses: Vec<Clause>,
    pub after: Option<CE>,
}

#[derive(Clone, Debug)]
pub enum CE {
    Lit(Lit),
    Var(VarId),
    Tuple(Vec<CE>),
    Cons(Box<CE>, Box<CE>),
    Map(Vec<(CE, CE)>),
    /// strict=true: `%{m | k => v}` (keys must exist)
    MapUpdate(Box<CE>, Vec<(CE, CE)>, bool),
    Struct(String, Vec<(CE, CE)>),
    Bin(Vec<BinSeg<CE>>),
    /// String interpolation pieces (converted with String.Chars if not binary)
    Interp(Vec<CE>),
    Call(FunKey, Vec<CE>),
    Bif(String, Vec<CE>),
    DynCall(Box<CE>, String, Vec<CE>),
    Apply(Box<CE>, Vec<CE>),
    Fn(Rc<Lambda>),
    /// Capture of a named function: `&Mod.fun/arity` (resolved target)
    FunRef(FunKey, Target),
    Block(Vec<CE>),
    Match(Pat, Box<CE>),
    Case(Vec<CE>, Vec<Clause>, Fail),
    If(Box<CE>, Box<CE>, Box<CE>),
    Receive(Vec<Clause>, Option<(Box<CE>, Box<CE>)>),
    Try(Box<TryE>),
}

/// How a function reference resolves.
#[derive(Clone, Debug)]
pub enum Target {
    Fun(FunKey),
    Bif(String),
    Dynamic,
}

#[derive(Clone, Debug)]
pub struct FunDef {
    pub key: FunKey,
    pub params: Vec<VarId>,
    pub body: CE,
    pub public: bool,
}

impl CE {
    pub fn atom(s: &str) -> CE {
        CE::Lit(Lit::Atom(s.to_string()))
    }
    pub fn nil() -> CE {
        CE::atom("nil")
    }
    pub fn int(i: i64) -> CE {
        CE::Lit(Lit::Int(i))
    }
    pub fn bif(name: &str, args: Vec<CE>) -> CE {
        CE::Bif(name.to_string(), args)
    }
}
