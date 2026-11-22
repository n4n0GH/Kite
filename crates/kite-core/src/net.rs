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
use std::sync::{Arc, OnceLock, RwLock};
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

/// How requests leave this machine ("privacy.proxy"): the pages', which Chromium is
/// told by its switches, and Kite's own (filter lists, search suggestions, add-ons,
/// what extensions fetch through Kite, a certificate looked up, yt-dlp). One answer for
/// all of them: with a proxy set for pages only, the rest went out directly, with the
/// address the proxy was there to keep to itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// As the desktop's settings or the environment say.
    System,
    Direct,
    /// Through the proxy the user named. SOCKS5 also looks names up through it.
    Proxy { socks: bool, host: String, port: u16 },
    /// A proxy was asked for, and what is set can't be one. Nothing goes out: going
    /// out directly instead would be the opposite of what was asked.
    Broken,
}

/// Where nothing listens: a proxy that takes no connection, for [`Route::Broken`].
const NOWHERE: (&str, u16) = ("127.0.0.1", 1);

/// A host name, an IPv4 address or a bracketed IPv6 one, and nothing else:
/// Chromium's switches are joined with spaces into QTWEBENGINE_CHROMIUM_FLAGS, so a
/// space here (say, from an imported settings file) would add switches of its
/// own, such as --no-sandbox.
pub fn valid_proxy_host(host: &str) -> bool {
    !host.is_empty() && host.len() <= 253 && !host.starts_with('-') && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-_:[]".contains(c))
}

impl Route {
    /// From the settings: "privacy.proxy" ("system" | "none" | "manual"),
    /// "privacy.proxyType" ("socks5" | "http"), "privacy.proxyHost", "privacy.proxyPort".
    pub fn from_settings(mode: &str, kind: &str, host: &str, port: i64) -> Route {
        let host = host.trim();
        match mode {
            "none" => Route::Direct,
            "manual" if valid_proxy_host(host) && (1..=65535).contains(&port) => Route::Proxy { socks: kind != "http", host: host.to_owned(), port: port as u16 },
            "manual" => Route::Broken,
            _ => Route::System,
        }
    }

    fn proxy(&self) -> Option<(bool, &str, u16)> {
        match self {
            Route::Proxy { socks, host, port } => Some((*socks, host.as_str(), *port)),
            Route::Broken => Some((true, NOWHERE.0, NOWHERE.1)),
            _ => None,
        }
    }

    /// Chromium's switches for it.
    pub fn chromium_flags(&self) -> Vec<String> {
        match (self, self.proxy()) {
            (Route::Direct, _) => vec!["--no-proxy-server".into()],
            (_, Some((socks, host, port))) => vec![format!("--proxy-server={}://{host}:{port}", if socks { "socks5" } else { "http" })],
            _ => vec![],
        }
    }

    /// The proxy's address for Kite's own client and for programs that take one
    /// (yt-dlp's --proxy): "socks5h://host:port" (names looked up by the proxy, as
    /// Chromium does with SOCKS5) or "http://host:port". None for the system's or no proxy.
    pub fn proxy_address(&self) -> Option<String> {
        self.proxy().map(|(socks, host, port)| format!("{}://{host}:{port}", if socks { "socks5h" } else { "http" }))
    }

    /// For the certificate lookup in C++: "system", "none", or "<socks5|http> <host> <port>".
    pub fn spelled_out(&self) -> String {
        match (self, self.proxy()) {
            (Route::Direct, _) => "none".into(),
            (_, Some((socks, host, port))) => format!("{} {} {port}", if socks { "socks5" } else { "http" }, host.trim_matches(['[', ']'])),
            _ => "system".into(),
        }
    }
}

static ROUTE: RwLock<Route> = RwLock::new(Route::System);

/// Set as Kite starts, from the settings pages are held to as well (a change of
/// those takes a restart, for Chromium's sake).
pub fn set_route(route: Route) {
    *ROUTE.write().unwrap_or_else(|e| e.into_inner()) = route;
}

pub fn route() -> Route {
    ROUTE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The start of an agent's settings, with [`tls`] in them, the [`route`] requests take,
/// and an end to waiting: for requests that need more (a shorter timeout, statuses that
/// aren't errors).
///
/// The times are for a server that stops answering, which would else hold whatever
/// asked for good (a list's update, and with it a child's profile waiting for its lists):
/// half a minute to connect, a minute for the answer to begin, a quarter of an hour in all.
pub fn config() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    use std::time::Duration;
    let config = ureq::Agent::config_builder()
        .tls_config(tls())
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_global(Some(Duration::from_secs(15 * 60)));
    let route = route();
    match (&route, route.proxy_address()) {
        // (From the environment, as the library does by itself.)
        (Route::System, _) => config,
        (Route::Direct, _) => config.proxy(None),
        // (One that can't be made is no reason to go without: nowhere it is then.)
        (_, Some(address)) => config.proxy(Some(ureq::Proxy::new(&address).or_else(|_| ureq::Proxy::new(&format!("socks5h://{}:{}", NOWHERE.0, NOWHERE.1))).expect("a proxy address"))),
        _ => config,
    }
}

/// An agent for a plain request.
pub fn agent() -> ureq::Agent {
    config().build().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_take_the_route_the_settings_name() {
        let route = |mode, kind, host, port| Route::from_settings(mode, kind, host, port);
        assert_eq!(route("system", "socks5", "", 1080), Route::System);
        assert_eq!(route("none", "socks5", "proxy.lan", 1080), Route::Direct);
        let socks = route("manual", "socks5", " 127.0.0.1 ", 9050);
        assert_eq!(socks, Route::Proxy { socks: true, host: "127.0.0.1".into(), port: 9050 });
        assert_eq!(socks.chromium_flags(), ["--proxy-server=socks5://127.0.0.1:9050"]);
        // (Names are looked up by the proxy, for Kite's own requests as for pages.)
        assert_eq!(socks.proxy_address().as_deref(), Some("socks5h://127.0.0.1:9050"));
        assert_eq!(socks.spelled_out(), "socks5 127.0.0.1 9050");
        let http = route("manual", "http", "[::1]", 3128);
        assert_eq!(http.chromium_flags(), ["--proxy-server=http://[::1]:3128"]);
        assert_eq!(http.proxy_address().as_deref(), Some("http://[::1]:3128"));
        assert_eq!(http.spelled_out(), "http ::1 3128");
        assert_eq!(Route::Direct.chromium_flags(), ["--no-proxy-server"]);
        assert_eq!((Route::Direct.proxy_address(), Route::Direct.spelled_out()), (None, "none".into()));
        assert_eq!((Route::System.chromium_flags().len(), Route::System.proxy_address(), Route::System.spelled_out()), (0, None, "system".into()));
    }

    #[test]
    fn a_proxy_that_cant_be_one_is_no_way_out() {
        // Asked for, and not usable: nothing goes out directly instead. And nothing of
        // what was typed becomes a switch of Chromium's.
        for (host, port) in [("", 1080), ("proxy.lan", 0), ("proxy.lan", 70000), ("1.2.3.4 --no-sandbox", 8080), ("a\t--remote-debugging-port=9222", 8080), ("--no-sandbox", 8080), ("a;b", 8080), ("a/b", 8080)] {
            let route = Route::from_settings("manual", "http", host, port);
            assert_eq!(route, Route::Broken, "{host:?} {port}");
            assert_eq!(route.chromium_flags(), ["--proxy-server=socks5://127.0.0.1:1"]);
            assert_eq!(route.proxy_address().as_deref(), Some("socks5h://127.0.0.1:1"));
        }
        // The addresses made are ones the client takes.
        for route in [Route::Broken, Route::Proxy { socks: true, host: "127.0.0.1".into(), port: 9050 }, Route::Proxy { socks: false, host: "[::1]".into(), port: 3128 }] {
            assert!(ureq::Proxy::new(&route.proxy_address().unwrap()).is_ok(), "{route:?}");
        }
    }

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
