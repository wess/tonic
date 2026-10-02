//! Definitions shared between the tonic compiler and the tonic runtime.
//! Both sides must agree on term encoding and on the indices of the
//! pre-registered atoms, so they live here.

/// Atoms with fixed indices. The compiler registers these first, in this order,
/// so the runtime can refer to them by constant.
pub const FIXED_ATOMS: &[&str] = &[
    "nil",                               // 0
    "true",                              // 1
    "false",                             // 2
    "ok",                                // 3
    "error",                             // 4
    "undefined",                         // 5
    "normal",                            // 6
    "infinity",                          // 7
    "EXIT",                              // 8
    "DOWN",                              // 9
    "process",                           // 10
    "kill",                              // 11
    "killed",                            // 12
    "noproc",                            // 13
    "throw",                             // 14
    "exit",                              // 15
    "__struct__",                        // 16
    "__exception__",                     // 17
    "message",                           // 18
    "Elixir.RuntimeError",               // 19
    "Elixir.ArgumentError",              // 20
    "Elixir.ArithmeticError",            // 21
    "Elixir.MatchError",                 // 22
    "Elixir.CaseClauseError",            // 23
    "Elixir.FunctionClauseError",        // 24
    "Elixir.KeyError",                   // 25
    "Elixir.BadMapError",                // 26
    "Elixir.BadFunctionError",           // 27
    "Elixir.BadArityError",              // 28
    "Elixir.UndefinedFunctionError",     // 29
    "Elixir.CondClauseError",            // 30
    "Elixir.WithClauseError",            // 31
    "Elixir.ErlangError",                // 32
    "Elixir.SystemLimitError",           // 33
    "Elixir.BadBooleanError",            // 34
    "Elixir.TryClauseError",             // 35
    "Elixir.Protocol.UndefinedError",    // 36
    "term",                              // 37
    "key",                               // 38
    "function",                          // 39
    "arity",                             // 40
    "module",                            // 41
    "args",                              // 42
    "original",                          // 43
    "value",                             // 44
    "reason",                            // 45
    "operator",                          // 46
    "protocol",                          // 47
    "description",                       // 48
    "shutdown",                          // 49
    "timeout",                           // 50
    "Elixir.Tonic.Internal",             // 51
    "format_exit",                       // 52
    "integer",                           // 53
    "float",                             // 54
    "atom",                              // 55
    "binary",                            // 56
    "list",                              // 57
    "tuple",                             // 58
    "map",                               // 59
    "pid",                               // 60
    "reference",                         // 61
    "badarg",                            // 62
    "badarith",                          // 63
    "Elixir.Integer",                    // 64
    "Elixir.Float",                      // 65
    "Elixir.Atom",                       // 66
    "Elixir.BitString",                  // 67
    "Elixir.List",                       // 68
    "Elixir.Tuple",                      // 69
    "Elixir.Map",                        // 70
    "Elixir.Function",                   // 71
    "Elixir.PID",                        // 72
    "Elixir.Reference",                  // 73
    "Elixir.Port",                       // 74
    "trap_exit",                         // 75
    "Elixir.File.Error",                 // 76
    "path",                              // 77
    "action",                            // 78
    "enoent",                            // 79
    "eacces",                            // 80
    "eisdir",                            // 81
    "enotdir",                           // 82
    "eexist",                            // 83
    "eof",                               // 84
    "badkey",                            // 85
    "badmap",                            // 86
    "clauses",                           // 87
    "kind",                              // 88
    "def",                               // 89
    "Elixir.UnicodeConversionError",     // 90
    "encoded",                           // 91
    "Elixir.Enum.OutOfBoundsError",      // 92
    "Elixir.Enum.EmptyError",            // 93
    "stop",                              // 94
    "tonic_main",                        // 95
    "monotonic",                         // 96
    "second",                            // 97
    "millisecond",                       // 98
    "microsecond",                       // 99
    "nanosecond",                        // 100
    "native",                            // 101
    "Elixir.Regex",                      // 102
    "source",                            // 103
    "opts",                              // 104
    "re_pattern",                        // 105
    "anonymous",                         // 106
    "capture",                           // 107
    "Elixir.ArgumentError.Internal",     // 108
    "big",                               // 109
    "little",                            // 110
    "signed",                            // 111
    "unsigned",                          // 112
    "utf8",                              // 113
    "unicode",                           // 114
    "latin1",                            // 115
    "incomplete",                        // 116
    "bad_return_value",                  // 117
    "global",                            // 118
    "abs",                               // 119
];

pub mod atom {
    pub const NIL: u64 = 0;
    pub const TRUE: u64 = 1;
    pub const FALSE: u64 = 2;
    pub const OK: u64 = 3;
    pub const ERROR: u64 = 4;
    pub const UNDEFINED: u64 = 5;
    pub const NORMAL: u64 = 6;
    pub const INFINITY: u64 = 7;
    pub const EXIT: u64 = 8;
    pub const DOWN: u64 = 9;
    pub const PROCESS: u64 = 10;
    pub const KILL: u64 = 11;
    pub const KILLED: u64 = 12;
    pub const NOPROC: u64 = 13;
    pub const THROW: u64 = 14;
    pub const EXIT_KIND: u64 = 15;
    pub const STRUCT: u64 = 16;
    pub const EXCEPTION: u64 = 17;
    pub const MESSAGE: u64 = 18;
    pub const RUNTIME_ERROR: u64 = 19;
    pub const ARGUMENT_ERROR: u64 = 20;
    pub const ARITHMETIC_ERROR: u64 = 21;
    pub const MATCH_ERROR: u64 = 22;
    pub const CASE_CLAUSE_ERROR: u64 = 23;
    pub const FUNCTION_CLAUSE_ERROR: u64 = 24;
    pub const KEY_ERROR: u64 = 25;
    pub const BAD_MAP_ERROR: u64 = 26;
    pub const BAD_FUNCTION_ERROR: u64 = 27;
    pub const BAD_ARITY_ERROR: u64 = 28;
    pub const UNDEFINED_FUNCTION_ERROR: u64 = 29;
    pub const COND_CLAUSE_ERROR: u64 = 30;
    pub const WITH_CLAUSE_ERROR: u64 = 31;
    pub const ERLANG_ERROR: u64 = 32;
    pub const SYSTEM_LIMIT_ERROR: u64 = 33;
    pub const BAD_BOOLEAN_ERROR: u64 = 34;
    pub const TRY_CLAUSE_ERROR: u64 = 35;
    pub const PROTOCOL_UNDEFINED_ERROR: u64 = 36;
    pub const TERM: u64 = 37;
    pub const KEY: u64 = 38;
    pub const FUNCTION: u64 = 39;
    pub const ARITY: u64 = 40;
    pub const MODULE: u64 = 41;
    pub const ARGS: u64 = 42;
    pub const ORIGINAL: u64 = 43;
    pub const VALUE: u64 = 44;
    pub const REASON: u64 = 45;
    pub const OPERATOR: u64 = 46;
    pub const PROTOCOL: u64 = 47;
    pub const DESCRIPTION: u64 = 48;
    pub const SHUTDOWN: u64 = 49;
    pub const TIMEOUT: u64 = 50;
    pub const TONIC_INTERNAL: u64 = 51;
    pub const FORMAT_EXIT: u64 = 52;
    pub const INTEGER: u64 = 53;
    pub const FLOAT: u64 = 54;
    pub const ATOM: u64 = 55;
    pub const BINARY: u64 = 56;
    pub const LIST: u64 = 57;
    pub const TUPLE: u64 = 58;
    pub const MAP: u64 = 59;
    pub const PID: u64 = 60;
    pub const REFERENCE: u64 = 61;
    pub const BADARG: u64 = 62;
    pub const BADARITH: u64 = 63;
    pub const E_INTEGER: u64 = 64;
    pub const E_FLOAT: u64 = 65;
    pub const E_ATOM: u64 = 66;
    pub const E_BITSTRING: u64 = 67;
    pub const E_LIST: u64 = 68;
    pub const E_TUPLE: u64 = 69;
    pub const E_MAP: u64 = 70;
    pub const E_FUNCTION: u64 = 71;
    pub const E_PID: u64 = 72;
    pub const E_REFERENCE: u64 = 73;
    pub const E_PORT: u64 = 74;
    pub const TRAP_EXIT: u64 = 75;
    pub const FILE_ERROR: u64 = 76;
    pub const PATH: u64 = 77;
    pub const ACTION: u64 = 78;
    pub const ENOENT: u64 = 79;
    pub const EACCES: u64 = 80;
    pub const EISDIR: u64 = 81;
    pub const ENOTDIR: u64 = 82;
    pub const EEXIST: u64 = 83;
    pub const EOF: u64 = 84;
    pub const BADKEY: u64 = 85;
    pub const BADMAP: u64 = 86;
    pub const CLAUSES: u64 = 87;
    pub const KIND: u64 = 88;
    pub const DEF: u64 = 89;
    pub const UNICODE_CONVERSION_ERROR: u64 = 90;
    pub const ENCODED: u64 = 91;
    pub const ENUM_OUT_OF_BOUNDS: u64 = 92;
    pub const ENUM_EMPTY: u64 = 93;
    pub const STOP: u64 = 94;
    pub const TONIC_MAIN: u64 = 95;
    pub const MONOTONIC: u64 = 96;
    pub const SECOND: u64 = 97;
    pub const MILLISECOND: u64 = 98;
    pub const MICROSECOND: u64 = 99;
    pub const NANOSECOND: u64 = 100;
    pub const NATIVE: u64 = 101;
    pub const REGEX: u64 = 102;
    pub const SOURCE: u64 = 103;
    pub const OPTS: u64 = 104;
    pub const RE_PATTERN: u64 = 105;
    pub const BIG: u64 = 109;
    pub const LITTLE: u64 = 110;
    pub const SIGNED: u64 = 111;
    pub const UNSIGNED: u64 = 112;
    pub const UTF8: u64 = 113;
    pub const UNICODE: u64 = 114;
    pub const LATIN1: u64 = 115;
    pub const INCOMPLETE: u64 = 116;
    pub const BAD_RETURN_VALUE: u64 = 117;
    pub const GLOBAL: u64 = 118;
}

/// Term encoding.
///
/// * `....1`  small integer (63-bit, value = word >> 1)
/// * `..010`  atom (index = word >> 3)
/// * `..100`  special immediates (`[]`, timeout marker)
/// * `..110`  pid (id = word >> 3)
/// * `..000`  pointer to a boxed heap object (non-zero)
/// * `0`      "no value": an exception is pending in the process context
pub mod enc {
    pub const NONE: u64 = 0;
    pub const NIL_LIST: u64 = 4;
    pub const TIMEOUT_MARK: u64 = 12;
    pub const SMALL_MAX: i64 = (1i64 << 62) - 1;
    pub const SMALL_MIN: i64 = -(1i64 << 62);

    #[inline(always)]
    pub const fn atom(i: u64) -> u64 {
        (i << 3) | 2
    }
    #[inline(always)]
    pub const fn small(i: i64) -> u64 {
        ((i << 1) | 1) as u64
    }
    #[inline(always)]
    pub const fn pid(i: u64) -> u64 {
        (i << 3) | 6
    }

    // Heap object tags (low 8 bits of the header word).
    pub const T_TUPLE: u64 = 1;
    pub const T_CONS: u64 = 2;
    pub const T_FLOAT: u64 = 3;
    pub const T_BIGINT: u64 = 4;
    pub const T_BINARY: u64 = 5;
    pub const T_MAP: u64 = 6;
    pub const T_MAPNODE: u64 = 7;
    pub const T_CLOSURE: u64 = 8;
    pub const T_REF: u64 = 9;
    pub const T_FORWARD: u64 = 10;
    pub const T_SUBBIN: u64 = 11;
    /// Bitstring whose size is not a whole number of bytes: [hdr, nbits, data...]
    pub const T_BITS: u64 = 12;
    /// Map HAMT node: [hdr, bitmap (small int), child...]; a child is a
    /// HAMT node or a T_MAPNODE leaf.
    pub const T_HAMT: u64 = 13;

    #[inline(always)]
    pub const fn header(tag: u64, payload_words: u64) -> u64 {
        (payload_words << 8) | tag
    }
}

/// 64-bit FNV-1a style mixing used for map priorities; must be identical in the
/// compiler (static maps) and the runtime.
pub fn mix64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
}
