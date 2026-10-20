//! Failures in OTP's shapes: posix atoms, `{:tls_alert, {name, text}}` and
//! `{:options, ...}`.

use tonic_core::net::{a, posix};
use tonic_core::nif::Env;
use tonic_core::term::*;
use rustls::{AlertDescription, CertificateError, Error};

#[derive(Clone)]
pub enum Failure {
    Posix(&'static str),
    Alert(&'static str, String),
    /// `{:options, {name, detail}}`; the detail is a charlist.
    Options(&'static str, String),
    /// `{:options, {name, path, {:error, posix}}}`: a file that could not be read.
    File(&'static str, String, &'static str),
}

/// OTP's alert atom and description for an alert code.
fn describe(code: u8) -> (&'static str, &'static str) {
    match code {
        0 => ("close_notify", "Close Notify"),
        10 => ("unexpected_message", "Unexpected Message"),
        20 => ("bad_record_mac", "Bad Record MAC"),
        22 => ("record_overflow", "Record Overflow"),
        30 => ("decompression_failure", "Decompression Failure"),
        40 => ("handshake_failure", "Handshake Failure"),
        42 => ("bad_certificate", "Bad Certificate"),
        43 => ("unsupported_certificate", "Unsupported Certificate"),
        44 => ("certificate_revoked", "Certificate Revoked"),
        45 => ("certificate_expired", "Certificate Expired"),
        46 => ("certificate_unknown", "Certificate Unknown"),
        47 => ("illegal_parameter", "Illegal Parameter"),
        48 => ("unknown_ca", "Unknown CA"),
        49 => ("access_denied", "Access Denied"),
        50 => ("decode_error", "Decode Error"),
        51 => ("decrypt_error", "Decrypt Error"),
        70 => ("protocol_version", "Protocol Version"),
        71 => ("insufficient_security", "Insufficient Security"),
        86 => ("inappropriate_fallback", "Inappropriate Fallback"),
        90 => ("user_canceled", "User Canceled"),
        100 => ("no_renegotiation", "No Renegotiation"),
        109 => ("missing_extension", "Missing extension"),
        110 => ("unsupported_extension", "Unsupported Extension"),
        112 => ("unrecognized_name", "Unrecognized Name"),
        116 => ("certificate_required", "Certificate required"),
        120 => ("no_application_protocol", "No application protocol"),
        _ => ("internal_error", "Internal Error"),
    }
}

/// The alert this side sends for a local error, as OTP would name it. A
/// failed host name check is a handshake failure there, not a bad certificate.
fn local(e: &Error) -> AlertDescription {
    match e {
        Error::InvalidCertificate(c) => match c {
            CertificateError::UnknownIssuer => AlertDescription::UnknownCA,
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                AlertDescription::HandshakeFailure
            }
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                AlertDescription::CertificateExpired
            }
            CertificateError::Revoked => AlertDescription::CertificateRevoked,
            CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. } => {
                AlertDescription::UnsupportedCertificate
            }
            _ => AlertDescription::BadCertificate,
        },
        Error::NoCertificatesPresented => AlertDescription::CertificateRequired,
        Error::NoApplicationProtocol => AlertDescription::NoApplicationProtocol,
        Error::PeerIncompatible(_) => AlertDescription::HandshakeFailure,
        Error::PeerMisbehaved(_) => AlertDescription::IllegalParameter,
        Error::InappropriateMessage { .. } | Error::InappropriateHandshakeMessage { .. } => {
            AlertDescription::UnexpectedMessage
        }
        Error::InvalidMessage(_) => AlertDescription::DecodeError,
        Error::DecryptError => AlertDescription::BadRecordMac,
        _ => AlertDescription::InternalError,
    }
}

pub fn from_rustls(e: &Error, server: bool) -> Failure {
    let (me, peer) = if server { ("server", "CLIENT") } else { ("client", "SERVER") };
    let (code, verb, sender) = match e {
        Error::AlertReceived(d) => (u8::from(*d), "received", peer),
        other => (u8::from(local(other)), "generated", if server { "SERVER" } else { "CLIENT" }),
    };
    let (name, text) = describe(code);
    let mut message = format!("TLS {me}: In state handshake {verb} {sender} ALERT: Fatal - {text}\n");
    if let Error::InvalidCertificate(c) = e {
        message.push_str(&format!(" {c:?}"));
    }
    Failure::Alert(name, message)
}

pub fn from_io(e: &std::io::Error, server: bool) -> Failure {
    match e.get_ref().and_then(|inner| inner.downcast_ref::<Error>()) {
        Some(tls) => from_rustls(tls, server),
        None => Failure::Posix(posix(e)),
    }
}

pub fn term(env: &mut Env, f: &Failure) -> Term {
    match f {
        Failure::Posix(reason) => a(reason),
        Failure::Alert(name, text) => {
            let t = crate::charlist(env, text);
            let inner = env.tuple(&[a(name), t]);
            env.tuple(&[a("tls_alert"), inner])
        }
        Failure::Options(name, detail) => {
            let d = crate::charlist(env, detail);
            let inner = env.tuple(&[a(name), d]);
            env.tuple(&[a("options"), inner])
        }
        Failure::File(name, path, reason) => {
            let p = crate::charlist(env, path);
            let e = env.tuple(&[atom(a::ERROR), a(reason)]);
            let inner = env.tuple(&[a(name), p, e]);
            env.tuple(&[a("options"), inner])
        }
    }
}

pub fn error(env: &mut Env, f: &Failure) -> Term {
    let reason = term(env, f);
    env.tuple(&[atom(a::ERROR), reason])
}
