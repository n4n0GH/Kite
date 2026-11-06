//! The `kite:certificate` page: a site's certificate chain, one tab per
//! certificate. Everything in a certificate is written by whoever made it, so
//! all of it is escaped, and the page allows no scripts.

use serde::Deserialize;

/// One certificate, as `kite_cpp`'s lookup describes it.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cert {
    /// (attribute, value) pairs, like ("CN", "example.com").
    pub subject: Vec<(String, String)>,
    pub issuer: Vec<(String, String)>,
    pub not_before: String,
    pub not_after: String,
    pub expired: bool,
    pub not_yet_valid: bool,
    pub self_signed: bool,
    pub dns: Vec<String>,
    pub ips: Vec<String>,
    pub emails: Vec<String>,
    pub key_algorithm: String,
    pub key_bits: i64,
    pub signature_algorithm: String,
    pub version: String,
    pub serial: String,
    pub sha256: String,
    pub sha1: String,
    pub pem: String,
}

impl Cert {
    fn attr(&self, name: &str) -> Option<&str> {
        self.subject.iter().find(|(k, v)| k == name && !v.is_empty()).map(|(_, v)| v.as_str())
    }
    /// What its tab is called.
    fn label(&self, index: usize) -> String {
        self.attr("CN")
            .or_else(|| self.attr("O"))
            .or_else(|| self.dns.first().map(String::as_str))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Certificate {}", index + 1))
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn attr_name(attr: &str) -> &str {
    match attr {
        "CN" => "Common name",
        "O" => "Organization",
        "OU" => "Organizational unit",
        "C" => "Country",
        "L" => "Locality",
        "ST" => "State or province",
        "serialNumber" => "Serial number",
        "businessCategory" => "Business category",
        other => other,
    }
}

fn row(name: &str, value: &str) -> String {
    format!("<tr><th>{}</th><td>{}</td></tr>", esc(name), esc(value))
}

fn section(title: &str, rows: &str) -> String {
    if rows.is_empty() {
        return String::new();
    }
    format!("<h2>{}</h2><table>{rows}</table>", esc(title))
}

fn names(pairs: &[(String, String)]) -> String {
    pairs.iter().map(|(k, v)| row(attr_name(k), v)).collect()
}

/// `2026-11-06T12:00:00Z` as `2026-11-06 12:00:00 UTC`.
fn date(iso: &str) -> String {
    iso.strip_suffix('Z').map(|d| format!("{} UTC", d.replace('T', " "))).unwrap_or_else(|| iso.to_owned())
}

fn cert_body(c: &Cert) -> String {
    let mut out = String::new();
    if c.expired {
        out.push_str("<p class=warn>This certificate has expired.</p>");
    } else if c.not_yet_valid {
        out.push_str("<p class=warn>This certificate isn't valid yet.</p>");
    }
    out.push_str(&section("Subject", &names(&c.subject)));
    let issuer = if c.self_signed { format!("{}{}", names(&c.issuer), row("Self-signed", "Yes")) } else { names(&c.issuer) };
    out.push_str(&section("Issuer", &issuer));
    out.push_str(&section("Validity", &format!("{}{}", row("Not before", &date(&c.not_before)), row("Not after", &date(&c.not_after)))));
    let alt: String = c
        .dns
        .iter()
        .map(|d| row("DNS name", d))
        .chain(c.ips.iter().map(|i| row("IP address", i)))
        .chain(c.emails.iter().map(|e| row("Email address", e)))
        .collect();
    out.push_str(&section("Subject alternative names", &alt));
    let mut key = row("Algorithm", &c.key_algorithm);
    if c.key_bits > 0 {
        key.push_str(&row("Key size", &format!("{} bits", c.key_bits)));
    }
    out.push_str(&section("Public key", &key));
    let mut misc = row("Serial number", &c.serial);
    if !c.signature_algorithm.is_empty() {
        misc.push_str(&row("Signature algorithm", &c.signature_algorithm));
    }
    misc.push_str(&row("Version", &c.version));
    out.push_str(&section("Miscellaneous", &misc));
    out.push_str(&section("Fingerprints", &format!("{}{}", row("SHA-256", &c.sha256), row("SHA-1", &c.sha1))));
    if !c.pem.is_empty() {
        out.push_str(&format!("<details><summary>PEM</summary><pre>{}</pre></details>", esc(&c.pem)));
    }
    out
}

const STYLE: &str = r#"
body { font: 14px/1.5 system-ui, sans-serif; max-width: 58em; margin: 0 auto; padding: 1.5em; }
h1 { font-size: 1.5em; font-weight: 600; margin: 0 0 .2em; }
h2 { font-size: 1em; font-weight: 600; margin: 1.6em 0 .4em; }
.note { opacity: .7; margin: 0 0 1.4em; }
.warn { color: #da4453; font-weight: 600; }
.tabs { display: flex; flex-wrap: wrap; gap: .25em; border-bottom: 1px solid #8888; }
input[type=radio] { position: absolute; opacity: 0; }
label { padding: .45em 1em; cursor: pointer; border: 1px solid transparent; border-bottom: 0; border-radius: 4px 4px 0 0; }
label:hover { background: #8882; }
.cert { display: none; }
table { border-collapse: collapse; width: 100%; }
th { font-weight: normal; opacity: .7; text-align: left; vertical-align: top; width: 13em; padding: .15em 1em .15em 0; }
td { overflow-wrap: anywhere; padding: .15em 0; font-variant-numeric: tabular-nums; }
summary { margin-top: 1.6em; font-weight: 600; cursor: pointer; }
pre { font-size: 12px; white-space: pre-wrap; overflow-wrap: anywhere; user-select: all; }
"#;

/// The whole page. `error` (when there's no chain to show) says why.
pub fn page(host: &str, chain: &[Cert], error: &str) -> String {
    let shown = esc(host);
    let title = if host.is_empty() { "Certificate".to_owned() } else { format!("Certificate for {shown}") };
    let head = format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>{title}</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
<meta name="color-scheme" content="light dark"><style>{STYLE}"#
    );
    if chain.is_empty() {
        let why = if error.is_empty() { "The site sent no certificate." } else { error };
        return format!("{head}</style></head><body><h1>{title}</h1><p class=warn>{}</p></body></html>", esc(why));
    }
    // Radio buttons and CSS for the tabs: this page runs no scripts.
    let mut css = String::new();
    let mut tabs = String::new();
    let mut bodies = String::new();
    for (i, c) in chain.iter().enumerate() {
        css.push_str(&format!("#t{i}:checked ~ #c{i} {{ display: block; }}\n#t{i}:checked ~ .tabs label[for=t{i}] {{ background: #3daee92e; border-color: #8888; font-weight: 600; }}\n#t{i}:focus-visible ~ .tabs label[for=t{i}] {{ outline: 2px solid #3daee9; }}\n"));
        tabs.push_str(&format!("<label for=t{i}>{}</label>", esc(&c.label(i))));
        bodies.push_str(&format!("<div class=cert id=c{i}>{}</div>", cert_body(c)));
    }
    let radios: String = (0..chain.len()).map(|i| format!("<input type=radio name=cert id=t{i}{}>", if i == 0 { " checked" } else { "" })).collect();
    format!(
        "{head}{css}</style></head><body><h1>{title}</h1>\
<p class=note>As the site sends it to a new connection, fetched just now. A site with several servers could have given the open page a different one.</p>\
{radios}<div class=tabs>{tabs}</div>{bodies}</body></html>"
    )
}

/// `page`, from the lookup's JSON.
pub fn page_from_json(host: &str, chain_json: &str, error: &str) -> String {
    let chain: Vec<Cert> = serde_json::from_str(chain_json).unwrap_or_default();
    page(host, &chain, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert() -> Cert {
        Cert {
            subject: vec![("CN".into(), "example.com".into()), ("O".into(), "Example".into())],
            issuer: vec![("CN".into(), "Some CA".into())],
            not_before: "2026-01-01T00:00:00Z".into(),
            not_after: "2027-01-01T00:00:00Z".into(),
            dns: vec!["example.com".into(), "www.example.com".into()],
            sha256: "AA:BB".into(),
            ..Default::default()
        }
    }

    #[test]
    fn shows_the_chain_with_a_tab_each() {
        let mut ca = cert();
        ca.subject = vec![("O".into(), "Root Org".into())];
        let html = page("example.com", &[cert(), ca], "");
        assert!(html.contains("<title>Certificate for example.com</title>"));
        assert!(html.contains("<label for=t0>example.com</label>"));
        assert!(html.contains("<label for=t1>Root Org</label>"));
        assert!(html.contains("www.example.com"));
        assert!(html.contains("2027-01-01 00:00:00 UTC"));
        assert!(html.contains("id=t0 checked"));
        assert!(!html.contains("id=t1 checked"));
    }

    #[test]
    fn what_a_certificate_says_cant_become_markup() {
        let evil = "<script>alert(1)</script><img src=x onerror=alert(1)>";
        let mut c = cert();
        c.subject = vec![("CN".into(), evil.into())];
        c.issuer = vec![(evil.into(), evil.into())];
        c.dns = vec![evil.into()];
        c.serial = evil.into();
        c.pem = evil.into();
        let html = page(evil, &[c], "");
        assert!(!html.contains("<script"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("default-src 'none'"));
        // A quote can't leave an attribute or the title.
        assert!(!page("a\"b'c", &[], "x\"y").contains("a\"b"));
    }

    #[test]
    fn says_why_when_theres_nothing_to_show() {
        let html = page("example.com", &[], "Connection refused");
        assert!(html.contains("Connection refused"));
        assert!(!html.contains("<label"));
        assert!(page("example.com", &[], "").contains("no certificate"));
    }

    #[test]
    fn flags_a_certificate_out_of_date() {
        let mut c = cert();
        assert!(!page("x", std::slice::from_ref(&c), "").contains("has expired"));
        c.expired = true;
        assert!(page("x", std::slice::from_ref(&c), "").contains("has expired"));
    }

    #[test]
    fn reads_the_lookups_json() {
        let json = r#"[{"subject":[["CN","a.test"]],"notBefore":"2026-01-01T00:00:00Z","keyBits":256,"selfSigned":true,"unknownField":1}]"#;
        let html = page_from_json("a.test", json, "");
        assert!(html.contains("<label for=t0>a.test</label>"));
        assert!(html.contains("256 bits"));
        assert!(page_from_json("a.test", "not json", "broken").contains("broken"));
    }
}
