//! Certificate validation and callback decisions.

use crate::{callback, config, options::Opts};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::ParsedCertificate;
use rustls::{CertificateError, DigitallySignedStruct, Error, RootCertStore, SignatureScheme};
use std::sync::Arc;
use tonic_core::net::a;

pub struct Verifier {
    pub opts: Opts,
    pub roots: Arc<RootCertStore>,
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Verifier")
    }
}

fn reason(error: &Error) -> &'static str {
    match error {
        Error::InvalidCertificate(CertificateError::UnknownIssuer) => "unknown_ca",
        Error::InvalidCertificate(
            CertificateError::Expired | CertificateError::ExpiredContext { .. },
        ) => "cert_expired",
        Error::InvalidCertificate(
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. },
        ) => "cert_not_yet_valid",
        Error::InvalidCertificate(
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
        ) => "hostname_check_failed",
        Error::InvalidCertificate(CertificateError::Revoked) => "cert_revoked",
        _ => "invalid_certificate",
    }
}

impl Verifier {
    fn event(&self, der: &CertificateDer<'_>, event: &str, bad: bool) -> bool {
        callback::accepted(&self.opts, |env| {
            let cert = env.bytes(der.as_ref());
            let event = if bad {
                env.tuple(&[a("bad_cert"), a(event)])
            } else {
                a(event)
            };
            env.tuple(&[a("verify"), cert, event])
        })
    }

    fn partial_chain(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
    ) -> Result<Arc<RootCertStore>, Error> {
        if !self.opts.partial_chain_callback {
            return Ok(self.roots.clone());
        }
        let mut chain: Vec<Vec<u8>> = intermediates
            .iter()
            .rev()
            .map(|cert| cert.to_vec())
            .collect();
        chain.push(end.to_vec());
        let issuer = certificate_names(chain.first().unwrap()).map(|(issuer, _)| issuer);
        if let Some(issuer) = issuer {
            if let Ok(roots) = config::trust_der(&self.opts) {
                if let Some(root) = roots.iter().find(|root| {
                    certificate_names(root.as_ref()).is_some_and(|(_, subject)| subject == issuer)
                }) {
                    if chain
                        .first()
                        .is_none_or(|cert| cert.as_slice() != root.as_ref())
                    {
                        chain.insert(0, root.to_vec());
                    }
                }
            }
        }
        let answer = callback::call(&self.opts, |env| {
            let certificates: Vec<_> = chain.iter().map(|cert| env.bytes(cert)).collect();
            let certificates = certificates
                .iter()
                .rev()
                .fold(tonic_core::term::NIL_LIST, |tail, cert| {
                    env.cons(*cert, tail)
                });
            env.tuple(&[a("partial_chain"), certificates])
        });
        match answer {
            Some(callback::Answer::Anchor(Some(anchor))) if chain.contains(&anchor) => {
                let mut roots = (*self.roots).clone();
                roots
                    .add(CertificateDer::from(anchor))
                    .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
                Ok(Arc::new(roots))
            }
            Some(callback::Answer::Anchor(None)) => Ok(self.roots.clone()),
            _ => Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            )),
        }
    }

    fn validate(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        now: UnixTime,
    ) -> Result<(), Error> {
        let parsed = ParsedCertificate::try_from(end)?;
        let provider = config::provider();
        if intermediates.len() > self.opts.depth {
            return Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        let roots = self.partial_chain(end, intermediates)?;
        let chain = rustls::client::verify_server_cert_signed_by_trust_anchor(
            &parsed,
            &roots,
            intermediates,
            now,
            provider.signature_verification_algorithms.all,
        );
        if let Err(error) = chain {
            if !self.opts.verify_callback || !self.event(end, reason(&error), true) {
                return Err(error);
            }
        }
        if !self.opts.sni_disabled {
            let matched = if self.opts.hostname_callback {
                callback::accepted(&self.opts, |env| {
                    let cert = env.bytes(end.as_ref());
                    let name = env.bytes(name.to_str().as_bytes());
                    env.tuple(&[a("hostname"), cert, name])
                })
            } else {
                rustls::client::verify_server_name(&parsed, name).is_ok()
            };
            if !matched {
                let error = Error::InvalidCertificate(CertificateError::NotValidForName);
                if !self.opts.verify_callback || !self.event(end, "hostname_check_failed", true) {
                    return Err(error);
                }
            }
        }
        if self.opts.verify_callback {
            for cert in intermediates.iter().rev() {
                if !self.event(cert, "valid", false) {
                    return Err(Error::InvalidCertificate(
                        CertificateError::ApplicationVerificationFailure,
                    ));
                }
            }
            if !self.event(end, "valid_peer", false) {
                return Err(Error::InvalidCertificate(
                    CertificateError::ApplicationVerificationFailure,
                ));
            }
        }
        Ok(())
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if self.opts.verify {
            self.validate(end, intermediates, name, now)?;
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &config::provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &config::provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        config::provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub struct ClientVerifier {
    pub verifier: Arc<dyn rustls::server::danger::ClientCertVerifier>,
    pub callbacks: Verifier,
}

impl std::fmt::Debug for ClientVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientVerifier")
    }
}

impl rustls::server::danger::ClientCertVerifier for ClientVerifier {
    fn offer_client_auth(&self) -> bool {
        self.verifier.offer_client_auth()
    }
    fn client_auth_mandatory(&self) -> bool {
        self.verifier.client_auth_mandatory()
    }
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        self.verifier.root_hint_subjects()
    }
    fn verify_client_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, Error> {
        if intermediates.len() > self.callbacks.opts.depth {
            return Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        let verifier = if self.callbacks.opts.partial_chain_callback {
            let roots = self.callbacks.partial_chain(end, intermediates)?;
            let builder = rustls::server::WebPkiClientVerifier::builder_with_provider(
                roots,
                config::provider(),
            );
            let builder = if self.callbacks.opts.fail_if_no_peer_cert {
                builder
            } else {
                builder.allow_unauthenticated()
            };
            builder
                .build()
                .map_err(|_| Error::InvalidCertificate(CertificateError::UnknownIssuer))?
        } else {
            self.verifier.clone()
        };
        if let Err(error) = verifier.verify_client_cert(end, intermediates, now) {
            if !self.callbacks.opts.verify_callback
                || !self.callbacks.event(end, reason(&error), true)
            {
                return Err(error);
            }
        }
        if self.callbacks.opts.verify_callback {
            for cert in intermediates.iter().rev() {
                if !self.callbacks.event(cert, "valid", false) {
                    return Err(Error::InvalidCertificate(
                        CertificateError::ApplicationVerificationFailure,
                    ));
                }
            }
            if !self.callbacks.event(end, "valid_peer", false) {
                return Err(Error::InvalidCertificate(
                    CertificateError::ApplicationVerificationFailure,
                ));
            }
        }
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.verifier.supported_verify_schemes()
    }
}

// names only identify a candidate trust anchor; rustls still verifies the chain
fn field(input: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let length = *input.get(1)? as usize;
    let (header, length) = if length < 128 {
        (2, length)
    } else {
        let count = length & 127;
        if count == 0 || count > std::mem::size_of::<usize>() {
            return None;
        }
        let mut length = 0usize;
        for byte in input.get(2..2 + count)? {
            length = length.checked_mul(256)?.checked_add(*byte as usize)?;
        }
        (2 + count, length)
    };
    let end = header.checked_add(length)?;
    Some((
        input.get(..end)?,
        input.get(header..end)?,
        input.get(end..)?,
    ))
}

fn certificate_names(der: &[u8]) -> Option<(&[u8], &[u8])> {
    let (_, certificate, _) = field(der)?;
    let (_, mut tbs, _) = field(certificate)?;
    if tbs.first() == Some(&0xa0) {
        tbs = field(tbs)?.2;
    }
    tbs = field(tbs)?.2;
    tbs = field(tbs)?.2;
    let (issuer, _, rest) = field(tbs)?;
    let (_, _, rest) = field(rest)?;
    let (subject, _, _) = field(rest)?;
    Some((issuer, subject))
}
