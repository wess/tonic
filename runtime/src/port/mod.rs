//! Ports to external programs (`open_port/2` with `spawn` and
//! `spawn_executable`).
//!
//! A port is a boxed `T_PORT` term whose number indexes the registry here.
//! The program's output is read by a tokio task that delivers
//! `{port, {:data, ...}}` messages to the connected process, as sockets do in
//! `net.rs`; commands queue to a writer task, so no scheduler thread ever
//! blocks on the program. Ports take part in links and monitors: a process's
//! link set holds `LINK_BIT | number` for a port, and an exit signal from a
//! port carries the same value as its sender.

pub mod bif;
mod frame;
mod spawn;

use crate::heap::{Ctx, Fragment};
use crate::nif::Env;
use crate::sched::{self, Signal};
use crate::term::*;
use frame::{Framer, Mode, Piece};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Marks a port in link sets and exit signal senders; pids stay far below it.
pub const LINK_BIT: u64 = 1 << 60;

pub struct Port {
    pub id: u64,
    name: String,
    os_pid: u32,
    binary: bool,
    mode: Mode,
    st: Mutex<State>,
    stop: tokio::sync::Notify,
}

struct State {
    alive: bool,
    owner: u64,
    links: HashSet<u64>,
    /// (monitor ref id, watcher pid)
    monitors: Vec<(u64, u64)>,
    input: u64,
    output: u64,
    writer: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
}

static PORTS: Mutex<BTreeMap<u64, Arc<Port>>> = parking_lot::const_mutex(BTreeMap::new());
/// #Port<0.0> to #Port<0.2> are the BEAM's own; a script's first port is 3.
static NEXT: AtomicU64 = AtomicU64::new(3);

fn a(name: &str) -> Term {
    atom(crate::atoms::intern(name))
}

pub fn lookup(t: Term) -> Option<Arc<Port>> {
    if !is_port(t) {
        return None;
    }
    PORTS.lock().get(&word(t, 1)).cloned()
}

pub fn all() -> Vec<u64> {
    PORTS.lock().keys().copied().collect()
}

pub fn term(ctx: &mut Ctx, id: u64) -> Term {
    let p = ctx.alloc(2);
    unsafe {
        *p = header(T_PORT, 1);
        *p.add(1) = id;
    }
    p as Term
}

fn env_term(env: &mut Env, id: u64) -> Term {
    let p = env.alloc(2);
    unsafe {
        *p = header(T_PORT, 1);
        *p.add(1) = id;
    }
    p as Term
}

/// The port number behind a link-set entry or exit signal sender.
pub fn linked_port(entry: u64) -> Option<u64> {
    (entry & LINK_BIT != 0).then_some(entry & !LINK_BIT)
}

/// The pid or port a link-set entry or exit signal sender stands for.
pub fn source_term(ctx: &mut Ctx, entry: u64) -> Term {
    match linked_port(entry) {
        Some(id) => term(ctx, id),
        None => pid(entry),
    }
}

fn deliver(to: u64, build: impl FnOnce(&mut Env) -> Term) {
    let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
    let message = build(&mut env);
    sched::send_fragment(to, env.export(message));
}

/// `{port, body}` to `to`.
fn tell(port: &Port, to: u64, body: impl FnOnce(&mut Env) -> Term) {
    deliver(to, |env| {
        let p = env_term(env, port.id);
        let b = body(env);
        env.tuple(&[p, b])
    })
}

fn payload(env: &mut Env, binary: bool, bytes: &[u8]) -> Term {
    if binary {
        return env.bytes(bytes);
    }
    bytes.iter().rev().fold(NIL_LIST, |acc, b| env.cons(small(*b as i64), acc))
}

fn exit_to(port: u64, to: u64, reason: &Fragment) {
    sched::send_signal(to, Signal::Exit { from: LINK_BIT | port, reason: sched::clone_fragment(reason), link: true });
}

fn atom_fragment(name: &str) -> Fragment {
    Env::new(std::ptr::null_mut(), std::ptr::null_mut()).export(a(name))
}

// ---------------------------------------------------------------------------
// Opening
// ---------------------------------------------------------------------------

/// The BEAM ignores SIGPIPE; without that, writing to a program that exited
/// would kill us instead of failing the write.
fn ignore_sigpipe() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    });
}

pub enum OpenError {
    Badarg(spawn::Bad),
    Posix(&'static str),
}

pub fn open(owner: u64, name: Term, opts: Term) -> Result<u64, OpenError> {
    let spec = spawn::parse(name, opts).map_err(OpenError::Badarg)?;
    ignore_sigpipe();
    crate::io::stdout_flush();
    // the number is taken even when the program fails to start, as on the BEAM
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let started = spawn::start(&spec).map_err(|e| OpenError::Posix(spawn::posix(&e)))?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let port = Arc::new(Port {
        id,
        name: spec.name.clone(),
        os_pid: started.os_pid,
        binary: spec.binary,
        mode: spec.mode,
        st: Mutex::new(State {
            alive: true,
            owner,
            links: HashSet::from([owner]),
            monitors: Vec::new(),
            input: 0,
            output: 0,
            writer: Some(tx),
        }),
        stop: tokio::sync::Notify::new(),
    });
    PORTS.lock().insert(id, port.clone());
    sched::link_entry(owner, LINK_BIT | id, true);
    let rt = sched::runtime();
    if let Some(input) = started.input {
        rt.spawn(write_loop(input, rx));
    }
    rt.spawn(read_loop(port, started.output, started.child, spec));
    Ok(id)
}

async fn write_loop(mut input: tokio::net::unix::pipe::Sender, mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) {
    // Queued commands still go out after a close; then the program sees EOF.
    while let Some(data) = rx.recv().await {
        if input.write_all(&data).await.is_err() {
            return;
        }
    }
}

/// Delivers output until end of file, then reports the exit status, `:eof`
/// or closes the port, in OTP's order.
async fn read_loop(port: Arc<Port>, output: Option<tokio::net::unix::pipe::Receiver>, mut child: tokio::process::Child, spec: spawn::Spec) {
    let mut framer = Framer::new(spec.mode);
    let reading = output.is_some();
    if let Some(mut output) = output {
        let mut buf = vec![0u8; 65536];
        loop {
            let n = tokio::select! {
                r = output.read(&mut buf) => r.unwrap_or(0),
                _ = port.stop.notified() => return,
            };
            if n == 0 {
                break;
            }
            if !send_pieces(&port, framer.push(&buf[..n]), n) {
                return;
            }
        }
    }
    // Without output to read, the program's exit stands for end of file.
    if spec.exit_status || !reading {
        let status = tokio::select! {
            s = child.wait() => s.ok(),
            _ = port.stop.notified() => return,
        };
        if let (true, Some(status)) = (spec.exit_status, status) {
            let code = spawn::exit_code(status);
            if !with_live(&port, |owner| tell(&port, owner, |env| env.tuple(&[a("exit_status"), small(code)]))) {
                return;
            }
        }
    }
    if !send_pieces(&port, framer.finish(), 0) {
        return;
    }
    if spec.eof {
        with_live(&port, |owner| tell(&port, owner, |_| a("eof")));
        return;
    }
    close(&port, atom_fragment("normal"), None);
}

/// Runs `f` with the owner while the port is open; holding the lock keeps a
/// close from slipping in, so nothing arrives after `port_close` returns.
fn with_live(port: &Port, f: impl FnOnce(u64)) -> bool {
    let st = port.st.lock();
    if !st.alive {
        return false;
    }
    f(st.owner);
    true
}

fn send_pieces(port: &Port, pieces: Vec<Piece>, read: usize) -> bool {
    let mut st = port.st.lock();
    if !st.alive {
        return false;
    }
    st.input += read as u64;
    for piece in pieces {
        tell(port, st.owner, |env| {
            let data = match &piece {
                Piece::Data(d) => payload(env, port.binary, d),
                Piece::Eol(d) => {
                    let p = payload(env, port.binary, d);
                    env.tuple(&[a("eol"), p])
                }
                Piece::Noeol(d) => {
                    let p = payload(env, port.binary, d);
                    env.tuple(&[a("noeol"), p])
                }
            };
            env.tuple(&[a("data"), data])
        });
    }
    true
}

// ---------------------------------------------------------------------------
// Commands and closing
// ---------------------------------------------------------------------------

/// Queues `data` for the program; false when the port is closed. Data too
/// long for the packet header closes the port with `:einval`.
pub fn command(port: &Port, data: Vec<u8>) -> bool {
    let Some(mut framed) = frame::header(port.mode, data.len()) else {
        return close(port, atom_fragment("einval"), None);
    };
    let len = data.len();
    framed.extend(data);
    let mut st = port.st.lock();
    if !st.alive {
        return false;
    }
    st.output += len as u64;
    if let Some(w) = &st.writer {
        let _ = w.send(framed);
    }
    true
}

/// The error the BEAM logs when a command is not iodata.
pub fn bad_value(port: &Port) {
    let text = format!("\n{} [error] Bad value on output port '{}'\n\n", crate::bif_more::log_time(), port.name);
    let text = if crate::bif_more::stdout_is_tty() { format!("\x1b[31m{}\x1b[0m\n", text) } else { format!("{}\n", text) };
    crate::io::stdout_write(text.as_bytes());
}

/// Closes the port: links get an exit signal with `reason`, monitors a DOWN,
/// and `reply` (a `{pid, :close}` sender) its `{port, :closed}` in between.
pub fn close(port: &Port, reason: Fragment, reply: Option<u64>) -> bool {
    let (links, monitors) = {
        let mut st = port.st.lock();
        if !st.alive {
            return false;
        }
        st.alive = false;
        st.writer = None;
        (std::mem::take(&mut st.links), std::mem::take(&mut st.monitors))
    };
    PORTS.lock().remove(&port.id);
    port.stop.notify_one();
    for l in links {
        sched::link_entry(l, LINK_BIT | port.id, false);
        exit_to(port.id, l, &reason);
    }
    if let Some(to) = reply {
        tell(port, to, |_| a("closed"));
    }
    for (refid, watcher) in monitors {
        if sched::monitor_claim(refid) {
            deliver(watcher, |env| down(env, refid, port.id, &reason));
        }
    }
    true
}

fn down(env: &mut Env, refid: u64, id: u64, reason: &Fragment) -> Term {
    let r = env.alloc(2);
    unsafe {
        *r = header(T_REF, 1);
        *r.add(1) = refid;
    }
    let p = env_term(env, id);
    let why = env.adopt(sched::clone_fragment(reason));
    env.tuple(&[atom(a::DOWN), r as Term, a("port"), p, why])
}

/// A message sent to the port with `send/2`. Only the connected process may
/// command it this way; anything else answers with a `badsig` exit signal.
pub fn message(dest: Term, msg: Term) -> Term {
    let Some(port) = lookup(dest) else { return msg };
    let owner = port.st.lock().owner;
    let from_owner = is_tuple(msg) && tuple_size(msg) == 2 && tuple_get(msg, 0) == pid(owner);
    let body = if from_owner { tuple_get(msg, 1) } else { NONE };
    let ok = if body == a("close") {
        close(&port, atom_fragment("normal"), Some(owner))
    } else if is_tuple(body) && tuple_size(body) == 2 && tuple_get(body, 0) == a("command") {
        let mut bytes = Vec::new();
        let valid = crate::nif::flatten_iolist(tuple_get(body, 1), &mut bytes);
        if !valid {
            bad_value(&port);
        }
        valid && command(&port, bytes)
    } else if is_tuple(body) && tuple_size(body) == 2 && tuple_get(body, 0) == a("connect") && is_pid(tuple_get(body, 1)) {
        connect(&port, pid_id(tuple_get(body, 1)));
        tell(&port, owner, |_| a("connected"));
        true
    } else {
        false
    };
    if !ok && port.st.lock().alive {
        exit_to(port.id, owner, &atom_fragment("badsig"));
    }
    msg
}

/// Makes `to` the connected process, linked to the port.
pub fn connect(port: &Port, to: u64) -> bool {
    {
        let mut st = port.st.lock();
        if !st.alive {
            return false;
        }
        st.owner = to;
        st.links.insert(to);
    }
    sched::link_entry(to, LINK_BIT | port.id, true);
    true
}

// ---------------------------------------------------------------------------
// Links, monitors and exit signals
// ---------------------------------------------------------------------------

pub fn link(ctx: &mut Ctx, t: Term) {
    let me = ctx.pid();
    let id = word(t, 1);
    let linked = lookup(t).is_some_and(|port| {
        let mut st = port.st.lock();
        if st.alive {
            st.links.insert(me);
        }
        st.alive
    });
    if linked {
        sched::link_entry(me, LINK_BIT | id, true);
    } else {
        exit_to(id, me, &atom_fragment("noproc"));
    }
}

pub fn unlink(me: u64, t: Term) {
    if let Some(port) = lookup(t) {
        port.st.lock().links.remove(&me);
    }
    sched::link_entry(me, LINK_BIT | word(t, 1), false);
}

pub fn monitor(ctx: &mut Ctx, t: Term) -> Term {
    let me = ctx.pid();
    let id = word(t, 1);
    let refid = sched::next_ref_id();
    let watched = lookup(t).is_some_and(|port| {
        let mut st = port.st.lock();
        if st.alive {
            st.monitors.push((refid, me));
            sched::monitor_register(refid, LINK_BIT | id, me);
        }
        st.alive
    });
    if !watched {
        deliver(me, |env| down(env, refid, id, &atom_fragment("noproc")));
    }
    sched::mk_ref_term(ctx, refid)
}

/// `exit(port, reason)`: any exit signal closes the port.
pub fn exit_signal(ctx: &mut Ctx, t: Term, reason: Term) {
    let reason = if reason == atom(a::KILL) { atom(a::KILLED) } else { reason };
    let f = ctx.to_fragment(reason);
    if let Some(port) = lookup(t) {
        close(&port, f, None);
    }
}

/// A process linked to the port exited: the connected process takes the port
/// with it; others only close it with an abnormal reason.
pub fn linked_exited(id: u64, who: u64, reason: &Fragment) {
    let Some(port) = PORTS.lock().get(&id).cloned() else { return };
    let owner = {
        let mut st = port.st.lock();
        st.links.remove(&who);
        st.owner
    };
    let normal = reason.root == atom(a::NORMAL);
    if owner == who || !normal {
        close(&port, sched::clone_fragment(reason), None);
    }
}

/// Ports close with their connected process even when it unlinked them.
pub fn owner_exited(who: u64, reason: &Fragment) {
    let owned: Vec<Arc<Port>> = PORTS.lock().values().filter(|p| p.st.lock().owner == who).cloned().collect();
    for port in owned {
        close(&port, sched::clone_fragment(reason), None);
    }
}
