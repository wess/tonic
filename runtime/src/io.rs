//! Standard IO with buffering that is safe to use from any process/thread.
//!
//! Output to a terminal is written immediately. Output to a pipe or file is
//! batched, but a flusher thread drains it within a few milliseconds so logs
//! of long-running programs appear promptly, and termination signals flush
//! before the process dies.

use parking_lot::{Condvar, Mutex};
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;
use std::time::Duration;

struct Out {
    buf: Vec<u8>,
    tty: bool,
}

/// How long batched output may wait before the flusher writes it.
const LATENCY: Duration = Duration::from_millis(5);

fn out() -> &'static (Mutex<Out>, Condvar) {
    static O: OnceLock<(Mutex<Out>, Condvar)> = OnceLock::new();
    O.get_or_init(|| {
        (
            Mutex::new(Out {
                buf: Vec::with_capacity(1 << 16),
                tty: std::io::stdout().is_terminal(),
            }),
            Condvar::new(),
        )
    })
}

pub fn init() {
    let (o, _) = out();
    if !o.lock().tty {
        std::thread::Builder::new()
            .name("tonic-stdout".into())
            .spawn(flusher)
            .expect("stdout flusher thread");
    }
    alt_stack();
    signals::install();
    // a runtime panic aborts; keep what the program already printed
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        flush_from_fault();
        previous(info);
    }));
}

/// Gives the calling thread an alternate signal stack, so the fault handler
/// can run after a stack overflow. Rust only does this itself when it owns
/// `main`, which a compiled program's runtime does not.
pub fn alt_stack() {
    #[cfg(unix)]
    unsafe {
        const SIZE: usize = 64 * 1024;
        let mem = Box::leak(vec![0u8; SIZE].into_boxed_slice());
        let ss = libc::stack_t { ss_sp: mem.as_mut_ptr() as *mut libc::c_void, ss_flags: 0, ss_size: SIZE };
        libc::sigaltstack(&ss, std::ptr::null_mut());
    }
}

/// Best-effort flush for crash paths, where the output lock may be held by
/// the thread that is failing.
pub fn flush_from_fault() {
    if let Some(mut o) = out().0.try_lock() {
        if !o.buf.is_empty() {
            unsafe { libc::write(1, o.buf.as_ptr() as *const libc::c_void, o.buf.len()) };
            o.buf.clear();
        }
    }
}

fn flusher() {
    let (o, cv) = out();
    loop {
        {
            let mut g = o.lock();
            while g.buf.is_empty() {
                cv.wait(&mut g);
            }
        }
        std::thread::sleep(LATENCY);
        stdout_flush();
    }
}

fn write_out(o: &mut Out) {
    let mut so = std::io::stdout().lock();
    let _ = so.write_all(&o.buf);
    let _ = so.flush();
    o.buf.clear();
}

pub fn stdout_write(bytes: &[u8]) {
    let (o, cv) = out();
    let mut o = o.lock();
    let was_empty = o.buf.is_empty();
    o.buf.extend_from_slice(bytes);
    if o.tty || o.buf.len() > (1 << 15) {
        write_out(&mut o);
    } else if was_empty {
        cv.notify_one();
    }
}

pub fn stdout_flush() {
    let mut o = out().0.lock();
    if !o.buf.is_empty() {
        write_out(&mut o);
    }
}

pub fn stderr_write(bytes: &[u8]) {
    stdout_flush();
    let mut e = std::io::stderr().lock();
    let _ = e.write_all(bytes);
    let _ = e.flush();
}

/// SIGHUP, SIGINT and SIGTERM keep their default effect, but buffered output
/// is written first. The handler only writes the signal number to a pipe; a
/// watcher thread does the flushing outside signal context.
#[cfg(unix)]
mod signals {
    use std::sync::atomic::{AtomicI32, Ordering};

    extern "C" {
        fn pipe(fds: *mut i32) -> i32;
        fn read(fd: i32, buf: *mut u8, n: usize) -> isize;
        fn write(fd: i32, buf: *const u8, n: usize) -> isize;
        fn signal(sig: i32, handler: usize) -> usize;
        fn raise(sig: i32) -> i32;
    }

    const SIG_DFL: usize = 0;
    const SIG_IGN: usize = 1;
    const SIGNALS: [i32; 3] = [1, 2, 15];

    static WRITE_FD: AtomicI32 = AtomicI32::new(-1);

    extern "C" fn notify(sig: i32) {
        let fd = WRITE_FD.load(Ordering::Relaxed);
        let b = sig as u8;
        unsafe { write(fd, &b, 1) };
    }

    pub fn install() {
        let mut fds = [0i32; 2];
        if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
            return;
        }
        WRITE_FD.store(fds[1], Ordering::Relaxed);
        let rd = fds[0];
        let spawned = std::thread::Builder::new().name("tonic-signals".into()).spawn(move || loop {
            let mut b = 0u8;
            if unsafe { read(rd, &mut b, 1) } != 1 {
                continue;
            }
            super::stdout_flush();
            unsafe {
                signal(b as i32, SIG_DFL);
                raise(b as i32);
            }
        });
        if spawned.is_err() {
            return;
        }
        for s in SIGNALS {
            // a signal ignored by whoever started us (nohup, background jobs) stays ignored
            if unsafe { signal(s, notify as *const () as usize) } == SIG_IGN {
                unsafe { signal(s, SIG_IGN) };
            }
        }
        for s in [libc::SIGSEGV, libc::SIGBUS] {
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = fault as *const () as usize;
                sa.sa_flags = libc::SA_ONSTACK | libc::SA_RESETHAND;
                libc::sigemptyset(&mut sa.sa_mask);
                libc::sigaction(s, &sa, std::ptr::null_mut());
            }
        }
    }

    const FAULT: &str = "\ntonic: fatal memory fault. The usual cause is a process exceeding its stack \
(deep non-tail recursion); raise TONIC_PROC_STACK_MB (default 256) or TONIC_MAIN_STACK_MB \
(default 1024).\n";

    /// Runs on the thread's alternate signal stack (Rust sets one up for every
    /// thread), so it works after a stack overflow. SA_RESETHAND restores the
    /// default action: returning re-executes the fault, which then terminates.
    extern "C" fn fault(_sig: i32) {
        super::flush_from_fault();
        unsafe { libc::write(2, FAULT.as_ptr() as *const libc::c_void, FAULT.len()) };
    }
}

#[cfg(not(unix))]
mod signals {
    pub fn install() {}
}
