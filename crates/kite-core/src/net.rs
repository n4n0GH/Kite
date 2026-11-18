//! Kite's own HTTPS requests: filter lists, add-on downloads, Safe Browsing, and what
//! extensions fetch through Kite. This says whose certificates they accept.
//!
//! The system's certificate authorities, so that one the user added there (a self-hosted
//! server's own, a company proxy's) counts for these requests as it does for a page in a
//! tab. The HTTP library's default is a list built into it, which knows none of those:
//! an extension couldn't reach a server that the browser around it opened without complaint.
//! That list is still what's used where no system store is found.

use base64::Engine;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use ureq::tls::{Certificate, RootCerts, TlsConfig};

/// The certificate authorities to trust, read once.
fn roots() -> RootCerts {
    static ROOTS: OnceLock<Option<Arc<Vec<Certificate<'static>>>>> = OnceLock::new();
    match ROOTS.get_or_init(|| {
        let certs = system_certs();
        if certs.is_empty() {
            log::warn!("no system certificate authorities found; Kite's own requests use the built-in list");
            return None;
        }
        log::debug!("{} system certificate authorities for Kite's own requests", certs.len());
        Some(Arc::new(certs.iter().map(|der| Certificate::from_der(der).to_owned()).collect()))
    }) {
        Some(certs) => RootCerts::Specific(certs.clone()),
        None => RootCerts::WebPki,
    }
}

fn system_certs() -> Vec<Vec<u8>> {
    // In a Flatpak the files are the runtime's own list. The host's store, with what the
    // user added to it, is reached through p11-kit.
    if Path::new("/.flatpak-info").exists() {
        match p11_kit_certs() {
            Some(certs) if !certs.is_empty() => return certs,
            _ => log::debug!("the host's certificate authorities couldn't be read through p11-kit; using the runtime's"),
        }
    }
    let found = rustls_native_certs::load_native_certs();
    for e in &found.errors {
        log::debug!("reading system certificate authorities: {e}");
    }
    found.certs.into_iter().map(|c| c.to_vec()).collect()
}

/// The authorities p11-kit knows for servers (`trust extract`), as DER.
fn p11_kit_certs() -> Option<Vec<Vec<u8>>> {
    let dir = tempfile::tempdir().ok()?;
    let file = dir.path().join("roots.pem");
    let done = std::process::Command::new("trust")
        .args(["extract", "--format=pem-bundle", "--filter=ca-anchors", "--purpose=server-auth", "--overwrite"])
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !done.status.success() {
        return None;
    }
    Some(parse_pem(&std::fs::read_to_string(&file).ok()?))
}

/// The certificates of a PEM bundle, as DER. What can't be read is left out.
fn parse_pem(text: &str) -> Vec<Vec<u8>> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut out = vec![];
    let mut rest = text;
    while let Some(start) = rest.find(BEGIN) {
        let body = &rest[start + BEGIN.len()..];
        let Some(end) = body.find(END) else { break };
        let b64: String = body[..end].chars().filter(|c| !c.is_whitespace()).collect();
        if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(b64) {
            if !der.is_empty() {
                out.push(der);
            }
        }
        rest = &body[end + END.len()..];
    }
    out
}

/// TLS settings for Kite's own requests.
pub fn tls() -> TlsConfig {
    TlsConfig::builder().root_certs(roots()).build()
}

/// The start of an agent's settings, with [`tls`] in them: for requests that need more
/// (a timeout, statuses that aren't errors).
pub fn config() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    ureq::Agent::config_builder().tls_config(tls())
}

/// An agent for a plain request.
pub fn agent() -> ureq::Agent {
    config().build().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_bundles() {
        let one = base64::engine::general_purpose::STANDARD.encode(b"first certificate, long enough to wrap over two lines of base64 text");
        let (a, b) = one.split_at(64);
        let text = format!("# a comment\n-----BEGIN CERTIFICATE-----\n{a}\n{b}\n-----END CERTIFICATE-----\nnoise\n-----BEGIN CERTIFICATE-----\nAAEC\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nnot base64!\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nAAEC");
        let certs = parse_pem(&text);
        assert_eq!(certs.len(), 2);
        assert_eq!(certs[0], b"first certificate, long enough to wrap over two lines of base64 text");
        assert_eq!(certs[1], [0, 1, 2]);
        assert!(parse_pem("").is_empty());
    }

    #[test]
    fn an_agent_is_made_either_way() {
        // (Whatever this machine's store holds: the settings are built without a panic.)
        let _ = agent();
        assert!(matches!(tls().root_certs(), RootCerts::Specific(_) | RootCerts::WebPki));
    }
}
