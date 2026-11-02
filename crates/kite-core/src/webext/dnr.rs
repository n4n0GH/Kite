//! declarativeNetRequest: rule parsing and matching for the request
//! interceptor. Supports block / allow / allowAllRequests / upgradeScheme /
//! redirect and request-header `set` operations of modifyHeaders.

use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    None,
    Block,
    Redirect(String),
    /// Request headers to set.
    Headers(Vec<(String, String)>),
}

#[derive(Debug, Clone)]
enum Action {
    Block,
    Allow,
    Upgrade,
    Redirect { url: Option<String>, ext_path: Option<String>, regex_sub: Option<String>, transform: Option<Value> },
    SetHeaders(Vec<(String, String)>),
}

impl Action {
    fn rank(&self) -> u8 {
        match self {
            Action::Allow => 0,
            Action::Block => 1,
            Action::Upgrade => 2,
            Action::Redirect { .. } => 3,
            Action::SetHeaders(_) => 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: i64,
    pub priority: i64,
    action: Action,
    url_re: Option<Regex>,
    regex_filter: bool,
    resource_types: Option<Vec<String>>,
    excluded_resource_types: Vec<String>,
    request_domains: Vec<String>,
    excluded_request_domains: Vec<String>,
    initiator_domains: Vec<String>,
    excluded_initiator_domains: Vec<String>,
    methods: Vec<String>,
    excluded_methods: Vec<String>,
    domain_type: Option<String>,
}

fn strs(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect())
        .unwrap_or_default()
}

/// Compile a DNR `urlFilter` to a regex.
pub fn url_filter_regex(f: &str, case_sensitive: bool) -> Result<Regex, regex::Error> {
    let mut re = String::new();
    if !case_sensitive {
        re.push_str("(?i)");
    }
    let mut s = f;
    if let Some(rest) = s.strip_prefix("||") {
        re.push_str(r"^[a-z][a-z0-9+.-]*://([^/?#]*\.)?");
        s = rest;
    } else if let Some(rest) = s.strip_prefix('|') {
        re.push('^');
        s = rest;
    }
    let (body, end_anchor) = match s.strip_suffix('|') {
        Some(b) => (b, true),
        None => (s, false),
    };
    for c in body.chars() {
        match c {
            '*' => re.push_str(".*"),
            '^' => re.push_str(r"([^a-zA-Z0-9_.%-]|$)"),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    if end_anchor {
        re.push('$');
    }
    Regex::new(&re)
}

fn domain_match(host: &str, list: &[String]) -> bool {
    list.iter().any(|d| host == d || host.ends_with(&format!(".{d}")))
}

/// Qt resource type names to DNR resource types.
pub fn resource_type(qt: &str) -> &'static str {
    match qt {
        "mainFrame" => "main_frame",
        "subFrame" => "sub_frame",
        "stylesheet" => "stylesheet",
        "script" | "worker" | "sharedWorker" | "serviceWorker" => "script",
        "image" | "favicon" => "image",
        "fontResource" => "font",
        "object" | "pluginResource" => "object",
        "xhr" => "xmlhttprequest",
        "ping" => "ping",
        "cspReport" => "csp_report",
        "media" => "media",
        "websocket" => "websocket",
        _ => "other",
    }
}

impl Rule {
    pub fn parse(v: &Value) -> Result<Rule, String> {
        let id = v.get("id").and_then(Value::as_i64).ok_or("rule without id")?;
        let a = v.get("action").ok_or("rule without action")?;
        let action = match a.get("type").and_then(Value::as_str) {
            Some("block") => Action::Block,
            Some("allow") | Some("allowAllRequests") => Action::Allow,
            Some("upgradeScheme") => Action::Upgrade,
            Some("redirect") => {
                let r = a.get("redirect").ok_or("redirect without target")?;
                Action::Redirect {
                    url: r.get("url").and_then(Value::as_str).map(str::to_owned),
                    ext_path: r.get("extensionPath").and_then(Value::as_str).map(str::to_owned),
                    regex_sub: r.get("regexSubstitution").and_then(Value::as_str).map(str::to_owned),
                    transform: r.get("transform").cloned(),
                }
            }
            Some("modifyHeaders") => Action::SetHeaders(
                a.get("requestHeaders")
                    .and_then(Value::as_array)
                    .map(|h| {
                        h.iter()
                            .filter(|x| x.get("operation").and_then(Value::as_str) == Some("set"))
                            .filter_map(|x| Some((x.get("header")?.as_str()?.to_owned(), x.get("value")?.as_str()?.to_owned())))
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            other => return Err(format!("unsupported action {other:?}")),
        };
        let c = v.get("condition").cloned().unwrap_or(Value::Object(Default::default()));
        let cs = c.get("isUrlFilterCaseSensitive").and_then(Value::as_bool).unwrap_or(false);
        let (url_re, regex_filter) = if let Some(rf) = c.get("regexFilter").and_then(Value::as_str) {
            let src = if cs { rf.to_owned() } else { format!("(?i){rf}") };
            (Some(Regex::new(&src).map_err(|e| e.to_string())?), true)
        } else if let Some(uf) = c.get("urlFilter").and_then(Value::as_str) {
            (Some(url_filter_regex(uf, cs).map_err(|e| e.to_string())?), false)
        } else {
            (None, false)
        };
        let pick = |a: &str, b: &str| {
            let v = strs(c.get(a));
            if v.is_empty() { strs(c.get(b)) } else { v }
        };
        Ok(Rule {
            id,
            priority: v.get("priority").and_then(Value::as_i64).unwrap_or(1),
            action,
            url_re,
            regex_filter,
            resource_types: c.get("resourceTypes").map(|_| strs(c.get("resourceTypes"))),
            excluded_resource_types: strs(c.get("excludedResourceTypes")),
            request_domains: strs(c.get("requestDomains")),
            excluded_request_domains: strs(c.get("excludedRequestDomains")),
            initiator_domains: pick("initiatorDomains", "domains"),
            excluded_initiator_domains: pick("excludedInitiatorDomains", "excludedDomains"),
            methods: strs(c.get("requestMethods")),
            excluded_methods: strs(c.get("excludedRequestMethods")),
            domain_type: c.get("domainType").and_then(Value::as_str).map(str::to_owned),
        })
    }

    fn matches(&self, req: &Request) -> bool {
        let rt = req.resource_type;
        match &self.resource_types {
            Some(t) => {
                if !t.iter().any(|x| x == rt) {
                    return false;
                }
            }
            None => {
                if rt == "main_frame" {
                    return false;
                }
            }
        }
        if self.excluded_resource_types.iter().any(|x| x == rt) {
            return false;
        }
        let m = req.method.to_lowercase();
        if (!self.methods.is_empty() && !self.methods.contains(&m)) || self.excluded_methods.contains(&m) {
            return false;
        }
        if (!self.request_domains.is_empty() && !domain_match(&req.host, &self.request_domains))
            || domain_match(&req.host, &self.excluded_request_domains)
        {
            return false;
        }
        if (!self.initiator_domains.is_empty() && !domain_match(&req.initiator_host, &self.initiator_domains))
            || domain_match(&req.initiator_host, &self.excluded_initiator_domains)
        {
            return false;
        }
        if let Some(dt) = &self.domain_type {
            let third = !req.initiator_host.is_empty() && site(&req.initiator_host) != site(&req.host);
            if (dt == "thirdParty") != third {
                return false;
            }
        }
        self.url_re.as_ref().map(|r| r.is_match(req.url)).unwrap_or(true)
    }
}

/// Rough registrable domain (last two labels) for first/third-party checks.
fn site(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    parts[parts.len().saturating_sub(2)..].join(".")
}

pub struct Request<'a> {
    pub url: &'a str,
    pub host: String,
    pub initiator_host: String,
    pub method: &'a str,
    pub resource_type: &'static str,
}

impl<'a> Request<'a> {
    pub fn new(url: &'a str, initiator: &str, method: &'a str, qt_type: &str) -> Self {
        Request {
            url,
            host: crate::util::host_of(url).to_lowercase(),
            initiator_host: crate::util::host_of(initiator).to_lowercase(),
            method,
            resource_type: resource_type(qt_type),
        }
    }
}

/// All rules of one extension.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub ext_base_url: String,
    pub static_rules: Vec<Rule>,
    pub dynamic_rules: Vec<Rule>,
    pub session_rules: Vec<Rule>,
}

impl RuleSet {
    fn best(&self, req: &Request) -> Option<&Rule> {
        self.static_rules
            .iter()
            .chain(&self.dynamic_rules)
            .chain(&self.session_rules)
            .filter(|r| r.matches(req))
            .max_by(|a, b| a.priority.cmp(&b.priority).then(b.action.rank().cmp(&a.action.rank())))
    }

    pub fn evaluate(&self, req: &Request) -> Outcome {
        let Some(rule) = self.best(req) else { return Outcome::None };
        match &rule.action {
            Action::Allow => Outcome::None,
            Action::Block => Outcome::Block,
            Action::Upgrade => {
                if req.url.starts_with("http://") {
                    Outcome::Redirect(format!("https://{}", &req.url[7..]))
                } else {
                    Outcome::None
                }
            }
            Action::SetHeaders(h) => Outcome::Headers(h.clone()),
            Action::Redirect { url, ext_path, regex_sub, transform } => {
                if let Some(u) = url {
                    return Outcome::Redirect(u.clone());
                }
                if let Some(p) = ext_path {
                    return Outcome::Redirect(format!("{}{}", self.ext_base_url, p.trim_start_matches('/')));
                }
                if let (Some(sub), true, Some(re)) = (regex_sub, rule.regex_filter, &rule.url_re) {
                    let js_style = sub.replace("\\0", "${0}");
                    let js_style = (1..10).fold(js_style, |s, i| s.replace(&format!("\\{i}"), &format!("${{{i}}}")));
                    return Outcome::Redirect(re.replace(req.url, js_style.as_str()).into_owned());
                }
                if let Some(t) = transform {
                    if let Ok(mut u) = url::Url::parse(req.url) {
                        if let Some(s) = t.get("scheme").and_then(Value::as_str) {
                            let _ = u.set_scheme(s);
                        }
                        if let Some(h) = t.get("host").and_then(Value::as_str) {
                            let _ = u.set_host(Some(h));
                        }
                        if let Some(p) = t.get("path").and_then(Value::as_str) {
                            u.set_path(p);
                        }
                        if let Some(q) = t.get("query").and_then(Value::as_str) {
                            u.set_query(Some(q.trim_start_matches('?')).filter(|q| !q.is_empty()));
                        }
                        if let Some(f) = t.get("fragment").and_then(Value::as_str) {
                            u.set_fragment(Some(f.trim_start_matches('#')).filter(|f| !f.is_empty()));
                        }
                        return Outcome::Redirect(u.to_string());
                    }
                }
                Outcome::None
            }
        }
    }
}

pub fn parse_rules(v: &Value) -> (Vec<Rule>, Vec<String>) {
    let mut rules = vec![];
    let mut errors = vec![];
    if let Some(a) = v.as_array() {
        for r in a {
            match Rule::parse(r) {
                Ok(r) => rules.push(r),
                Err(e) => errors.push(e),
            }
        }
    }
    (rules, errors)
}

/// Combine outcomes of several extensions: block wins, then redirect, then headers.
pub fn combine(outcomes: impl IntoIterator<Item = Outcome>) -> Outcome {
    let mut best = Outcome::None;
    for o in outcomes {
        best = match (&best, &o) {
            (Outcome::Block, _) => break,
            (_, Outcome::Block) => o,
            (Outcome::Redirect(_), _) => best,
            (_, Outcome::Redirect(_)) => o,
            (Outcome::None, Outcome::Headers(_)) => o,
            _ => best,
        };
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rules() {
        let (rules, errs) = parse_rules(&json!([
            {"id": 1, "action": {"type": "block"}, "condition": {"urlFilter": "||tracker.test^", "resourceTypes": ["script", "image"]}},
            {"id": 2, "priority": 2, "action": {"type": "allow"}, "condition": {"urlFilter": "||tracker.test/ok"}},
            {"id": 3, "action": {"type": "redirect", "redirect": {"extensionPath": "/empty.js"}}, "condition": {"urlFilter": "|https://cdn.test/lib.js|"}},
            {"id": 4, "action": {"type": "upgradeScheme"}, "condition": {"requestDomains": ["plain.test"], "resourceTypes": ["main_frame"]}},
            {"id": 5, "action": {"type": "redirect", "redirect": {"regexSubstitution": "https://new.test/\\1"}}, "condition": {"regexFilter": "^https://old\\.test/(.*)$"}},
            {"id": 6, "action": {"type": "nope"}}
        ]));
        assert_eq!(errs.len(), 1);
        let rs = RuleSet { ext_base_url: "moz-extension://u/".into(), static_rules: rules, ..Default::default() };
        let ev = |u: &str, t: &str| rs.evaluate(&Request::new(u, "https://site.test/", "GET", t));
        assert_eq!(ev("https://a.tracker.test/x.js", "script"), Outcome::Block);
        assert_eq!(ev("https://tracker.test.evil/x.js", "script"), Outcome::None);
        assert_eq!(ev("https://tracker.test/x.css", "stylesheet"), Outcome::None);
        assert_eq!(ev("https://tracker.test/ok/x.js", "script"), Outcome::None);
        assert_eq!(ev("https://cdn.test/lib.js", "script"), Outcome::Redirect("moz-extension://u/empty.js".into()));
        assert_eq!(ev("http://plain.test/a", "mainFrame"), Outcome::Redirect("https://plain.test/a".into()));
        assert_eq!(ev("https://old.test/p?q", "xhr"), Outcome::Redirect("https://new.test/p?q".into()));
        assert_eq!(combine([Outcome::Redirect("x".into()), Outcome::Block]), Outcome::Block);
    }
}
