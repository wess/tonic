//! `open_port/2` arguments and starting the external program.

use super::frame::Mode;
use crate::term::*;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::Stdio;
use tokio::net::unix::pipe;

pub struct Spec {
    pub name: String,
    program: String,
    args: Vec<String>,
    arg0: Option<String>,
    cd: Option<String>,
    env: Vec<(String, Option<String>)>,
    pub binary: bool,
    pub exit_status: bool,
    pub eof: bool,
    pub mode: Mode,
    stderr_to_stdout: bool,
    use_stdio: bool,
    read: bool,
    write: bool,
    shell: bool,
}

/// Which argument was wrong, for the badarg's error info.
pub enum Bad {
    Name,
    Options,
}

/// Text of a binary or a flat list of code points.
pub fn text(t: Term) -> Option<String> {
    if is_binary(t) {
        return String::from_utf8(bin_bytes(t).to_vec()).ok();
    }
    let mut s = String::new();
    let mut l = t;
    while is_cons(l) {
        let c = head(l);
        s.push(if is_small(c) { char::from_u32(small_val(c) as u32)? } else { return None });
        l = tail(l);
    }
    (l == NIL_LIST).then_some(s)
}

fn items(mut l: Term) -> Option<Vec<Term>> {
    let mut v = Vec::new();
    while is_cons(l) {
        v.push(head(l));
        l = tail(l);
    }
    (l == NIL_LIST).then_some(v)
}

fn name_of(t: Term) -> &'static str {
    crate::atoms::name(atom_idx(t))
}

pub fn parse(name: Term, opts: Term) -> Result<Spec, Bad> {
    if !is_tuple(name) || tuple_size(name) != 2 || !is_atom(tuple_get(name, 0)) {
        return Err(Bad::Name);
    }
    let command = text(tuple_get(name, 1)).ok_or(Bad::Name)?;
    let shell = match name_of(tuple_get(name, 0)) {
        "spawn" => true,
        "spawn_executable" => false,
        _ => return Err(Bad::Name),
    };
    // OTP runs a spawn command line through the shell with exec, so the
    // command becomes the port's process.
    let (program, args) = if shell {
        ("/bin/sh".to_string(), vec!["-c".to_string(), format!("exec {}", command)])
    } else {
        (command.clone(), Vec::new())
    };
    let mut spec = Spec {
        name: command,
        program,
        args,
        arg0: None,
        cd: None,
        env: Vec::new(),
        binary: false,
        exit_status: false,
        eof: false,
        mode: Mode::Stream,
        stderr_to_stdout: false,
        use_stdio: true,
        read: false,
        write: false,
        shell,
    };
    for opt in items(opts).ok_or(Bad::Options)? {
        option(&mut spec, opt).ok_or(Bad::Options)?;
    }
    if !spec.read && !spec.write {
        spec.read = true;
        spec.write = true;
    }
    Ok(spec)
}

/// `args` and `arg0` only apply to `spawn_executable`; spawn ignores them.
fn option(spec: &mut Spec, opt: Term) -> Option<()> {
    let executable = !spec.shell;
    if is_atom(opt) {
        match name_of(opt) {
            "binary" => spec.binary = true,
            "exit_status" => spec.exit_status = true,
            "eof" => spec.eof = true,
            "stderr_to_stdout" => spec.stderr_to_stdout = true,
            "use_stdio" => spec.use_stdio = true,
            "nouse_stdio" => spec.use_stdio = false,
            "in" => spec.read = true,
            "out" => spec.write = true,
            "stream" => spec.mode = Mode::Stream,
            "hide" | "overlapped_io" => {}
            _ => return None,
        }
        return Some(());
    }
    if !is_tuple(opt) || tuple_size(opt) != 2 || !is_atom(tuple_get(opt, 0)) {
        return None;
    }
    let value = tuple_get(opt, 1);
    match name_of(tuple_get(opt, 0)) {
        "line" if is_small(value) && small_val(value) > 0 => spec.mode = Mode::Line(small_val(value) as usize),
        "packet" if is_small(value) && [1, 2, 4].contains(&small_val(value)) => {
            spec.mode = Mode::Packet(small_val(value) as usize)
        }
        "args" if executable => {
            for a in items(value)? {
                spec.args.push(text(a)?);
            }
        }
        "args" => {}
        "arg0" if executable => spec.arg0 = Some(text(value)?),
        "arg0" => {}
        "cd" => spec.cd = Some(text(value)?),
        "env" => {
            for pair in items(value)? {
                if !is_tuple(pair) || tuple_size(pair) != 2 {
                    return None;
                }
                let v = tuple_get(pair, 1);
                let v = if v == FALSE { None } else { Some(text(v)?) };
                spec.env.push((text(tuple_get(pair, 0))?, v));
            }
        }
        "parallelism" | "busy_limits_port" | "busy_limits_msgq" => {}
        _ => return None,
    }
    Some(())
}

/// The running program and the port's ends of its pipes.
pub struct Started {
    pub child: tokio::process::Child,
    pub os_pid: u32,
    pub input: Option<pipe::Sender>,
    pub output: Option<pipe::Receiver>,
}

fn pipe_pair() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let pair = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    for fd in [fds[0], fds[1]] {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok(pair)
}

pub fn start(spec: &Spec) -> std::io::Result<Started> {
    // A program without a slash is a path, as on the BEAM, not a PATH search.
    let program = if spec.program.contains('/') { spec.program.clone() } else { format!("./{}", spec.program) };
    let mut command = tokio::process::Command::new(&program);
    command.args(&spec.args);
    if let Some(arg0) = &spec.arg0 {
        command.arg0(arg0);
    } else if spec.shell {
        command.arg0("sh");
    }
    if let Some(cd) = &spec.cd {
        command.current_dir(cd);
    }
    for (k, v) in &spec.env {
        match v {
            Some(v) => command.env(k, v),
            None => command.env_remove(k),
        };
    }
    let (input_read, input_write) = pipe_pair()?;
    let (output_read, output_write) = pipe_pair()?;
    if spec.use_stdio {
        command.stdin(if spec.write { Stdio::from(input_read.try_clone()?) } else { Stdio::null() });
        command.stdout(if spec.read { Stdio::from(output_write.try_clone()?) } else { Stdio::null() });
        if spec.stderr_to_stdout && spec.read {
            command.stderr(Stdio::from(output_write.try_clone()?));
        }
    } else {
        // nouse_stdio: the program reads fd 3 and writes fd 4.
        let (r, w) = (input_read.as_raw_fd(), output_write.as_raw_fd());
        unsafe {
            command.pre_exec(move || {
                let (r, w) = (libc::dup(r), libc::dup(w));
                if r < 0 || w < 0 || libc::dup2(r, 3) < 0 || libc::dup2(w, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let _guard = crate::sched::runtime().enter();
    let child = command.spawn()?;
    drop(command);
    drop(input_read);
    drop(output_write);
    let os_pid = child.id().unwrap_or(0);
    let input = if spec.write { Some(pipe::Sender::from_owned_fd(input_write)?) } else { None };
    let output = if spec.read { Some(pipe::Receiver::from_owned_fd(output_read)?) } else { None };
    Ok(Started { child, os_pid, input, output })
}

/// OTP's name for a failure to start the program.
pub fn posix(e: &std::io::Error) -> &'static str {
    match e.raw_os_error() {
        Some(libc::ENOENT) => "enoent",
        Some(libc::EACCES) | Some(libc::EPERM) => "eacces",
        Some(libc::ENOTDIR) => "enotdir",
        Some(libc::ENOEXEC) => "enoexec",
        Some(libc::ENOMEM) => "enomem",
        Some(libc::EAGAIN) => "eagain",
        Some(libc::EMFILE) => "emfile",
        Some(libc::ENFILE) => "enfile",
        Some(libc::ENAMETOOLONG) => "enametoolong",
        Some(libc::E2BIG) => "e2big",
        _ => match e.kind() {
            std::io::ErrorKind::NotFound => "enoent",
            std::io::ErrorKind::PermissionDenied => "eacces",
            _ => "einval",
        },
    }
}

/// Exit code as OTP reports it: 128 + signal for a killed program.
pub fn exit_code(status: std::process::ExitStatus) -> i64 {
    use std::os::unix::process::ExitStatusExt;
    status.code().map(|c| c as i64).unwrap_or_else(|| 128 + status.signal().unwrap_or(0) as i64)
}
