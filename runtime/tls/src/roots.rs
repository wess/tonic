//! The operating system's trust store, for `:public_key.cacerts_get/0`.
//!
//! Read from the PEM bundle the system maintains (`SSL_CERT_FILE` first), the
//! same files OpenSSL-based tools use. On macOS that is /etc/ssl/cert.pem,
//! which Apple keeps in step with the system roots.

use tonic_core::bif_core::cx;
use tonic_core::heap::Ctx;
use tonic_core::term::*;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use std::sync::OnceLock;

const BUNDLES: &[&str] = &[
    "/etc/ssl/cert.pem",
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    "/etc/ssl/ca-bundle.pem",
    "/usr/local/etc/openssl/cert.pem",
    "/opt/homebrew/etc/openssl@3/cert.pem",
];

fn load() -> Vec<Vec<u8>> {
    let from_env = std::env::var("SSL_CERT_FILE").ok().into_iter();
    let paths = from_env.chain(BUNDLES.iter().map(|p| p.to_string()));
    for path in paths {
        let Ok(pem) = std::fs::read(&path) else { continue };
        let certs: Vec<Vec<u8>> = CertificateDer::pem_slice_iter(&pem).filter_map(Result::ok).map(|c| c.to_vec()).collect();
        if !certs.is_empty() {
            return certs;
        }
    }
    Vec::new()
}

/// DER binaries of the system roots, or `{:error, :no_cacerts_found}`.
#[no_mangle]
pub extern "C" fn tn_ssl_cacerts(c: *mut Ctx) -> Term {
    static CERTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    let ctx = cx(c);
    let certs = CERTS.get_or_init(load);
    if certs.is_empty() {
        return tonic_core::net::ctx_result(ctx, None, Some("no_cacerts_found"));
    }
    crate::build(ctx, |env| {
        certs.iter().rev().fold(NIL_LIST, |tail, der| {
            let b = env.bytes(der);
            env.cons(b, tail)
        })
    })
}
