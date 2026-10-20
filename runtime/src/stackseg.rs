//! Growable process stacks. Compiled functions compare the stack pointer
//! with `Ctx::slim` on entry; near the end of the current stack they call
//! here and run again on a fresh segment. Frames never move (exceptions
//! return by value and processes stop by returning), so this is the old
//! segmented-stack scheme without copying: deep body recursion such as
//! `:lists.map/2` over millions of elements is bounded by memory, as on the
//! BEAM, not by the size of the first stack.

use crate::bif_core::cx;
use crate::heap::Ctx;
use crate::term::Term;
use corosensei::stack::{DefaultStack, Stack};
use std::ffi::c_void;

/// Room left below the limit for runtime (Rust) code called by the deepest
/// compiled frame: a quarter of the stack, at most this.
const RED_ZONE: usize = 1 << 20;

/// Segments are at least this big, so a small first stack (TONIC_PROC_STACK_MB)
/// does not mean switching stacks every few frames.
const MIN_SEGMENT: usize = 4 << 20;

/// Segments kept after the recursion that needed them returns, so a depth
/// that keeps crossing a boundary does not map a stack each time.
const KEEP: usize = 2;

pub fn limit_of(stack: &DefaultStack) -> *mut u8 {
    let (low, high) = (stack.limit().get(), stack.base().get());
    (low + RED_ZONE.min((high - low) / 4)) as *mut u8
}

fn segment_bytes() -> usize {
    crate::sched::proc_stack_bytes().max(MIN_SEGMENT)
}

fn on_segment(c: *mut Ctx, run: impl FnOnce() -> Term) -> Term {
    let ctx = cx(c);
    let mut seg = match ctx.segs.pop() {
        Some(s) => s,
        None => DefaultStack::new(segment_bytes()).expect("failed to allocate a process stack segment"),
    };
    let saved = ctx.slim;
    ctx.slim = limit_of(&seg);
    let r = corosensei::on_stack(&mut seg, run);
    let ctx = cx(c);
    ctx.slim = saved;
    if ctx.segs.len() < KEEP {
        ctx.segs.push(seg);
    }
    r
}

#[no_mangle]
pub extern "C" fn tn_grow_fnptr(c: *mut Ctx, fp: *const c_void, n: u64, args: *const Term) -> Term {
    on_segment(c, || unsafe { crate::sched::tonic_call_fnptr(c, fp, n, args) })
}

#[no_mangle]
pub extern "C" fn tn_grow_closure(c: *mut Ctx, clo: Term, n: u64, args: *const Term) -> Term {
    on_segment(c, || unsafe { crate::sched::tonic_call_closure(c, clo, n, args) })
}
