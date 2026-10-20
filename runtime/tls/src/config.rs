//! rustls client and server configurations from `Opts`.

use crate::alert::Failure;
use crate::options::Opts;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier};
use rustls::sign::CertifiedKey;
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme};
use std::sync::{Arc, OnceLock};

pub fn provider() -> Arc<CryptoProvider> {
    static P: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    P.get_or_init(|| Arc::new(rustls::crypto::ring::default_provider())).clone()
}

fn versions(opts: &Opts) -> Vec<&'static rustls::SupportedProtocolVersion> {
    opts.versions
        .iter()
        .map(|v| if *v == "tlsv1.3" { &rustls::version::TLS13 } else { &rustls::version::TLS12 })
        .collect()
}

fn read(name: &'static str, path: &str) -> Result<Vec<u8>, Failure> {
    std::fs::read(path).map_err(|e| Failure::File(name, path.to_string(), file_error(&e)))
}

fn file_error(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::NotFound => "enoent",
        std::io::ErrorKind::PermissionDenied => "eacces",
        _ => "eio",
    }
}

/// Trust anchors from `cacerts` and `cacertfile`; unparsable ones are skipped.
fn trust(opts: &Opts) -> Result<RootCertStore, Failure> {
    let mut ders: Vec<CertificateDer<'static>> = opts.cacerts.iter().map(|d| CertificateDer::from(d.clone())).collect();
    if let Some(path) = &opts.cacertfile {
        let pem = read("cacertfile", path)?;
        ders.extend(CertificateDer::pem_slice_iter(&pem).filter_map(Result::ok));
    }
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(ders);
    Ok(roots)
}

/// Our own certificate chain and key, if any. The key file defaults to the
/// certificate file, as in OTP.
fn chain(opts: &Opts) -> Result<Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>, Failure> {
    let mut certs: Vec<CertificateDer<'static>> = opts.certs.iter().map(|d| CertificateDer::from(d.clone())).collect();
    if certs.is_empty() {
        if let Some(path) = &opts.certfile {
            let pem = read("certfile", path)?;
            certs = CertificateDer::pem_slice_iter(&pem).filter_map(Result::ok).collect();
            if certs.is_empty() {
                return Err(Failure::Options("certfile", path.clone()));
            }
        }
    }
    if certs.is_empty() {
        return Ok(None);
    }
    let key = match (&opts.key, opts.keyfile.as_ref().or(opts.certfile.as_ref())) {
        (Some(der), _) => PrivateKeyDer::try_from(der.clone()).map_err(|_| Failure::Options("key", String::new()))?,
        (None, Some(path)) => {
            let pem = read("keyfile", path)?;
            PrivateKeyDer::from_pem_slice(&pem).map_err(|_| Failure::Options("keyfile", path.clone()))?
        }
        (None, None) => return Err(Failure::Options("key", String::new())),
    };
    Ok(Some((certs, key)))
}

fn certified(opts: &Opts) -> Result<Option<Arc<CertifiedKey>>, Failure> {
    match chain(opts)? {
        Some((certs, key)) => CertifiedKey::from_der(certs, key, &provider())
            .map(|k| Some(Arc::new(k)))
            .map_err(|_| Failure::Options("keyfile", opts.keyfile.clone().unwrap_or_default())),
        None => Ok(None),
    }
}

/// `verify: :verify_none`: any certificate, but signatures still checked.
#[derive(Debug)]
struct Unverified(Arc<CryptoProvider>);

impl ServerCertVerifier for Unverified {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub fn client(opts: &Opts) -> Result<Arc<ClientConfig>, Failure> {
    let builder = ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&versions(opts))
        .map_err(|_| Failure::Options("versions", String::new()))?;
    let builder = if opts.verify {
        builder.with_root_certificates(trust(opts)?)
    } else {
        builder.dangerous().with_custom_certificate_verifier(Arc::new(Unverified(provider())))
    };
    let mut config = match chain(opts)? {
        Some((certs, key)) => builder
            .with_client_auth_cert(certs, key)
            .map_err(|_| Failure::Options("keyfile", opts.keyfile.clone().unwrap_or_default()))?,
        None => builder.with_no_client_auth(),
    };
    config.alpn_protocols = opts.alpn.clone();
    config.enable_sni = !opts.sni_disabled;
    Ok(Arc::new(config))
}

/// Picks the certificate for the SNI host name (`sni_hosts`), or the default.
#[derive(Debug)]
struct Certs {
    default: Option<Arc<CertifiedKey>>,
    hosts: Vec<(String, Arc<CertifiedKey>)>,
}

impl ResolvesServerCert for Certs {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let host = hello.server_name().map(|h| h.to_ascii_lowercase());
        host.and_then(|h| self.hosts.iter().find(|(name, _)| name.eq_ignore_ascii_case(&h)).map(|(_, k)| k.clone()))
            .or_else(|| self.default.clone())
    }
}

pub fn server(opts: &Opts) -> Result<Arc<ServerConfig>, Failure> {
    let builder = ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&versions(opts))
        .map_err(|_| Failure::Options("versions", String::new()))?;
    let builder = if opts.verify {
        let roots = Arc::new(trust(opts)?);
        let verifier = WebPkiClientVerifier::builder_with_provider(roots, provider());
        let verifier = if opts.fail_if_no_peer_cert { verifier } else { verifier.allow_unauthenticated() };
        builder.with_client_cert_verifier(verifier.build().map_err(|_| Failure::Options("cacerts", String::new()))?)
    } else {
        builder.with_no_client_auth()
    };
    let mut hosts = Vec::new();
    for (name, nested) in &opts.sni_hosts {
        if let Some(key) = certified(nested)? {
            hosts.push((name.clone(), key));
        }
    }
    let default = certified(opts)?;
    if default.is_none() && hosts.is_empty() {
        return Err(Failure::Options("certs_keys", String::new()));
    }
    let mut config = builder.with_cert_resolver(Arc::new(Certs { default, hosts }));
    config.alpn_protocols = opts.alpn_preferred.clone();
    Ok(Arc::new(config))
}
