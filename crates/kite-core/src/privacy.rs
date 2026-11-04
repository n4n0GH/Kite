//! Privacy helpers for the request interceptor: tracking-parameter stripping
//! ("privacy.stripTrackingParams") and HTTPS-only upgrades
//! ("privacy.httpsOnly").

/// Query parameters that only carry click or campaign identifiers (the list
/// Firefox's query stripping and Brave use, plus all `utm_*`).
const TRACKING_PARAMS: &[&str] = &[
    "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid", "msclkid", "yclid", "ttclid", "twclid", "li_fat_id",
    "mc_cid", "mc_eid", "_hsenc", "_hsmi", "__hssc", "__hstc", "__hsfp", "hsctatracking", "mkt_tok", "igshid",
    "igsh", "oly_anon_id", "oly_enc_id", "vero_id", "vero_conv", "_openstat", "rb_clickid", "s_cid", "ss_email_id",
    "bsft_clkid", "bsft_uid", "wickedid", "epik", "ref_src", "ref_url", "ml_subscriber", "ml_subscriber_hash",
];

fn is_tracking_param(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("utm_") || TRACKING_PARAMS.contains(&name.as_str())
}

/// `url` without tracking parameters, or `None` if it has none.
pub fn strip_tracking_params(url: &str) -> Option<String> {
    let mut u = url::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let query = u.query()?.to_owned();
    // Keep the other parameters exactly as they were written.
    let parts: Vec<&str> = query.split('&').collect();
    let kept: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|p| {
            let name = p.split('=').next().unwrap_or("");
            let name = percent_encoding::percent_decode_str(name).decode_utf8_lossy();
            !is_tracking_param(&name)
        })
        .collect();
    if kept.len() == parts.len() {
        return None;
    }
    let joined = kept.join("&");
    u.set_query(if kept.is_empty() { None } else { Some(&joined) });
    Some(u.into())
}

/// Hosts HTTPS-only leaves alone: local names and addresses, which rarely
/// have certificates.
pub fn is_local_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") || host.ends_with(".lan")
        || host.ends_with(".internal") || host.ends_with(".home.arpa") || !host.contains('.') && !host.contains(':')
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80,
        Err(_) => false,
    }
}

/// The `https:` version of an `http:` page URL to upgrade, if any.
pub fn https_upgrade(url: &str) -> Option<String> {
    let mut u = url::Url::parse(url).ok()?;
    if u.scheme() != "http" || is_local_host(u.host_str()?) {
        return None;
    }
    u.set_scheme("https").ok()?;
    // An explicit port 80 doesn't speak TLS.
    if u.port() == Some(80) {
        u.set_port(None).ok()?;
    }
    Some(u.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_params() {
        assert_eq!(strip_tracking_params("https://a.org/p?utm_source=x&id=5&fbclid=abc").as_deref(), Some("https://a.org/p?id=5"));
        assert_eq!(strip_tracking_params("https://a.org/p?UTM_Medium=x").as_deref(), Some("https://a.org/p"));
        assert_eq!(strip_tracking_params("https://a.org/p?q=a%26b&gclid=1#frag").as_deref(), Some("https://a.org/p?q=a%26b#frag"));
        assert_eq!(strip_tracking_params("https://a.org/p?id=5&ref=home"), None);
        assert_eq!(strip_tracking_params("https://a.org/"), None);
        assert_eq!(strip_tracking_params("moz-extension://x/p?utm_source=1"), None);
    }

    #[test]
    fn upgrades_public_http() {
        assert_eq!(https_upgrade("http://example.com/a?b").as_deref(), Some("https://example.com/a?b"));
        assert_eq!(https_upgrade("http://example.com:80/").as_deref(), Some("https://example.com/"));
        assert_eq!(https_upgrade("https://example.com/"), None);
        for local in ["http://localhost:8080/", "http://127.0.0.1/", "http://192.168.1.1/", "http://router/", "http://nas.local/", "http://[::1]/", "http://10.0.0.5:3000/"] {
            assert_eq!(https_upgrade(local), None, "{local}");
        }
    }
}
