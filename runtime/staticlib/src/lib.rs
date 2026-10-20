//! The linkable runtime: every crate whose `tn_*` symbols compiled programs
//! call. Each keeps its own object files in the archive, so the linker only
//! loads what a program references.

pub use tonic_core;
pub use tonic_tls;
