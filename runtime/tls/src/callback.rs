//! Run handshake callbacks in the process awaiting the handshake.

use crate::options::{self, Opts};
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::net::{a, deliver, socket_term};
use tonic_core::nif::Env;
use tonic_core::term::*;

pub enum Answer {
    Accept(bool),
    Anchor(Option<Vec<u8>>),
    Options(Opts),
}

struct Pending {
    owner: u64,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    sender: Sender<Answer>,
    options: Opts,
}

static PENDING: parking_lot::Mutex<Option<HashMap<u64, Pending>>> = parking_lot::const_mutex(None);

fn owner_exited(pid: u64) {
    if let Some(pending) = PENDING.lock().as_mut() {
        pending.retain(|_, p| p.owner != pid);
    }
}

pub fn cancel(token: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    token.store(true, std::sync::atomic::Ordering::Release);
    if let Some(pending) = PENDING.lock().as_mut() {
        pending.retain(|_, p| !std::sync::Arc::ptr_eq(&p.cancel, token));
    }
}

pub fn call(opts: &Opts, event: impl FnOnce(&mut Env) -> Term) -> Option<Answer> {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| tonic_core::sched::on_exit(owner_exited));
    let id = tonic_core::sched::next_ref_id();
    let (sender, receiver) = mpsc::channel();
    PENDING.lock().get_or_insert_with(HashMap::new).insert(
        id,
        Pending {
            owner: opts.callback_owner,
            cancel: opts.callback_cancel.clone(),
            sender,
            options: opts.clone(),
        },
    );
    if opts
        .callback_cancel
        .load(std::sync::atomic::Ordering::Acquire)
        || !tonic_core::sched::is_alive(opts.callback_owner)
    {
        PENDING
            .lock()
            .as_mut()
            .and_then(|pending| pending.remove(&id));
        return None;
    }
    deliver(opts.callback_owner, |env| {
        let operation = socket_term(env, opts.callback_operation);
        let reference = socket_term(env, id);
        let event = event(env);
        env.tuple(&[a("$tonic_ssl_callback"), operation, reference, event])
    });
    let wait = || match opts.callback_deadline {
        Some(deadline) => match receiver
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            Ok(answer) => Some(answer),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                opts.callback_timed_out
                    .store(true, std::sync::atomic::Ordering::Release);
                None
            }
            Err(_) => None,
        },
        None => receiver.recv().ok(),
    };
    // rustls' verifier is synchronous. Let Tokio replace this worker while
    // the Elixir process runs the callback and other handshakes make progress.
    let answer = tokio::task::block_in_place(wait);
    PENDING
        .lock()
        .as_mut()
        .and_then(|pending| pending.remove(&id));
    answer
}

pub fn accepted(opts: &Opts, event: impl FnOnce(&mut Env) -> Term) -> bool {
    matches!(call(opts, event), Some(Answer::Accept(true)))
}

#[no_mangle]
pub extern "C" fn tn_ssl_callback_reply(c: *mut Ctx, reference: Term, result: Term) -> Term {
    let ctx = cx(c);
    if !is_ref(reference) {
        return ctx.badarg("argument error");
    }
    let id = word(reference, 1);
    let mut pending = PENDING.lock();
    let Some(entry) = pending.as_mut().and_then(|pending| pending.get(&id)) else {
        return atom(a::OK);
    };
    if entry.owner != ctx.pid() {
        return ctx.badarg("argument error");
    }
    let answer = if is_tuple(result)
        && tuple_size(result) == 2
        && tuple_get(result, 0) == a("trusted_ca")
        && is_binary(tuple_get(result, 1))
    {
        Answer::Anchor(Some(bin_bytes(tuple_get(result, 1)).to_vec()))
    } else if result == a("unknown_ca") {
        Answer::Anchor(None)
    } else if result == TRUE || result == FALSE {
        Answer::Accept(result == TRUE)
    } else {
        let mut opts = entry.options.clone();
        match options::apply(&mut opts, result) {
            Ok(()) => Answer::Options(opts),
            Err(_) => Answer::Accept(false),
        }
    };
    let entry = pending
        .as_mut()
        .and_then(|pending| pending.remove(&id))
        .unwrap();
    let _ = entry.sender.send(answer);
    atom(a::OK)
}
