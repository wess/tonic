//! Runs a compiled program in the foreground for `tonic run`.
//!
//! Termination signals sent to tonic are passed on to the program, so killing
//! `tonic run` never leaves the program running behind it, and tonic stays
//! alive long enough to delete the temporary executable.

use std::path::Path;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicI32, Ordering};

static CHILD: AtomicI32 = AtomicI32::new(0);
static EARLY: AtomicI32 = AtomicI32::new(0);
static LAST: AtomicI32 = AtomicI32::new(0);

#[cfg(unix)]
use libc::{kill, signal};

/// Signals whose default action ends tonic; each is passed on to the program.
#[cfg(unix)]
const SIGNALS: [i32; 7] = [
    libc::SIGHUP,
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTERM,
    libc::SIGALRM,
    libc::SIGUSR1,
    libc::SIGUSR2,
];

#[cfg(unix)]
extern "C" fn forward(sig: i32) {
    LAST.store(sig, Ordering::SeqCst);
    let pid = CHILD.load(Ordering::SeqCst);
    if pid > 0 {
        unsafe { kill(pid, sig) };
    } else {
        EARLY.store(sig, Ordering::SeqCst);
    }
}

/// Signal the program was last sent through tonic, if any.
pub fn forwarded() -> i32 {
    LAST.load(Ordering::SeqCst)
}

pub fn run(exe: &Path, args: &[String]) -> std::io::Result<ExitStatus> {
    #[cfg(unix)]
    for s in SIGNALS {
        // a signal ignored by whoever started tonic (nohup, background jobs)
        // stays ignored, for tonic and so for the program
        if unsafe { signal(s, forward as *const () as libc::sighandler_t) } == libc::SIG_IGN {
            unsafe { signal(s, libc::SIG_IGN) };
        }
    }
    let mut command = Command::new(exe);
    command.args(args);
    // SIGKILL cannot be passed on; on Linux the program dies with tonic anyway
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    CHILD.store(child.id() as i32, Ordering::SeqCst);
    #[cfg(unix)]
    {
        // a signal that arrived while spawning
        let early = EARLY.swap(0, Ordering::SeqCst);
        if early != 0 {
            unsafe { kill(child.id() as i32, early) };
        }
    }
    child.wait()
}
