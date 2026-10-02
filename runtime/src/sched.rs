//! Processes on tokio.
//!
//! Each Elixir process is a stackful coroutine (corosensei) owned by a tokio
//! task. The coroutine runs compiled code until it must wait (receive with an
//! empty mailbox, reduction budget exhausted, exit) and then suspends back to
//! the task, which `.await`s the right tokio primitive — a `Notify` for new
//! mail, `sleep_until` for `after` timeouts, `yield_now` for fairness. The
//! tokio multi-threaded scheduler therefore runs Elixir processes in parallel
//! across cores; since every process has its own heap and messages are copied,
//! no locks are needed on the hot path.

use crate::heap::{Ctx, Fragment, MIN_HEAP_WORDS, REDUCTIONS};
use crate::term::*;
use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

extern "C" {
    /// Emitted by the compiler: call closure `clo` with `n` arguments.
    pub fn tonic_call_closure(ctx: *mut Ctx, clo: Term, n: u64, args: *const Term) -> Term;
    /// Emitted by the compiler: call function pointer `fp` with `n` arguments.
    pub fn tonic_call_fnptr(ctx: *mut Ctx, fp: *const c_void, n: u64, args: *const Term) -> Term;
}

pub enum Signal {
    Msg(Fragment),
    Exit { from: u64, reason: Fragment, link: bool },
    /// process_info request from another process: reply with
    /// `{:"$tonic_info", ref, info}`.
    Info { from: u64, refid: u64, items: Fragment },
}

pub struct ProcState {
    pub links: HashSet<u64>,
    /// (ref id, watcher pid): processes monitoring this one.
    pub monitored_by: Vec<(u64, u64)>,
    pub trap_exit: bool,
    pub alive: bool,
    pub name: Option<u64>,
    /// process_flag(:priority) value (atom index).
    pub priority: u64,
    /// initial_call {m, f, arity} as atom indices / arity.
    pub initial_call: (u64, u64, u64),
    pub parent: Option<u64>,
}

pub struct ProcShared {
    pub pid: u64,
    pub inbox: Mutex<VecDeque<Signal>>,
    pub notify: tokio::sync::Notify,
    pub has_signal: AtomicBool,
    pub st: Mutex<ProcState>,
}

pub enum YieldReason {
    Reductions,
    Wait(Option<Instant>),
}

type Co = Coroutine<(), YieldReason, (), DefaultStack>;
struct SendCo(Co);

struct SendStack(DefaultStack);
unsafe impl Send for SendStack {}

struct StackPool(Vec<DefaultStack>);
unsafe impl Send for StackPool {}
impl std::ops::Deref for StackPool {
    type Target = Vec<DefaultStack>;
    fn deref(&self) -> &Vec<DefaultStack> {
        &self.0
    }
}
impl std::ops::DerefMut for StackPool {
    fn deref_mut(&mut self) -> &mut Vec<DefaultStack> {
        &mut self.0
    }
}

fn stack_pool() -> &'static parking_lot::Mutex<StackPool> {
    static P: std::sync::OnceLock<parking_lot::Mutex<StackPool>> = std::sync::OnceLock::new();
    P.get_or_init(|| parking_lot::Mutex::new(StackPool(Vec::new())))
}
unsafe impl Send for SendCo {}
impl SendCo {
    #[inline]
    fn resume(&mut self) -> CoroutineResult<YieldReason, ()> {
        self.0.resume(())
    }
}

struct Globals {
    procs: RwLock<HashMap<u64, Arc<ProcShared>>>,
    names: RwLock<HashMap<u64, u64>>,
    next_pid: AtomicU64,
    next_ref: AtomicU64,
    funs: RwLock<HashMap<(u64, u64, u64), usize>>,
    alive: AtomicUsize,
    rt: OnceLock<tokio::runtime::Handle>,
    timers: Mutex<HashMap<u64, (tokio::task::JoinHandle<()>, Instant)>>,
    proc_stack: usize,
    pub argv: Mutex<Vec<String>>,
}

fn g() -> &'static Globals {
    static G: OnceLock<Globals> = OnceLock::new();
    G.get_or_init(|| Globals {
        procs: RwLock::new(HashMap::new()),
        names: RwLock::new(HashMap::new()),
        next_pid: AtomicU64::new(100),
        next_ref: AtomicU64::new(1000),
        funs: RwLock::new(HashMap::new()),
        alive: AtomicUsize::new(0),
        rt: OnceLock::new(),
        timers: Mutex::new(HashMap::new()),
        proc_stack: std::env::var("TONIC_PROC_STACK_MB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(64)
            * 1024
            * 1024,
        argv: Mutex::new(Vec::new()),
    })
}

pub fn argv() -> Vec<String> {
    g().argv.lock().clone()
}

pub fn next_ref_id() -> u64 {
    g().next_ref.fetch_add(1, Ordering::Relaxed)
}

pub fn lookup_proc(pid: u64) -> Option<Arc<ProcShared>> {
    g().procs.read().get(&pid).cloned()
}

pub fn whereis(name: u64) -> Option<u64> {
    g().names.read().get(&name).copied()
}

pub fn register_name(name: u64, pid: u64) -> bool {
    let mut names = g().names.write();
    if names.contains_key(&name) {
        return false;
    }
    if let Some(p) = lookup_proc(pid) {
        let mut st = p.st.lock();
        if !st.alive || st.name.is_some() {
            return false;
        }
        st.name = Some(name);
    } else {
        return false;
    }
    names.insert(name, pid);
    true
}

pub fn unregister_name(name: u64) -> bool {
    let mut names = g().names.write();
    if let Some(pid) = names.remove(&name) {
        if let Some(p) = lookup_proc(pid) {
            p.st.lock().name = None;
        }
        true
    } else {
        false
    }
}

pub fn registered() -> Vec<u64> {
    g().names.read().keys().copied().collect()
}

pub fn register_funs(entries: &[(u64, u64, u64, usize)]) {
    let mut f = g().funs.write();
    for &(m, n, a, p) in entries {
        f.insert((m, n, a), p);
    }
}

pub fn lookup_fun(m: Term, f: Term, arity: u64) -> Option<usize> {
    g().funs.read().get(&(m, f, arity)).copied()
}

pub fn module_exists(m: Term) -> bool {
    g().funs.read().keys().any(|k| k.0 == m)
}

impl Ctx {
    pub fn shared(&self) -> &ProcShared {
        unsafe { &*self.shared }
    }
    pub fn pid(&self) -> u64 {
        self.shared().pid
    }

    fn suspend(&mut self, why: YieldReason) {
        let y = self.yielder as *const Yielder<(), YieldReason>;
        unsafe { (*y).suspend(why) };
    }

    /// Reduction budget exhausted: let others run. Returns false if the process
    /// was killed while suspended.
    pub fn yield_reds(&mut self) -> bool {
        self.reds = REDUCTIONS;
        self.suspend(YieldReason::Reductions);
        if self.shared().has_signal.load(Ordering::Acquire) {
            return self.drain();
        }
        true
    }

    /// Move pending signals into the save queue. Returns false if killed.
    pub fn drain(&mut self) -> bool {
        let sigs: VecDeque<Signal> = {
            let mut ib = self.shared().inbox.lock();
            self.shared().has_signal.store(false, Ordering::Release);
            std::mem::take(&mut *ib)
        };
        for s in sigs {
            match s {
                Signal::Msg(f) => {
                    let t = self.from_fragment(&f);
                    self.saveq.push_back(t);
                }
                Signal::Exit { from, reason, link } => {
                    let r = self.from_fragment(&reason);
                    let trap = self.shared().st.lock().trap_exit;
                    if r == atom(a::KILL) && !link {
                        self.kill_reason = atom(a::KILLED);
                        return false;
                    }
                    if trap {
                        let base = self.roots.len();
                        self.roots.push(r);
                        let t = self.tuple(&[atom(a::EXIT), pid(from), self.roots[base]]);
                        self.roots.truncate(base);
                        self.saveq.push_back(t);
                        continue;
                    }
                    if r == atom(a::NORMAL) {
                        if from == self.pid() && !link {
                            self.kill_reason = r;
                            return false;
                        }
                        continue;
                    }
                    self.kill_reason = r;
                    return false;
                }
                Signal::Info { from, refid, items } => {
                    let it = self.from_fragment(&items);
                    let info = crate::bif_proc::info_items(self, it, false);
                    let base = self.roots.len();
                    self.roots.push(info);
                    let r = mk_ref_term(self, refid);
                    self.roots.push(r);
                    let t = self.tuple(&[atom(crate::atoms::intern("$tonic_info")), self.roots[base + 1], self.roots[base]]);
                    self.roots.truncate(base);
                    let f = self.to_fragment(t);
                    send_fragment(from, f);
                }
            }
        }
        true
    }

    /// Next message for the current receive, or TIMEOUT_MARK, or NONE if killed.
    pub fn recv_next(&mut self, timeout: Term) -> Term {
        if !self.recv_started {
            self.recv_started = true;
            self.recv_cursor = 0;
            self.recv_deadline = if timeout == atom(a::INFINITY) {
                None
            } else if is_small(timeout) && small_val(timeout) >= 0 {
                Some(Instant::now() + Duration::from_millis(small_val(timeout) as u64))
            } else {
                self.recv_started = false;
                return self.raise_error(atom(crate::atoms::intern("timeout_value")));
            };
        }
        loop {
            if self.shared().has_signal.load(Ordering::Acquire) && !self.drain() {
                self.recv_started = false;
                return NONE;
            }
            if self.recv_cursor < self.saveq.len() {
                let m = self.saveq[self.recv_cursor];
                self.recv_cursor += 1;
                return m;
            }
            if let Some(d) = self.recv_deadline {
                if Instant::now() >= d {
                    self.recv_started = false;
                    return TIMEOUT_MARK;
                }
            }
            let d = self.recv_deadline;
            self.suspend(YieldReason::Wait(d));
        }
    }

    pub fn recv_accept(&mut self) {
        if self.recv_cursor > 0 {
            self.saveq.remove(self.recv_cursor - 1);
        }
        self.recv_started = false;
        self.recv_cursor = 0;
    }
}

pub fn send_signal(to: u64, sig: Signal) -> bool {
    if let Some(p) = lookup_proc(to) {
        {
            let mut ib = p.inbox.lock();
            ib.push_back(sig);
            p.has_signal.store(true, Ordering::Release);
        }
        p.notify.notify_one();
        true
    } else {
        false
    }
}

pub fn send_fragment(to: u64, f: Fragment) {
    send_signal(to, Signal::Msg(f));
}

fn new_shared(pid: u64) -> Arc<ProcShared> {
    Arc::new(ProcShared {
        pid,
        inbox: Mutex::new(VecDeque::new()),
        notify: tokio::sync::Notify::new(),
        has_signal: AtomicBool::new(false),
        st: Mutex::new(ProcState {
            links: HashSet::new(),
            monitored_by: Vec::new(),
            trap_exit: false,
            alive: true,
            name: None,
            priority: crate::atoms::intern("normal"),
            initial_call: (crate::atoms::intern("erlang"), crate::atoms::intern("apply"), 2),
            parent: None,
        }),
    })
}

pub enum Entry {
    Closure(Fragment),
    /// {module, function, args}
    Mfa(Fragment),
    Main(extern "C" fn(*mut Ctx) -> Term),
}

pub struct SpawnOpts {
    pub link_to: Option<u64>,
    pub parent: Option<u64>,
    pub monitor_by: Option<(u64, u64)>,
    /// Inherited group leader (pid id), if the parent has one.
    pub group_leader: Option<u64>,
}

/// Group leaders set by programs (pid id -> group leader pid id). Processes
/// without an entry use the standard I/O directly.
pub fn group_leaders() -> &'static parking_lot::Mutex<HashMap<u64, u64>> {
    static G: std::sync::OnceLock<parking_lot::Mutex<HashMap<u64, u64>>> = std::sync::OnceLock::new();
    G.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Spawn a new process; returns its pid id.
pub fn spawn(entry: Entry, opts: SpawnOpts) -> u64 {
    let pid = g().next_pid.fetch_add(1, Ordering::Relaxed);
    if let Some(gl) = opts.group_leader {
        group_leaders().lock().insert(pid, gl);
    }
    let shared = new_shared(pid);
    shared.st.lock().parent = opts.parent;
    if let Some(parent) = opts.link_to {
        shared.st.lock().links.insert(parent);
        if let Some(pp) = lookup_proc(parent) {
            pp.st.lock().links.insert(pid);
        }
    }
    if let Some((refid, watcher)) = opts.monitor_by {
        shared.st.lock().monitored_by.push((refid, watcher));
        monitor_index().lock().insert(refid, (pid, watcher));
    }
    g().procs.write().insert(pid, shared.clone());
    g().alive.fetch_add(1, Ordering::Relaxed);
    let is_main = matches!(entry, Entry::Main(_));
    let stack_size = if is_main {
        std::env::var("TONIC_MAIN_STACK_MB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1024)
            * 1024
            * 1024
    } else {
        g().proc_stack
    };
    let sh2 = shared.clone();
    let stack = if is_main { None } else { stack_pool().lock().pop() };
    let stack = match stack {
        Some(s) => s,
        None => {
            let st = DefaultStack::new(stack_size).expect("failed to allocate process stack");
            if is_main {
                advise_huge(&st);
            }
            st
        }
    };
    let co: Co = Coroutine::with_stack(
        stack,
        move |yielder: &Yielder<(), YieldReason>, ()| {
            let mut ctx = Ctx::new(match std::env::var("TONIC_MIN_HEAP").ok().and_then(|s| s.parse::<usize>().ok()) {
                Some(n) => n.max(64),
                None => if is_main { 1 << 16 } else { MIN_HEAP_WORDS },
            });
            ctx.shared = Arc::as_ptr(&sh2);
            ctx.yielder = yielder as *const _ as *const c_void;
            let cp: *mut Ctx = &mut *ctx;
            let r = match &entry {
                Entry::Main(f) => f(cp),
                Entry::Closure(frag) => {
                    let f = ctx.from_fragment(frag);
                    ctx.spawn_fun = f;
                    unsafe { tonic_call_closure(cp, f, 0, std::ptr::null()) }
                }
                Entry::Mfa(frag) => {
                    let t = ctx.from_fragment(frag);
                    ctx.spawn_fun = t;
                    crate::bif_core::tn_apply_mfa(cp, tuple_get(t, 0), tuple_get(t, 1), tuple_get(t, 2))
                }
            };
            finish_process(&mut ctx, r, is_main);
        },
    );
    let mut co = SendCo(co);
    let task = async move {
        let shared = shared;
        loop {
            match co.resume() {
                CoroutineResult::Yield(YieldReason::Reductions) => tokio::task::yield_now().await,
                CoroutineResult::Yield(YieldReason::Wait(deadline)) => match deadline {
                    None => shared.notify.notified().await,
                    Some(d) => {
                        let d = tokio::time::Instant::from_std(d);
                        tokio::select! {
                            _ = shared.notify.notified() => {}
                            _ = tokio::time::sleep_until(d) => {}
                        }
                    }
                },
                CoroutineResult::Return(()) => break,
            }
        }
        // Recycle the stack for the next process.
        if !is_main {
            let stack = co.0.into_stack();
            let mut pool = stack_pool().lock();
            if pool.len() < 256 {
                pool.push(SendStack(stack).0);
            }
        }
    };
    g().rt.get().expect("runtime").spawn(task);
    pid
}

/// Compute the exit reason from how the process ended, notify links and
/// monitors, and deregister.
fn finish_process(ctx: &mut Ctx, r: Term, is_main: bool) {
    let reason = if r != NONE {
        atom(a::NORMAL)
    } else if ctx.kill_reason != 0 {
        ctx.kill_reason
    } else {
        ctx.flush_pending_args();
        let kind = ctx.exc_kind;
        let rs = ctx.exc_reason;
        if kind == atom(a::EXIT_KIND) {
            rs
        } else {
            // Report the crash like the BEAM logger does.
            if !is_main {
                let msg = crate::bif_misc::format_exit_message(ctx, kind, rs);
                let text = format!(
                    "\n{} [error] Process #PID<0.{}.0> raised an exception\n{}",
                    crate::bif_more::log_time(),
                    ctx.pid(),
                    msg
                );
                let text = if crate::bif_more::stdout_is_tty() { format!("\x1b[31m{}\x1b[0m\n", text) } else { format!("{}\n", text) };
                crate::io::stdout_write(text.as_bytes());
            }
            if kind == atom(a::THROW) {
                let base = ctx.roots.len();
                ctx.roots.push(rs);
                let nocatch = crate::atoms::intern("nocatch");
                let inner = ctx.tuple(&[atom(nocatch), ctx.roots[base]]);
                ctx.roots[base] = inner;
                let t = ctx.tuple(&[ctx.roots[base], NIL_LIST]);
                ctx.roots.truncate(base);
                t
            } else {
                // {reason, stacktrace}, as on the BEAM
                let base = ctx.roots.len();
                ctx.roots.push(rs);
                let st = if ctx.exc_stack == 0 { NIL_LIST } else { ctx.exc_stack };
                ctx.roots.push(st);
                let t = ctx.tuple(&[ctx.roots[base], ctx.roots[base + 1]]);
                ctx.roots.truncate(base);
                t
            }
        }
    };
    if is_main {
        if r == NONE {
            let kind = ctx.exc_kind;
            let rs = ctx.exc_reason;
            if ctx.kill_reason == 0 {
                let msg = crate::bif_misc::format_exit_message_x(ctx, kind, rs, true);
                crate::io::stdout_flush();
                eprintln!("{}", msg);
            }
            crate::io::stdout_flush();
            std::process::exit(1);
        }
        crate::io::stdout_flush();
        std::process::exit(0);
    }
    ctx.exit_reason = reason;
    process_exit(ctx);
}

fn process_exit(ctx: &mut Ctx) {
    let me = ctx.pid();
    let shared = unsafe { &*ctx.shared };
    let (links, monitors, name) = {
        let mut st = shared.st.lock();
        st.alive = false;
        (
            std::mem::take(&mut st.links),
            std::mem::take(&mut st.monitored_by),
            st.name.take(),
        )
    };
    g().procs.write().remove(&me);
    crate::ets::owner_exited(me);
    group_leaders().lock().remove(&me);
    if let Some(n) = name {
        g().names.write().remove(&n);
    }
    g().alive.fetch_sub(1, Ordering::Relaxed);
    let reason = ctx.exit_reason;
    let rf = ctx.to_fragment(reason);
    for l in links {
        if let Some(p) = lookup_proc(l) {
            p.st.lock().links.remove(&me);
        }
        send_signal(
            l,
            Signal::Exit {
                from: me,
                reason: clone_fragment(&rf),
                link: true,
            },
        );
    }
    for (refid, watcher) in monitors {
        // Claim the monitor and deliver DOWN while holding the index lock,
        // so a concurrent demonitor either wins or finds the message queued.
        let mut idx = monitor_index().lock();
        if idx.remove(&refid).is_none() {
            continue;
        }
        monitor_gone(refid);
        // {:DOWN, ref, :process, pid, reason}
        let r = mk_ref_term(ctx, refid);
        let base = ctx.roots.len();
        ctx.roots.push(r);
        let head = match monitor_tag_take(refid) {
            Some(f) => ctx.from_fragment(&f),
            None => atom(a::DOWN),
        };
        ctx.roots.push(head);
        let t = ctx.tuple(&[
            ctx.roots[base + 1],
            ctx.roots[base],
            atom(a::PROCESS),
            pid(me),
            ctx.exit_reason,
        ]);
        ctx.roots.truncate(base);
        let f = ctx.to_fragment(t);
        send_fragment(watcher, f);
        drop(idx);
    }
}

pub fn clone_fragment(f: &Fragment) -> Fragment {
    let lo = f.buf.as_ptr() as usize;
    let hi = lo + f.buf.len() * 8;
    let words = crate::heap::copy_size(f.root, lo, hi).max(1);
    let mut buf: Vec<u64> = Vec::with_capacity(words);
    unsafe { buf.set_len(words) };
    let mut free = buf.as_mut_ptr();
    let root = unsafe { crate::heap::copy_term(f.root, lo, hi, &mut free) };
    Fragment { buf, root }
}

pub fn mk_ref_term(ctx: &mut Ctx, id: u64) -> Term {
    let p = ctx.alloc(2);
    unsafe {
        *p = header(T_REF, 1);
        *p.add(1) = id;
    }
    p as Term
}

// ---------------------------------------------------------------------------
// Links / monitors / exits
// ---------------------------------------------------------------------------

pub fn link(me: u64, other: u64) -> bool {
    match lookup_proc(other) {
        Some(p) => {
            {
                let mut st = p.st.lock();
                if !st.alive {
                    return false;
                }
                st.links.insert(me);
            }
            if let Some(mp) = lookup_proc(me) {
                mp.st.lock().links.insert(other);
            }
            true
        }
        None => false,
    }
}

pub fn unlink(me: u64, other: u64) {
    if let Some(p) = lookup_proc(other) {
        p.st.lock().links.remove(&me);
    }
    if let Some(p) = lookup_proc(me) {
        p.st.lock().links.remove(&other);
    }
}

/// monitor ref id -> (target, watcher)
fn monitor_index() -> &'static parking_lot::Mutex<HashMap<u64, (u64, u64)>> {
    static M: std::sync::OnceLock<parking_lot::Mutex<HashMap<u64, (u64, u64)>>> = std::sync::OnceLock::new();
    M.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Process aliases: ref id -> (owner pid, deactivate on demonitor).
fn aliases() -> &'static parking_lot::Mutex<HashMap<u64, (u64, bool)>> {
    static A: std::sync::OnceLock<parking_lot::Mutex<HashMap<u64, (u64, bool)>>> = std::sync::OnceLock::new();
    A.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

pub fn alias_add(refid: u64, owner: u64, on_demonitor: bool) {
    aliases().lock().insert(refid, (owner, on_demonitor));
}

pub fn alias_remove(refid: u64, owner: u64) -> bool {
    let mut a = aliases().lock();
    match a.get(&refid) {
        Some((o, _)) if *o == owner => {
            a.remove(&refid);
            true
        }
        _ => false,
    }
}

pub fn alias_lookup(refid: u64) -> Option<u64> {
    let mut a = aliases().lock();
    let r = a.get(&refid).map(|(o, _)| *o);
    if r.is_some() && reply_aliases().lock().remove(&refid) {
        a.remove(&refid);
    }
    r
}

/// Aliases created with the :reply option: deactivated by the first message.
fn reply_aliases() -> &'static parking_lot::Mutex<std::collections::HashSet<u64>> {
    static R: std::sync::OnceLock<parking_lot::Mutex<std::collections::HashSet<u64>>> = std::sync::OnceLock::new();
    R.get_or_init(|| parking_lot::Mutex::new(std::collections::HashSet::new()))
}

pub fn alias_mark_reply(refid: u64) {
    reply_aliases().lock().insert(refid);
}

/// Custom DOWN tags (monitor option {tag, Tag}): ref id -> tag.
fn monitor_tags() -> &'static parking_lot::Mutex<HashMap<u64, Fragment>> {
    static M: std::sync::OnceLock<parking_lot::Mutex<HashMap<u64, Fragment>>> = std::sync::OnceLock::new();
    M.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

pub fn monitor_tag_set(refid: u64, f: Fragment) {
    monitor_tags().lock().insert(refid, f);
}

pub fn monitor_tag_take(refid: u64) -> Option<Fragment> {
    let m = monitor_tags();
    let mut g = m.lock();
    if g.is_empty() {
        return None;
    }
    g.remove(&refid)
}

/// Returns false if the target is not alive.
pub fn monitor(watcher: u64, target: u64, refid: u64) -> bool {
    match lookup_proc(target) {
        Some(p) => {
            let mut st = p.st.lock();
            if !st.alive {
                return false;
            }
            st.monitored_by.push((refid, watcher));
            monitor_index().lock().insert(refid, (target, watcher));
            true
        }
        None => false,
    }
}

fn monitor_gone(refid: u64) {
    let mut a = aliases().lock();
    if let Some((_, true)) = a.get(&refid) {
        a.remove(&refid);
    }
}

/// Remove a monitor; true if it was still active.
pub fn demonitor(refid: u64) -> bool {
    let target = monitor_index().lock().remove(&refid);
    let _ = monitor_tag_take(refid);
    let found = target.is_some();
    if let Some((t, _)) = target {
        if let Some(p) = lookup_proc(t) {
            p.st.lock().monitored_by.retain(|(r, _)| *r != refid);
        }
    }
    monitor_gone(refid);
    found
}

pub fn is_alive(pid: u64) -> bool {
    lookup_proc(pid).map(|p| p.st.lock().alive).unwrap_or(false)
}

pub fn set_trap_exit(pid: u64, v: bool) -> bool {
    if let Some(p) = lookup_proc(pid) {
        let mut st = p.st.lock();
        let old = st.trap_exit;
        st.trap_exit = v;
        old
    } else {
        false
    }
}

pub fn all_pids() -> Vec<u64> {
    let mut v: Vec<u64> = g().procs.read().keys().copied().collect();
    v.sort();
    v
}

pub fn links_of(pid: u64) -> Vec<u64> {
    lookup_proc(pid)
        .map(|p| p.st.lock().links.iter().copied().collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------

pub fn send_after(to: u64, f: Fragment, ms: u64) -> u64 {
    send_after_id(to, f, ms, next_ref_id())
}

pub fn send_after_id(to: u64, f: Fragment, ms: u64, id: u64) -> u64 {
    let deadline = Instant::now() + Duration::from_millis(ms);
    let h = g().rt.get().unwrap().spawn(async move {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        g().timers.lock().remove(&id);
        send_fragment(to, f);
    });
    g().timers.lock().insert(id, (h, deadline));
    id
}

/// Returns remaining ms if the timer was active.
pub fn cancel_timer(id: u64) -> Option<u64> {
    if let Some((h, d)) = g().timers.lock().remove(&id) {
        h.abort();
        let now = Instant::now();
        Some(if d > now { (d - now).as_millis() as u64 } else { 0 })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct FunEntry {
    pub module: u64,
    pub name: u64,
    pub arity: u64,
    pub ptr: *const c_void,
}

#[no_mangle]
pub unsafe extern "C" fn tonic_start(
    argc: i32,
    argv: *const *const std::ffi::c_char,
    atoms: *const *const std::ffi::c_char,
    natoms: u64,
    atom_beam: *const u32,
    funs: *const FunEntry,
    nfuns: u64,
    main_entry: extern "C" fn(*mut Ctx) -> Term,
    pfuns: *const FunEntry,
    npfuns: u64,
) -> i32 {
    // Atoms: fixed ones first (the compiler emits them in the same order).
    for i in 0..natoms as usize {
        let s = std::ffi::CStr::from_ptr(*atoms.add(i)).to_string_lossy();
        let idx = crate::atoms::intern_with(&s, Some(*atom_beam.add(i)));
        debug_assert_eq!(idx as usize, i);
    }
    let mut fe = Vec::with_capacity((nfuns + npfuns) as usize);
    for i in 0..nfuns as usize {
        let e = &*funs.add(i);
        fe.push((e.module, e.name, e.arity, e.ptr as usize));
    }
    // The precompiled prelude's functions.
    if !pfuns.is_null() {
        for i in 0..npfuns as usize {
            let e = &*pfuns.add(i);
            fe.push((e.module, e.name, e.arity, e.ptr as usize));
        }
    }
    register_funs(&fe);
    {
        let mut av = g().argv.lock();
        for i in 1..argc as usize {
            av.push(
                std::ffi::CStr::from_ptr(*argv.add(i))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    crate::io::init();
    let threads = std::env::var("TONIC_SCHEDULERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads.max(1))
        .enable_time()
        .thread_stack_size(8 * 1024 * 1024)
        .build()
        .expect("tokio runtime");
    let _ = g().rt.set(rt.handle().clone());
    rt.block_on(async {
        spawn(
            Entry::Main(main_entry),
            SpawnOpts {
                link_to: None,
                parent: None,
                monitor_by: None,
                group_leader: None,
            },
        );
        // The main process calls exit() when done.
        std::future::pending::<()>().await;
    });
    0
}

/// Back the (large, deep-recursion) main stack with transparent huge pages so
/// first-touch page faults don't dominate deep body recursion.
fn advise_huge(st: &DefaultStack) {
    #[cfg(target_os = "linux")]
    {
        use corosensei::stack::Stack;
        extern "C" {
            fn madvise(addr: *mut std::ffi::c_void, len: usize, advice: i32) -> i32;
        }
        let lo = st.limit().get();
        let hi = st.base().get();
        let lo2 = (lo + 0x1f_ffff) & !0x1f_ffff;
        if hi > lo2 {
            unsafe {
                madvise(lo2 as *mut _, hi - lo2, 14 /* MADV_HUGEPAGE */);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = st;
}

/// Remaining ms of an active timer.
pub fn read_timer(id: u64) -> Option<u64> {
    g().timers.lock().get(&id).map(|(_, d)| {
        let now = Instant::now();
        if *d > now { (*d - now).as_millis() as u64 } else { 0 }
    })
}

pub fn priority_of(pid: u64) -> Option<u64> {
    lookup_proc(pid).map(|p| p.st.lock().priority)
}

pub fn set_priority(pid: u64, v: u64) -> u64 {
    match lookup_proc(pid) {
        Some(p) => std::mem::replace(&mut p.st.lock().priority, v),
        None => v,
    }
}

/// Pids monitored by `watcher`.
pub fn monitors_of(watcher: u64) -> Vec<u64> {
    monitor_index().lock().values().filter(|(_, w)| *w == watcher).map(|(t, _)| *t).collect()
}

pub fn proc_state<R>(pid: u64, f: impl FnOnce(&ProcState) -> R) -> Option<R> {
    lookup_proc(pid).map(|p| f(&p.st.lock()))
}

pub fn set_initial_call(pid: u64, m: u64, f: u64, a: u64) {
    if let Some(p) = lookup_proc(pid) {
        p.st.lock().initial_call = (m, f, a);
    }
}

pub fn queue_len(pid: u64) -> usize {
    lookup_proc(pid).map(|p| p.inbox.lock().iter().filter(|s| matches!(s, Signal::Msg(_))).count()).unwrap_or(0)
}
