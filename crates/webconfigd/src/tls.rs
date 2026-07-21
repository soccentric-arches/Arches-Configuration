//! TLS (spec §4.8).
//!
//! Per-device self-signed cert auto-generated on first boot via rcgen — never
//! a shared baked-in cert. Supports upload of a customer cert/key and CSR
//! generation. HTTP→HTTPS redirect on by default; plain HTTP only as an
//! explicit admin opt-in. No OpenSSL anywhere (spec §2).

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rcgen error: {0}")]
    Rcgen(String),
    #[error("pem parse error: {0}")]
    Pem(String),
}

/// Ensure a self-signed cert + key exist at `cert_path` / `key_path`; generate
/// them on first boot (spec §4.8). The subject CN is the device hostname when
/// available, else "webconfig".
pub fn ensure_self_signed(
    cert_path: &Path,
    key_path: &Path,
    hostname: Option<&str>,
) -> Result<(), TlsError> {
    if cert_path.exists() && key_path.exists() {
        return Ok(());
    }
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // rcgen: generate a self-signed cert. We use the minimal params that build
    // on all three targets (ring provider, no OpenSSL).
    let params = {
        let mut p = rcgen::CertificateParams::new(vec![hostname.unwrap_or("webconfig").to_string()])
            .map_err(|e| TlsError::Rcgen(e.to_string()))?;
        p.distinguished_name = rcgen::DistinguishedName::new();
        p.distinguished_name
            .push(rcgen::DnType::CommonName, hostname.unwrap_or("webconfig"));
        p.self_signed = true;
        p.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
        p
    };
    let cert = params
        .self_signed()
        .map_err(|e| TlsError::Rcgen(e.to_string()))?;
    std::fs::write(cert_path, cert.pem())?;
    std::fs::write(key_path, cert.serialize_private_key_pem())?;
    tracing::info!("generated self-signed TLS cert at {}", cert_path.display());
    Ok(())
}

/// Build a `rustls` ServerConfig from PEM cert + key files.
pub fn server_config(cert_path: &Path, key_path: &Path) -> Result<ServerConfig, TlsError> {
    let cert_pem = std::fs::read(cert_path)?;
    let key_pem = std::fs::read(key_path)?;

    let certs: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_pem.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TlsError::Pem(e.to_string()))?
            .into_iter()
            .map(CertificateDer::from)
            .collect();

    let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
        .map_err(|e| TlsError::Pem(e.to_string()))?
        .ok_or_else(|| TlsError::Pem("no private key in pem".into()))?;

    let cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, PrivateKeyDer::from(key))
        .map_err(|e| TlsError::Pem(e.to_string()))?;
    Ok(cfg)
}

/// An acceptor wrapping a ServerConfig for tokio-rustls.
pub fn acceptor(cert_path: &Path, key_path: &Path) -> Result<Arc<ServerConfig>, TlsError> {
    Ok(Arc::new(server_config(cert_path, key_path)?))
}

/// Generate a CSR (PEM) for a customer CA to sign (spec §4.8). Uses a fresh
/// keypair; the private key is returned alongside so the caller can store it.
pub fn generate_csr(cn: &str) -> Result<(String, String), TlsError> {
    let mut params =
        rcgen::CertificateParams::new(vec![cn.to_string()]).map_err(|e| TlsError::Rcgen(e.to_string()))?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, cn);
    params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
    let cert = params
        .self_signed()
        .map_err(|e| TlsError::Rcgen(e.to_string()))?;
    let csr = cert
        .serialize_request_pem()
        .map_err(|e| TlsError::Rcgen(e.to_string()))?;
    let key = cert.serialize_private_key_pem();
    Ok((csr, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_self_signed_on_first_boot() {
        let d = tempfile::tempdir().unwrap();
        let cert = d.path().join("cert.pem");
        let key = d.path().join("key.pem");
        ensure_self_signed(&cert, &key, Some("arches-01")).unwrap();
        assert!(cert.exists());
        assert!(key.exists());
        // Second call is a no-op.
        let before = std::fs::metadata(&cert).unwrap().modified().unwrap();
        ensure_self_signed(&cert, &key, Some("arches-01")).unwrap();
        let after = std::fs::metadata(&cert).unwrap().modified().unwrap();
        assert_eq!(before, after, "existing cert must not be overwritten");
    }

    #[test]
    fn builds_server_config_from_pem() {
        let d = tempfile::tempdir().unwrap();
        let cert = d.path().join("cert.pem");
        let key = d.path().join("key.pem");
        ensure_self_signed(&cert, &key, None).unwrap();
        let cfg = server_config(&cert, &key).unwrap();
        let _ = acceptor(&cert, &key).unwrap();
        // ServerConfig is debug-printable enough to ensure it built.
        drop(cfg);
    }

    #[test]
    fn csr_generation_returns_pem_pair() {
        let (csr, key) = generate_csr("arches-01").unwrap();
        assert!(csr.contains("BEGIN CERTIFICATE REQUEST"));
        assert!(key.contains("BEGIN PRIVATE KEY"));
    }
}