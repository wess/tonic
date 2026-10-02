#![allow(improper_ctypes, improper_ctypes_definitions, clippy::missing_safety_doc)]
//! tonic runtime: values, garbage collection, processes (on tokio) and BIFs
//! for compiled Elixir programs.

pub mod atoms;
pub mod beamhash;
pub mod bif_bin;
pub mod bif_core;
pub mod bif_extra;
pub mod bif_file;
pub mod bif_misc;
pub mod bif_more;
pub mod bif_proc;
pub mod cmp;
pub mod ets;
pub mod exc;
pub mod heap;
pub mod io;
pub mod map;
pub mod num;
pub mod re;
pub mod ustr;
pub mod sched;
pub mod term;
