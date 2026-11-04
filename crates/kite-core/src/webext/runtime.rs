//! WebExtension API backend.
//!
//! The runtime is a synchronous state machine: the GUI layer feeds it API
//! calls from extension contexts ([`Runtime::call`]), replies to UI requests
//! ([`Runtime::ui_reply`]) and browser events ([`Runtime::browser_event`]),
//! and executes the returned [`Effect`]s (reply to a page, deliver events,
//! ask the UI to open a tab, refresh toolbar buttons, ...).
//!
//! Identity: every extension gets a random per-session token which is only
//! embedded in scripts running with that extension's privileges (its pages and
//! its content-script world). Calls carry the token, so a web page or another
//! extension cannot impersonate it.

use super::blocking::BlockingHub;
use super::dnr::{self, RuleSet};
use super::matchpattern::MatchSet;
use super::registry::{Extension, Registry};
use super::storage::Storage;
use crate::shared::Shared;
use crate::util::now_ms;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Where a call came from (filled in by the GUI layer, not by page JS).
#[derive(Debug, Clone, Default)]
pub struct Origin {
    /// Identifies the WebEngineView ("tab:12", "bg:<ext>", "popup:<ext>", ...).
    pub view: String,
    /// Tab id if the view is a browser tab, else -1.
    pub tab_id: i64,
    pub window_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Resolve/reject a pending API call in a page.
    Reply { view: String, ctx: String, call_id: i64, result: Result<Value, String> },
    /// Deliver an event/message payload to contexts living in `view`.
    Deliver { view: String, ctxs: Vec<String>, payload: Value },
    /// Ask the GUI to perform `method`; it must answer via `Runtime::ui_reply`.
    Ui { view: String, ctx: String, call_id: i64, ext: String, method: String, args: Value },
    /// Some UI-visible state changed: "actions", "menus", "scripts",
    /// "extensions", "commands", "background", "bookmarks", "history".
    Changed(String),
}

#[derive(Debug, Clone)]
struct Context {
    ext: String,
    /// "background" | "popup" | "options" | "sidebar" | "tab" | "content"
    kind: String,
    view: String,
    tab_id: i64,
    window_id: i64,
    frame_id: i64,
    url: String,
    /// Events of `LISTENED_EVENTS` this context has listeners for.
    listens: std::collections::HashSet<String>,
}

/// Events only delivered to contexts that listen for them: their payloads
/// can be large (storage changes carry old and new values, megabytes for
/// uBlock Origin's caches), and every delivery has to cross into the view.
const LISTENED_EVENTS: [&str; 5] = ["storage.onChanged", "storage.local.onChanged", "storage.sync.onChanged", "storage.session.onChanged", "storage.managed.onChanged"];

struct PendingMsg {
    reply_view: String,
    reply_ctx: String,
    call_id: i64,
    remaining: usize,
    targets: Vec<String>,
}

struct Port {
    opener: String,
    others: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ActionState {
    pub title: Option<String>,
    pub popup: Option<String>,
    pub badge_text: Option<String>,
    pub badge_bg: Option<String>,
    pub badge_fg: Option<String>,
    pub icon: Option<String>,
    pub enabled: Option<bool>,
    /// pageAction visibility
    pub shown: Option<bool>,
}

impl ActionState {
    fn merged(&self, over: Option<&ActionState>) -> ActionState {
        let Some(o) = over else { return self.clone() };
        ActionState {
            title: o.title.clone().or_else(|| self.title.clone()),
            popup: o.popup.clone().or_else(|| self.popup.clone()),
            badge_text: o.badge_text.clone().or_else(|| self.badge_text.clone()),
            badge_bg: o.badge_bg.clone().or_else(|| self.badge_bg.clone()),
            badge_fg: o.badge_fg.clone().or_else(|| self.badge_fg.clone()),
            icon: o.icon.clone().or_else(|| self.icon.clone()),
            enabled: o.enabled.or(self.enabled),
            shown: o.shown.or(self.shown),
        }
    }
}

#[derive(Default)]
struct Actions {
    global: ActionState,
    per_tab: HashMap<i64, ActionState>,
}

#[derive(Debug, Clone)]
struct MenuItem {
    ext: String,
    id: String,
    parent: Option<String>,
    title: String,
    kind: String,
    contexts: Vec<String>,
    checked: bool,
    enabled: bool,
    visible: bool,
    document_patterns: Option<MatchSet>,
    target_patterns: Option<MatchSet>,
    icon: Option<String>,
}

#[derive(Debug, Clone)]
struct Alarm {
    scheduled: f64,
    period: Option<f64>,
}

#[derive(Debug, Clone)]
struct WebRequestListener {
    event: String,
    urls: MatchSet,
    types: Vec<String>,
    tab_id: Option<i64>,
    /// Registered with "blocking": answered over the [`BlockingHub`]
    /// instead of being delivered as an event.
    blocking: bool,
}

pub struct Runtime {
    pub registry: Registry,
    pub storage: Storage,
    pub shared: Shared,
    tokens: HashMap<String, String>,
    ext_tokens: HashMap<String, String>,
    worlds: HashMap<String, u32>,
    next_world: u32,
    ctxs: HashMap<String, Context>,
    /// `_.listen` calls that came before the context registered.
    early_listens: HashMap<String, std::collections::HashSet<String>>,
    tabs: BTreeMap<i64, Value>,
    windows: BTreeMap<i64, Value>,
    focused_window: i64,
    msgs: HashMap<u64, PendingMsg>,
    next_msg: u64,
    ports: HashMap<String, Port>,
    actions: HashMap<String, Actions>,
    page_actions: HashMap<String, Actions>,
    sidebars: HashMap<String, Actions>,
    menus: Vec<MenuItem>,
    alarms: HashMap<(String, String), Alarm>,
    web_request: HashMap<String, Vec<WebRequestListener>>,
    pub blocking: Arc<BlockingHub>,
    pub dnr: HashMap<String, RuleSet>,
    dnr_disabled: HashMap<String, Vec<String>>,
    dynamic_scripts: HashMap<String, Vec<Value>>,
    pending_startup: HashMap<String, Value>,
    uninstall_urls: HashMap<String, String>,
    notifications: HashMap<String, Vec<String>>,
    /// Background contexts that called `_ready`.
    ready: HashMap<String, String>,
    /// Messages/connections sent before the extension's background page
    /// finished loading; replayed on `_ready` (Firefox semantics).
    waiting_for_background: HashMap<String, Vec<QueuedCall>>,
}

struct QueuedCall {
    origin: Origin,
    ctx: String,
    method: String,
    args: Vec<Value>,
    call_id: i64,
    since: std::time::Instant,
}

/// How long callers wait for a starting background page.
const BACKGROUND_STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

const NO_RECEIVER: &str = "Could not establish connection. Receiving end does not exist.";

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Null)
}
fn arg_str(args: &[Value], i: usize) -> String {
    args.get(i).and_then(Value::as_str).unwrap_or_default().to_owned()
}
fn arg_i64(args: &[Value], i: usize) -> Option<i64> {
    args.get(i).and_then(Value::as_i64)
}
fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}
fn token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

type R = Result<Value, String>;

/// Result of handling a call: either an immediate value, or effects only
/// (the reply is produced later, e.g. by the UI or a message response).
enum Handled {
    Now(R),
    Later(Vec<Effect>),
    /// Reply now and also emit effects.
    With(R, Vec<Effect>),
}

impl Runtime {
    pub fn new(shared: Shared) -> anyhow::Result<Self> {
        let dir = shared.profile.extensions_dir();
        let registry = Registry::open(dir.clone());
        let storage = Storage::open(&dir.join("storage.db"))?;
        let mut rt = Runtime {
            registry,
            storage,
            shared,
            tokens: HashMap::new(),
            ext_tokens: HashMap::new(),
            worlds: HashMap::new(),
            next_world: 10,
            ctxs: HashMap::new(),
            early_listens: HashMap::new(),
            tabs: BTreeMap::new(),
            windows: BTreeMap::new(),
            focused_window: -1,
            msgs: HashMap::new(),
            next_msg: 1,
            ports: HashMap::new(),
            actions: HashMap::new(),
            page_actions: HashMap::new(),
            sidebars: HashMap::new(),
            menus: vec![],
            alarms: HashMap::new(),
            web_request: HashMap::new(),
            blocking: Arc::new(BlockingHub::new()),
            dnr: HashMap::new(),
            dnr_disabled: HashMap::new(),
            dynamic_scripts: HashMap::new(),
            pending_startup: HashMap::new(),
            uninstall_urls: HashMap::new(),
            notifications: HashMap::new(),
            ready: HashMap::new(),
            waiting_for_background: HashMap::new(),
        };
        let ids: Vec<String> = rt.registry.extensions.iter().map(|e| e.info.id.clone()).collect();
        for id in ids {
            rt.prepare_extension(&id);
            rt.pending_startup.insert(id, json!({"event": "runtime.onStartup", "args": []}));
        }
        Ok(rt)
    }

    /// Per-session state for an (re)loaded extension.
    fn prepare_extension(&mut self, id: &str) {
        if !self.ext_tokens.contains_key(id) {
            let t = token();
            self.tokens.insert(t.clone(), id.to_owned());
            self.blocking.add_token(&t, id);
            self.ext_tokens.insert(id.to_owned(), t);
        }
        if !self.worlds.contains_key(id) {
            self.worlds.insert(id.to_owned(), self.next_world);
            self.next_world = (self.next_world + 1).min(255);
        }
        self.load_dnr(id);
    }

    fn load_dnr(&mut self, id: &str) {
        let Some(ext) = self.registry.get(id) else { return };
        let disabled = self.dnr_disabled.get(id).cloned().unwrap_or_default();
        let mut set = self.dnr.remove(id).unwrap_or_default();
        set.ext_base_url = ext.base_url();
        set.static_rules.clear();
        for (rid, enabled, path) in &ext.manifest.dnr_rulesets {
            let on = if disabled.contains(rid) { false } else { *enabled || self.dnr_disabled.get(id).is_some_and(|d| d.contains(&format!("+{rid}"))) };
            if !on {
                continue;
            }
            if let Some(text) = ext.resolve(path).and_then(|p| std::fs::read_to_string(p).ok()) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    set.static_rules.extend(dnr::parse_rules(&v).0);
                }
            }
        }
        if let Ok(v) = self.storage.get(id, "local", &json!("__kite_dnr_dynamic")) {
            if let Some(rules) = v.get("__kite_dnr_dynamic") {
                set.dynamic_rules = dnr::parse_rules(rules).0;
            }
        }
        self.dnr.insert(id.to_owned(), set);
    }

    pub fn token_of(&self, ext: &str) -> Option<&str> {
        self.ext_tokens.get(ext).map(String::as_str)
    }
    pub fn world_of(&self, ext: &str) -> u32 {
        self.worlds.get(ext).copied().unwrap_or(10)
    }
    pub fn ext_for_token(&self, token: &str) -> Option<&str> {
        self.tokens.get(token).map(String::as_str)
    }
    pub fn dynamic_scripts(&self, ext: &str) -> Vec<Value> {
        self.dynamic_scripts.get(ext).cloned().unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // Extension lifecycle (driven by the GUI's add-on manager)
    // ------------------------------------------------------------------

    pub fn on_installed(&mut self, id: &str, updated: bool, previous_version: &str, temporary: bool) -> Vec<Effect> {
        self.prepare_extension(id);
        let mut details = json!({"reason": if updated { "update" } else { "install" }, "temporary": temporary});
        if updated {
            details["previousVersion"] = previous_version.into();
        }
        self.pending_startup.insert(id.to_owned(), json!({"event": "runtime.onInstalled", "args": [details]}));
        self.drop_ext_state(id);
        vec![Effect::Changed("extensions".into()), Effect::Changed("scripts".into()), Effect::Changed("background".into()), Effect::Changed("actions".into())]
    }

    pub fn on_enabled_changed(&mut self, id: &str, enabled: bool) -> Vec<Effect> {
        if enabled {
            self.prepare_extension(id);
            self.pending_startup.insert(id.to_owned(), json!({"event": "runtime.onStartup", "args": []}));
        } else {
            self.drop_ext_state(id);
        }
        vec![Effect::Changed("extensions".into()), Effect::Changed("scripts".into()), Effect::Changed("background".into()), Effect::Changed("actions".into()), Effect::Changed("menus".into())]
    }

    /// Call before removing from the registry; returns the uninstall URL to open, if any.
    pub fn on_uninstall(&mut self, id: &str) -> Option<String> {
        self.drop_ext_state(id);
        let _ = self.storage.wipe(id);
        self.dnr.remove(id);
        self.uninstall_urls.remove(id)
    }

    fn drop_ext_state(&mut self, id: &str) {
        self.waiting_for_background.remove(id);
        self.menus.retain(|m| m.ext != id);
        self.alarms.retain(|(e, _), _| e != id);
        self.web_request.remove(id);
        self.actions.remove(id);
        self.page_actions.remove(id);
        self.sidebars.remove(id);
        self.ready.remove(id);
        self.ctxs.retain(|_, c| c.ext != id);
        if let Some(set) = self.dnr.get_mut(id) {
            set.session_rules.clear();
        }
    }

    // ------------------------------------------------------------------
    // Entry points
    // ------------------------------------------------------------------

    /// Handle an API call. `token` authenticates the extension.
    pub fn call(&mut self, origin: &Origin, token: &str, ctx: &str, method: &str, args: Value, call_id: i64) -> Vec<Effect> {
        let Some(ext_id) = self.tokens.get(token).cloned() else {
            return vec![Effect::Reply { view: origin.view.clone(), ctx: ctx.into(), call_id, result: Err("invalid extension token".into()) }];
        };
        if !self.registry.get(&ext_id).is_some_and(|e| e.info.enabled) {
            return vec![Effect::Reply { view: origin.view.clone(), ctx: ctx.into(), call_id, result: Err("extension is disabled".into()) }];
        }
        if log::log_enabled!(log::Level::Trace) {
            let a = args.to_string();
            log::trace!("call {method} from {ctx} ({}): {}", origin.view, a.chars().take(300).collect::<String>());
        }
        let args: Vec<Value> = match args {
            Value::Array(a) => a,
            Value::Null => vec![],
            v => vec![v],
        };
        let handled = self.dispatch(origin, &ext_id, ctx, method, &args, call_id);
        let reply = |result: R| Effect::Reply { view: origin.view.clone(), ctx: ctx.into(), call_id, result };
        match handled {
            Handled::Now(r) => vec![reply(r)],
            Handled::Later(e) => e,
            Handled::With(r, mut e) => {
                e.insert(0, reply(r));
                e
            }
        }
    }

    /// The GUI finished a [`Effect::Ui`] request.
    pub fn ui_reply(&mut self, view: &str, ctx: &str, call_id: i64, result: R) -> Vec<Effect> {
        if ctx.is_empty() {
            return vec![];
        }
        vec![Effect::Reply { view: view.into(), ctx: ctx.into(), call_id, result }]
    }

    /// A WebEngineView went away: drop its contexts.
    pub fn view_closed(&mut self, view: &str) -> Vec<Effect> {
        let gone: Vec<String> = self.ctxs.iter().filter(|(_, c)| c.view == view).map(|(k, _)| k.clone()).collect();
        let mut fx = vec![];
        for c in gone {
            fx.extend(self.unregister(&c));
        }
        fx
    }

    fn unregister(&mut self, ctx: &str) -> Vec<Effect> {
        self.early_listens.remove(ctx);
        let Some(c) = self.ctxs.remove(ctx) else { return vec![] };
        if self.ready.get(&c.ext).is_some_and(|r| r == ctx) {
            self.ready.remove(&c.ext);
        }
        let mut fx = vec![];
        // Pending messages waiting on this context: count as "no response".
        let ids: Vec<u64> = self.msgs.iter().filter(|(_, m)| m.targets.iter().any(|t| t == ctx)).map(|(k, _)| *k).collect();
        for id in ids {
            fx.extend(self.respond(id, ctx, Value::Null, false));
        }
        // Ports
        let port_ids: Vec<String> = self
            .ports
            .iter()
            .filter(|(_, p)| p.opener == ctx || p.others.iter().any(|o| o == ctx))
            .map(|(k, _)| k.clone())
            .collect();
        for p in port_ids {
            fx.extend(self.port_disconnect_from(&p, ctx, None));
        }
        fx
    }

    // ------------------------------------------------------------------
    // Delivery helpers
    // ------------------------------------------------------------------

    fn deliver(&self, ctxs: &[String], payload: Value) -> Vec<Effect> {
        let mut by_view: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in ctxs {
            if let Some(ctx) = self.ctxs.get(c) {
                by_view.entry(ctx.view.clone()).or_default().push(c.clone());
            }
        }
        by_view.into_iter().map(|(view, ctxs)| Effect::Deliver { view, ctxs, payload: payload.clone() }).collect()
    }

    /// Fire `name(args)` on every context of `ext` matching `filter`.
    fn fire(&self, ext: &str, name: &str, args: Value, include_content: bool) -> Vec<Effect> {
        let targets: Vec<String> = self
            .ctxs
            .iter()
            .filter(|(_, c)| c.ext == ext && (include_content || c.kind != "content"))
            .filter(|(_, c)| !LISTENED_EVENTS.contains(&name) || c.listens.contains(name))
            .map(|(k, _)| k.clone())
            .collect();
        if targets.is_empty() {
            return vec![];
        }
        self.deliver(&targets, json!({"type": "event", "name": name, "args": args}))
    }

    /// Fire an event on all enabled extensions (pages only), optionally
    /// requiring a permission.
    fn fire_all(&self, name: &str, args_for: impl Fn(&Extension) -> Option<Value>) -> Vec<Effect> {
        let mut fx = vec![];
        for e in self.registry.enabled() {
            if let Some(args) = args_for(e) {
                fx.extend(self.fire(&e.info.id, name, args, false));
            }
        }
        fx
    }

    fn sender(&self, ctx: &Context, ext: &str) -> Value {
        let mut s = json!({"id": ext, "url": ctx.url, "frameId": ctx.frame_id, "envType": if ctx.kind == "content" { "content_child" } else { "addon_child" }});
        if let Some(o) = origin_of(&ctx.url) {
            s["origin"] = o.into();
        }
        if ctx.tab_id >= 0 {
            if let Some(t) = self.tabs.get(&ctx.tab_id) {
                s["tab"] = t.clone();
            }
        }
        s
    }

    // ------------------------------------------------------------------
    // Tabs / windows mirror
    // ------------------------------------------------------------------

    fn can_see_tab_details(&self, ext: &Extension, url: &str) -> bool {
        ext.has_permission("tabs") || MatchSet::parse(ext.host_patterns().iter().map(String::as_str)).matches(url)
    }

    fn tab_for(&self, ext: &Extension, tab: &Value) -> Value {
        let mut t = tab.clone();
        let url = tab.get("url").and_then(Value::as_str).unwrap_or_default();
        if !self.can_see_tab_details(ext, url) {
            if let Some(o) = t.as_object_mut() {
                o.remove("url");
                o.remove("title");
                o.remove("favIconUrl");
            }
        }
        t
    }

    /// (tab id, window id) of the active tab in the focused window.
    pub fn active_tab(&self) -> (i64, i64) {
        self.tabs
            .values()
            .find(|t| t["active"] == true && t["windowId"].as_i64() == Some(self.focused_window))
            .or_else(|| self.tabs.values().find(|t| t["active"] == true))
            .map(|t| (t["id"].as_i64().unwrap_or(-1), t["windowId"].as_i64().unwrap_or(-1)))
            .unwrap_or((-1, self.focused_window))
    }

    /// Best guess of the tab a network request belongs to (Qt's profile-level
    /// interceptor does not tell us), by matching the first-party URL.
    /// (tab, window) showing a page of `origin` (e.g. "https://kde.org"),
    /// preferring the active tab, then the most recently used one.
    pub fn tab_for_origin(&self, origin: &str) -> (i64, i64) {
        let id = self.tab_for_first_party(origin);
        let window = self.tabs.get(&id).and_then(|t| t["windowId"].as_i64()).unwrap_or(-1);
        (id, window)
    }

    pub fn tab_for_first_party(&self, first_party: &str) -> i64 {
        let origin = |u: &str| url::Url::parse(u).map(|u| u.origin().ascii_serialization()).unwrap_or_default();
        let fp = origin(first_party);
        self.tabs
            .values()
            .filter(|t| origin(t["url"].as_str().unwrap_or("")) == fp)
            .max_by_key(|t| (t["active"] == true, t["lastAccessed"].as_i64().unwrap_or(0)))
            .and_then(|t| t["id"].as_i64())
            .unwrap_or(-1)
    }

    pub fn tab(&self, id: i64) -> Option<&Value> {
        self.tabs.get(&id)
    }

    fn query_tabs(&self, ext: &Extension, q: &Value, origin: &Origin) -> Vec<Value> {
        let q = obj(q);
        let current_window = if origin.window_id >= 0 { origin.window_id } else { self.focused_window };
        let url_set = match q.get("url") {
            Some(Value::String(s)) => Some(MatchSet::parse([s.as_str()])),
            Some(Value::Array(a)) => Some(MatchSet::parse(a.iter().filter_map(Value::as_str))),
            _ => None,
        };
        let title_re = q.get("title").and_then(Value::as_str).map(|t| regex::Regex::new(&super::matchpattern::glob_to_regex(t)).ok()).flatten();
        self.tabs
            .values()
            .filter(|t| {
                let wid = t["windowId"].as_i64().unwrap_or(-1);
                for (k, v) in &q {
                    let ok = match k.as_str() {
                        "active" | "pinned" | "audible" | "muted" | "highlighted" | "discarded" | "hidden" => {
                            let actual = if k == "muted" { t["mutedInfo"]["muted"].clone() } else if k == "highlighted" { t["active"].clone() } else { t.get(k).cloned().unwrap_or(json!(false)) };
                            actual.as_bool().unwrap_or(false) == v.as_bool().unwrap_or(false)
                        }
                        "currentWindow" => (wid == current_window) == v.as_bool().unwrap_or(true),
                        "lastFocusedWindow" => (wid == self.focused_window) == v.as_bool().unwrap_or(true),
                        "windowId" => v.as_i64().map(|w| if w == -2 { wid == current_window } else { wid == w }).unwrap_or(true),
                        "index" => t["index"] == *v,
                        "status" => t["status"] == *v,
                        "windowType" => v == "normal",
                        "url" => url_set.as_ref().map(|s| s.matches(t["url"].as_str().unwrap_or(""))).unwrap_or(true),
                        "title" => title_re.as_ref().map(|r| r.is_match(t["title"].as_str().unwrap_or(""))).unwrap_or(true),
                        "incognito" => !v.as_bool().unwrap_or(false),
                        _ => true,
                    };
                    if !ok {
                        return false;
                    }
                }
                true
            })
            .map(|t| self.tab_for(ext, t))
            .collect()
    }

    fn window_json(&self, w: &Value, populate: bool, ext: &Extension) -> Value {
        let mut w = w.clone();
        let id = w["id"].as_i64().unwrap_or(-1);
        w["focused"] = (id == self.focused_window).into();
        if populate {
            w["tabs"] = Value::Array(self.tabs.values().filter(|t| t["windowId"].as_i64() == Some(id)).map(|t| self.tab_for(ext, t)).collect());
        }
        w
    }

    /// Browser → extensions events and mirror updates.
    ///
    /// * `tabCreated` {tab}, `tabUpdated` {tab, changes}, `tabRemoved` {tabId, windowId, windowClosing},
    ///   `tabActivated` {tabId, windowId, previousTabId}, `tabMoved` {tabId, windowId, fromIndex, toIndex}
    /// * `windowCreated` {window}, `windowRemoved` {windowId}, `windowFocused` {windowId}
    /// * `navigation` {event, tabId, url, frameId, ...} (webNavigation)
    /// * `historyVisited` {entry}, `bookmarksChanged` {...}, `downloadCreated`/`downloadChanged`
    /// * `cookieChanged` {cookie, removed, cause}
    pub fn browser_event(&mut self, name: &str, d: &Value) -> Vec<Effect> {
        let mut fx = vec![];
        match name {
            "tabCreated" => {
                let tab = d["tab"].clone();
                if let Some(id) = tab["id"].as_i64() {
                    self.tabs.insert(id, tab.clone());
                    fx.extend(self.fire_all("tabs.onCreated", |e| Some(json!([self.tab_for(e, &tab)]))));
                }
            }
            "tabUpdated" => {
                let tab = d["tab"].clone();
                let Some(id) = tab["id"].as_i64() else { return fx };
                let old = self.tabs.insert(id, tab.clone());
                let changes = d["changes"].clone();
                if changes.as_object().is_some_and(|c| !c.is_empty()) {
                    // keep content-script sender info current
                    for c in self.ctxs.values_mut() {
                        if c.tab_id == id && c.frame_id == 0 && changes.get("url").is_some() && c.kind != "content" {
                            c.url = tab["url"].as_str().unwrap_or_default().to_owned();
                        }
                    }
                    fx.extend(self.fire_all("tabs.onUpdated", |e| {
                        let mut ch = changes.clone();
                        let url = tab["url"].as_str().unwrap_or_default();
                        if !self.can_see_tab_details(e, url) {
                            if let Some(o) = ch.as_object_mut() {
                                o.remove("url");
                                o.remove("title");
                                o.remove("favIconUrl");
                                if o.is_empty() {
                                    return None;
                                }
                            }
                        }
                        Some(json!([id, ch, self.tab_for(e, &tab)]))
                    }));
                    // Per-tab action state resets on navigation.
                    if changes.get("url").is_some() && old.is_some_and(|o| o["url"] != tab["url"]) {
                        for a in self.actions.values_mut().chain(self.page_actions.values_mut()) {
                            a.per_tab.remove(&id);
                        }
                        fx.push(Effect::Changed("actions".into()));
                    }
                }
            }
            "tabRemoved" => {
                let id = d["tabId"].as_i64().unwrap_or(-1);
                self.tabs.remove(&id);
                for a in self.actions.values_mut().chain(self.page_actions.values_mut()) {
                    a.per_tab.remove(&id);
                }
                let info = json!({"windowId": d["windowId"], "isWindowClosing": d["windowClosing"].as_bool().unwrap_or(false)});
                fx.extend(self.fire_all("tabs.onRemoved", |_| Some(json!([id, info]))));
            }
            "tabActivated" => {
                let id = d["tabId"].as_i64().unwrap_or(-1);
                let wid = d["windowId"].as_i64().unwrap_or(-1);
                for t in self.tabs.values_mut() {
                    if t["windowId"].as_i64() == Some(wid) {
                        let active = t["id"].as_i64() == Some(id);
                        t["active"] = active.into();
                        t["highlighted"] = active.into();
                        if active {
                            t["lastAccessed"] = now_ms().into();
                        }
                    }
                }
                let info = json!({"tabId": id, "windowId": wid, "previousTabId": d["previousTabId"]});
                fx.extend(self.fire_all("tabs.onActivated", |_| Some(json!([info]))));
                fx.extend(self.fire_all("tabs.onHighlighted", |_| Some(json!([{"windowId": wid, "tabIds": [id]}]))));
                fx.push(Effect::Changed("actions".into()));
            }
            "tabMoved" => {
                let id = d["tabId"].as_i64().unwrap_or(-1);
                if let Some(t) = self.tabs.get_mut(&id) {
                    t["index"] = d["toIndex"].clone();
                }
                let info = json!({"windowId": d["windowId"], "fromIndex": d["fromIndex"], "toIndex": d["toIndex"]});
                fx.extend(self.fire_all("tabs.onMoved", |_| Some(json!([id, info]))));
            }
            "tabsReindexed" => {
                // {windowId, order: [tabId...]}
                if let Some(order) = d["order"].as_array() {
                    for (i, tid) in order.iter().enumerate() {
                        if let Some(t) = tid.as_i64().and_then(|id| self.tabs.get_mut(&id)) {
                            t["index"] = (i as i64).into();
                        }
                    }
                }
            }
            "windowCreated" => {
                let w = d["window"].clone();
                if let Some(id) = w["id"].as_i64() {
                    self.windows.insert(id, w.clone());
                    fx.extend(self.fire_all("windows.onCreated", |e| Some(json!([self.window_json(&w, false, e)]))));
                }
            }
            "windowRemoved" => {
                let id = d["windowId"].as_i64().unwrap_or(-1);
                self.windows.remove(&id);
                fx.extend(self.fire_all("windows.onRemoved", |_| Some(json!([id]))));
            }
            "windowFocused" => {
                let id = d["windowId"].as_i64().unwrap_or(-1);
                if id != self.focused_window {
                    self.focused_window = id;
                    fx.extend(self.fire_all("windows.onFocusChanged", |_| Some(json!([id]))));
                }
            }
            "navigation" => {
                let ev = d["event"].as_str().unwrap_or_default();
                let url = d["url"].as_str().unwrap_or_default().to_owned();
                let mut details = d.clone();
                if let Some(o) = details.as_object_mut() {
                    o.remove("event");
                    o.insert("timeStamp".into(), now_ms().into());
                    o.entry("frameId").or_insert(json!(0));
                    o.entry("parentFrameId").or_insert(json!(-1));
                    o.entry("processId").or_insert(json!(-1));
                }
                fx.extend(self.fire_all(&format!("webNavigation.{ev}"), |e| {
                    (e.has_permission("webNavigation") && !url.starts_with("kite:")).then(|| json!([details]))
                }));
            }
            "historyVisited" => {
                let entry = d["entry"].clone();
                fx.extend(self.fire_all("history.onVisited", |e| e.has_permission("history").then(|| json!([entry]))));
                fx.extend(self.fire_all("history.onTitleChanged", |e| {
                    (e.has_permission("history") && d["titleChanged"].as_bool() == Some(true)).then(|| json!([{"url": entry["url"], "title": entry["title"]}]))
                }));
            }
            "historyRemoved" => {
                let info = json!({"allHistory": d["allHistory"].as_bool().unwrap_or(false), "urls": d["urls"]});
                fx.extend(self.fire_all("history.onVisitRemoved", |e| e.has_permission("history").then(|| json!([info]))));
            }
            "bookmarkEvent" => {
                // {name: "onCreated"|"onRemoved"|"onChanged"|"onMoved", args: [...]}
                let ev = format!("bookmarks.{}", d["name"].as_str().unwrap_or_default());
                let args = d["args"].clone();
                fx.extend(self.fire_all(&ev, |e| e.has_permission("bookmarks").then(|| args.clone())));
                fx.push(Effect::Changed("bookmarks".into()));
            }
            "downloadCreated" | "downloadChanged" | "downloadErased" => {
                let ev = format!("downloads.on{}", &name["download".len()..]);
                let args = d["args"].clone();
                fx.extend(self.fire_all(&ev, |e| e.has_permission("downloads").then(|| args.clone())));
            }
            "cookieChanged" => {
                let c: Option<crate::shared::Cookie> = serde_json::from_value(d["cookie"].clone()).ok();
                if let Some(c) = c {
                    let removed = d["removed"].as_bool().unwrap_or(false);
                    {
                        let mut jar = self.shared.cookies();
                        jar.retain(|x| !(x.name == c.name && x.domain == c.domain && x.path == c.path));
                        if !removed {
                            jar.push(c.clone());
                        }
                    }
                    let info = json!({"removed": removed, "cookie": c.to_api(), "cause": d["cause"].as_str().unwrap_or(if removed { "explicit" } else { "explicit" })});
                    if d["initial"].as_bool() != Some(true) {
                        fx.extend(self.fire_all("cookies.onChanged", |e| e.has_permission("cookies").then(|| json!([info]))));
                    }
                }
            }
            "notificationClicked" | "notificationClosed" | "notificationButtonClicked" => {
                let ext = d["ext"].as_str().unwrap_or_default().to_owned();
                let ev = format!("notifications.on{}", &name["notification".len()..]);
                let mut args = vec![d["id"].clone()];
                if name == "notificationClosed" {
                    args.push(json!(d["byUser"].as_bool().unwrap_or(true)));
                    if let Some(list) = self.notifications.get_mut(&ext) {
                        list.retain(|n| Some(n.as_str()) != d["id"].as_str());
                    }
                }
                if name == "notificationButtonClicked" {
                    args.push(d["buttonIndex"].clone());
                }
                fx.extend(self.fire(&ext, &ev, Value::Array(args), false));
            }
            "omniboxInputEntered" | "omniboxInputChanged" | "omniboxInputStarted" | "omniboxInputCancelled" => {
                let ext = d["ext"].as_str().unwrap_or_default().to_owned();
                let ev = format!("omnibox.onInput{}", &name["omniboxInput".len()..]);
                fx.extend(self.fire(&ext, &ev, d["args"].clone(), false));
            }
            _ => {}
        }
        fx
    }

    // ------------------------------------------------------------------
    // Periodic work
    // ------------------------------------------------------------------

    /// Fire due alarms. Call about once per second.
    pub fn tick(&mut self) -> Vec<Effect> {
        let now = now_ms() as f64;
        let mut fx = vec![];
        // Give up on background pages that never finished loading.
        for queue in self.waiting_for_background.values_mut() {
            queue.retain(|q| {
                if q.since.elapsed() < BACKGROUND_STARTUP_TIMEOUT {
                    return true;
                }
                if q.method.ends_with("connect") {
                    let port = q.args.first().and_then(Value::as_str).unwrap_or_default();
                    fx.push(Effect::Deliver { view: q.origin.view.clone(), ctxs: vec![q.ctx.clone()], payload: json!({"type": "portDisconnect", "portId": port, "error": NO_RECEIVER}) });
                } else {
                    fx.push(Effect::Reply { view: q.origin.view.clone(), ctx: q.ctx.clone(), call_id: q.call_id, result: Err(NO_RECEIVER.into()) });
                }
                false
            });
        }
        self.waiting_for_background.retain(|_, q| !q.is_empty());
        let due: Vec<(String, String)> = self.alarms.iter().filter(|(_, a)| a.scheduled <= now).map(|(k, _)| k.clone()).collect();
        for key in due {
            let a = self.alarms.get(&key).cloned().unwrap();
            let info = json!({"name": key.1, "scheduledTime": a.scheduled, "periodInMinutes": a.period});
            fx.extend(self.fire(&key.0, "alarms.onAlarm", json!([info]), false));
            match a.period {
                Some(p) => {
                    self.alarms.get_mut(&key).unwrap().scheduled = now + p * 60_000.0;
                }
                None => {
                    self.alarms.remove(&key);
                }
            }
        }
        fx
    }

    // ------------------------------------------------------------------
    // Messaging
    // ------------------------------------------------------------------

    fn respond(&mut self, msg_id: u64, from: &str, value: Value, handled: bool) -> Vec<Effect> {
        let Some(m) = self.msgs.get_mut(&msg_id) else { return vec![] };
        if !m.targets.iter().any(|t| t == from) {
            return vec![];
        }
        m.targets.retain(|t| t != from);
        if handled {
            let m = self.msgs.remove(&msg_id).unwrap();
            return vec![Effect::Reply { view: m.reply_view, ctx: m.reply_ctx, call_id: m.call_id, result: Ok(value) }];
        }
        m.remaining = m.remaining.saturating_sub(1);
        if m.remaining == 0 {
            let m = self.msgs.remove(&msg_id).unwrap();
            return vec![Effect::Reply { view: m.reply_view, ctx: m.reply_ctx, call_id: m.call_id, result: Ok(Value::Null) }];
        }
        vec![]
    }

    /// Contexts of `ext` (other than `except`) that receive runtime messages:
    /// extension pages, but a background page only once it finished loading.
    fn page_targets(&self, ext: &str, except: &str) -> Vec<String> {
        self.ctxs
            .iter()
            .filter(|(k, c)| c.ext == ext && c.kind != "content" && *k != except)
            .filter(|(k, c)| c.kind != "background" || self.ready.get(ext).is_some_and(|r| r == *k))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// The extension has a background page that has not finished loading.
    fn background_starting(&self, ext: &str) -> bool {
        !self.ready.contains_key(ext) && self.registry.get(ext).is_some_and(|e| e.info.enabled && e.manifest.background.is_some())
    }

    fn queue_for_background(&mut self, ext: &str, origin: &Origin, ctx: &str, method: &str, args: &[Value], call_id: i64) {
        log::debug!("{ext}: background not ready, queueing {method} from {ctx}");
        self.waiting_for_background.entry(ext.to_owned()).or_default().push(QueuedCall {
            origin: origin.clone(),
            ctx: ctx.to_owned(),
            method: method.to_owned(),
            args: args.to_vec(),
            call_id,
            since: std::time::Instant::now(),
        });
    }

    fn send_message(&mut self, origin: &Origin, from_ctx: &str, call_id: i64, ext: &str, targets: Vec<String>, message: Value, kind: &str) -> Handled {
        let Some(c) = self.ctxs.get(from_ctx).cloned() else { return Handled::Now(Err("context not registered".into())) };
        if targets.is_empty() {
            return Handled::Now(Err(NO_RECEIVER.into()));
        }
        let id = self.next_msg;
        self.next_msg += 1;
        self.msgs.insert(
            id,
            PendingMsg { reply_view: origin.view.clone(), reply_ctx: from_ctx.into(), call_id, remaining: targets.len(), targets: targets.clone() },
        );
        let sender = self.sender(&c, ext);
        Handled::Later(self.deliver(&targets, json!({"type": kind, "msgId": id, "message": message, "sender": sender})))
    }

    fn port_disconnect_from(&mut self, port: &str, from: &str, error: Option<String>) -> Vec<Effect> {
        let Some(p) = self.ports.get_mut(port) else { return vec![] };
        log::debug!("port {port}: disconnect from {from} (opener {}, others {:?})", p.opener, p.others);
        let payload = json!({"type": "portDisconnect", "portId": port, "error": error});
        if p.opener == from {
            let others = p.others.clone();
            self.ports.remove(port);
            return self.deliver(&others, payload);
        }
        p.others.retain(|o| o != from);
        if p.others.is_empty() {
            let opener = p.opener.clone();
            self.ports.remove(port);
            return self.deliver(&[opener], payload);
        }
        vec![]
    }

    // ------------------------------------------------------------------
    // Toolbar actions
    // ------------------------------------------------------------------

    fn action_map(&mut self, ns: &str) -> &mut HashMap<String, Actions> {
        match ns {
            "pageAction" => &mut self.page_actions,
            "sidebarAction" => &mut self.sidebars,
            _ => &mut self.actions,
        }
    }

    fn action_set(&mut self, ns: &str, ext: &str, details: &Value, f: impl FnOnce(&mut ActionState, &Value)) -> Vec<Effect> {
        let tab = details.get("tabId").and_then(Value::as_i64);
        let a = self.action_map(ns).entry(ext.to_owned()).or_default();
        let st = match tab {
            Some(t) => a.per_tab.entry(t).or_default(),
            None => &mut a.global,
        };
        f(st, details);
        vec![Effect::Changed("actions".into())]
    }

    fn action_state(&self, ns: &str, ext: &Extension, tab: Option<i64>) -> ActionState {
        let man = match ns {
            "pageAction" => ext.manifest.page_action.as_ref(),
            "sidebarAction" => ext.manifest.sidebar_action.as_ref(),
            _ => ext.manifest.action.as_ref(),
        };
        let base = ActionState {
            title: Some(man.map(|m| m.default_title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| ext.manifest.name.clone())),
            popup: Some(man.map(|m| m.default_popup.clone()).unwrap_or_default()),
            badge_text: Some(String::new()),
            badge_bg: Some("#d92626".into()),
            badge_fg: Some("#ffffff".into()),
            icon: man
                .and_then(|m| super::manifest::Manifest::icon_for(&m.default_icon, 32))
                .or_else(|| super::manifest::Manifest::icon_for(&ext.manifest.icons, 32))
                .map(|p| ext.url(&p)),
            enabled: Some(true),
            shown: Some(ns != "pageAction" || man.is_some_and(|m| !m.show_matches.is_empty() && tab.and_then(|t| self.tabs.get(&t)).is_some_and(|t| MatchSet::parse(m.show_matches.iter().map(String::as_str)).matches(t["url"].as_str().unwrap_or(""))))),
        };
        let map = match ns {
            "pageAction" => &self.page_actions,
            "sidebarAction" => &self.sidebars,
            _ => &self.actions,
        };
        let Some(a) = map.get(&ext.info.id) else { return base };
        let g = base.merged(Some(&a.global));
        g.merged(tab.and_then(|t| a.per_tab.get(&t)))
    }

    /// Toolbar buttons for the active tab, for the GUI.
    pub fn toolbar_json(&self, tab: i64) -> Value {
        let tab = (tab >= 0).then_some(tab);
        let mut out = vec![];
        for e in self.registry.enabled() {
            for (ns, has) in [("browserAction", e.manifest.action.is_some()), ("pageAction", e.manifest.page_action.is_some())] {
                if !has {
                    continue;
                }
                let s = self.action_state(ns, e, tab);
                if ns == "pageAction" && s.shown != Some(true) {
                    continue;
                }
                out.push(json!({
                    "ext": e.info.id, "kind": ns, "name": e.manifest.name,
                    "title": s.title, "popup": s.popup.filter(|p| !p.is_empty()).map(|p| if p.contains("://") { p } else { e.url(&p) }),
                    "badgeText": s.badge_text, "badgeBg": s.badge_bg, "badgeFg": s.badge_fg,
                    "icon": s.icon, "enabled": s.enabled,
                }));
            }
        }
        Value::Array(out)
    }

    pub fn sidebars_json(&self) -> Value {
        Value::Array(
            self.registry
                .enabled()
                .filter(|e| e.manifest.sidebar_action.is_some())
                .map(|e| {
                    let s = self.action_state("sidebarAction", e, None);
                    json!({"ext": e.info.id, "title": s.title, "icon": s.icon, "panel": s.popup.map(|p| if p.contains("://") { p } else { e.url(&p) })})
                })
                .collect(),
        )
    }

    /// The user clicked an extension's toolbar button without a popup.
    pub fn action_clicked(&mut self, ext: &str, kind: &str, tab_id: i64) -> Vec<Effect> {
        let Some(tab) = self.tabs.get(&tab_id).cloned() else { return vec![] };
        let Some(e) = self.registry.get(ext) else { return vec![] };
        let t = self.tab_for(e, &tab);
        let mut fx = self.fire(ext, &format!("{kind}.onClicked"), json!([t, {"button": 0, "modifiers": []}]), false);
        if kind == "browserAction" {
            fx.extend(self.fire(ext, "action.onClicked", json!([t, {"button": 0, "modifiers": []}]), false));
        }
        fx
    }

    // ------------------------------------------------------------------
    // Context menus
    // ------------------------------------------------------------------

    /// Menu entries matching a context-menu invocation.
    /// `info`: {contexts: [...], pageUrl, linkUrl, srcUrl, frameUrl, selectionText, editable, mediaType}
    pub fn menus_json(&self, info: &Value) -> Value {
        let contexts: Vec<String> = info["contexts"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        let page_url = info["pageUrl"].as_str().unwrap_or_default();
        let target_url = info["linkUrl"].as_str().or(info["srcUrl"].as_str()).unwrap_or_default();
        let visible = |m: &MenuItem| -> bool {
            if !m.visible {
                return false;
            }
            let ctx_ok = m.contexts.iter().any(|c| c == "all" || contexts.contains(c) || (c == "page" && contexts.len() == 1 && contexts[0] == "page"));
            let doc_ok = m.document_patterns.as_ref().map(|p| p.matches(page_url)).unwrap_or(true);
            let tgt_ok = target_url.is_empty() || m.target_patterns.as_ref().map(|p| p.matches(target_url)).unwrap_or(true);
            ctx_ok && doc_ok && tgt_ok
        };
        let mut out = vec![];
        for e in self.registry.enabled() {
            let items: Vec<&MenuItem> = self.menus.iter().filter(|m| m.ext == e.info.id).collect();
            fn build(items: &[&MenuItem], parent: Option<&str>, visible: &dyn Fn(&MenuItem) -> bool, sel: &str) -> Vec<Value> {
                items
                    .iter()
                    .filter(|m| m.parent.as_deref() == parent && (parent.is_some() || visible(m)))
                    .map(|m| {
                        json!({
                            "id": m.id, "ext": m.ext, "type": m.kind, "checked": m.checked, "enabled": m.enabled, "icon": m.icon,
                            "title": m.title.replace("%s", &sel.chars().take(40).collect::<String>()),
                            "children": build(items, Some(&m.id), visible, sel),
                        })
                    })
                    .collect()
            }
            let sel = info["selectionText"].as_str().unwrap_or_default();
            let top = build(&items, None, &visible, sel);
            if top.is_empty() {
                continue;
            }
            // Firefox groups multiple top-level items under the extension name.
            if top.len() > 1 {
                out.push(json!({"id": "", "ext": e.info.id, "type": "normal", "enabled": true, "title": e.manifest.name, "icon": e.icon_url(16), "children": top}));
            } else {
                out.extend(top);
            }
        }
        Value::Array(out)
    }

    pub fn menu_clicked(&mut self, ext: &str, item: &str, info: &Value, tab_id: i64) -> Vec<Effect> {
        let Some(m) = self.menus.iter_mut().find(|m| m.ext == ext && m.id == item) else { return vec![] };
        let mut was_checked = None;
        if m.kind == "checkbox" {
            was_checked = Some(m.checked);
            m.checked = !m.checked;
        }
        if m.kind == "radio" {
            was_checked = Some(m.checked);
            let parent = m.parent.clone();
            for o in self.menus.iter_mut().filter(|o| o.ext == ext && o.kind == "radio" && o.parent == parent) {
                o.checked = o.id == item;
            }
        }
        let mut data = info.clone();
        data["menuItemId"] = item.into();
        if let Some(p) = self.menus.iter().find(|m| m.ext == ext && m.id == item).and_then(|m| m.parent.clone()) {
            data["parentMenuItemId"] = p.into();
        }
        if let Some(w) = was_checked {
            data["wasChecked"] = w.into();
            data["checked"] = self.menus.iter().find(|m| m.ext == ext && m.id == item).map(|m| m.checked).into();
        }
        data["modifiers"] = json!([]);
        data["button"] = json!(0);
        let tab = self.tabs.get(&tab_id).cloned();
        let tab = match (tab, self.registry.get(ext)) {
            (Some(t), Some(e)) => {
                // Clicking a menu grants activeTab-like access: include details.
                let _ = e;
                t
            }
            _ => Value::Null,
        };
        let mut fx = self.fire(ext, "menus.onClicked", json!([data, tab]), false);
        fx.extend(self.fire(ext, "contextMenus.onClicked", json!([data, tab]), false));
        fx
    }

    pub fn command(&mut self, ext: &str, name: &str, tab_id: i64) -> Vec<Effect> {
        let tab = self.tabs.get(&tab_id).cloned().unwrap_or(Value::Null);
        self.fire(ext, "commands.onCommand", json!([name, tab]), false)
    }

    pub fn commands_json(&self) -> Value {
        Value::Array(
            self.registry
                .enabled()
                .flat_map(|e| {
                    // All commands, also those without a suggested key: the
                    // user can give them one (Settings → Shortcuts).
                    e.manifest.commands.iter().map(move |c| json!({"ext": e.info.id, "extName": e.manifest.name, "name": c.name, "shortcut": c.shortcut, "description": c.description}))
                })
                .collect(),
        )
    }

    // ------------------------------------------------------------------
    // Network hooks (called synchronously from the request interceptor)
    // ------------------------------------------------------------------

    pub fn dnr_check(&self, url: &str, initiator: &str, method: &str, qt_type: &str) -> dnr::Outcome {
        if self.dnr.is_empty() {
            return dnr::Outcome::None;
        }
        let req = dnr::Request::new(url, initiator, method, qt_type);
        dnr::combine(
            self.dnr
                .iter()
                .filter(|(id, _)| self.registry.get(id).is_some_and(|e| e.info.enabled))
                .map(|(_, set)| set.evaluate(&req)),
        )
    }

    /// Fire webRequest events for a request. Non-blocking listeners get
    /// them as events; the returned extensions have blocking
    /// `onBeforeRequest` listeners for it and must be asked through
    /// [`Runtime::blocking`] (outside the runtime lock).
    pub fn web_request(&mut self, details: &Value) -> (Vec<Effect>, Vec<String>) {
        if self.web_request.is_empty() {
            return (vec![], vec![]);
        }
        let url = details["url"].as_str().unwrap_or_default();
        let ty = details["type"].as_str().unwrap_or_default();
        let tab = details["tabId"].as_i64();
        let mut fx = vec![];
        let mut ask = vec![];
        for (ext, listeners) in &self.web_request {
            let Some(e) = self.registry.get(ext) else { continue };
            if !e.info.enabled {
                continue;
            }
            let hosts = MatchSet::parse(e.host_patterns().iter().map(String::as_str));
            if !hosts.matches(url) {
                continue;
            }
            let matching = listeners
                .iter()
                .filter(|l| l.urls.matches(url) && (l.types.is_empty() || l.types.iter().any(|t| t == ty)) && l.tab_id.map(|t| Some(t) == tab).unwrap_or(true));
            let mut events: Vec<&str> = vec![];
            for l in matching {
                if !l.blocking {
                    events.push(l.event.as_str());
                } else if l.event == "onBeforeRequest" && !ask.contains(ext) {
                    ask.push(ext.clone());
                }
            }
            events.sort();
            events.dedup();
            for ev in events {
                fx.extend(self.fire(ext, &format!("webRequest.{ev}"), json!([details]), false));
            }
        }
        (fx, ask)
    }

    /// May tabs start loading? Extensions that can block requests
    /// (`webRequestBlocking`) get to finish starting up first, like Firefox
    /// holding network requests until e.g. uBlock Origin's filters are
    /// loaded. Ready means: answered the [`BlockingHub`] probe, or the
    /// background page is up without any blocking listener.
    pub fn network_ready(&self) -> bool {
        self.registry.enabled().all(|e| {
            let id = &e.info.id;
            !e.has_permission("webRequestBlocking")
                || e.manifest.background.is_none()
                || self.blocking.is_ready(id)
                || (self.ready.contains_key(id) && !self.web_request.get(id).is_some_and(|v| v.iter().any(|l| l.blocking)))
        })
    }

    /// Name of an enabled extension that blocks content itself (uBlock
    /// Origin, AdGuard, uBO Lite, ...), if any. Kite's built-in blocker
    /// stands down while one is active, so pages aren't filtered twice.
    ///
    /// A content blocker either has a blocking `onBeforeRequest` listener
    /// for all web URLs, or a large declarativeNetRequest ruleset.
    pub fn content_blocker(&self) -> Option<&str> {
        const PROBES: [&str; 2] = ["https://kite.invalid/ad.js", "http://kite.invalid/ad.js"];
        const MIN_DNR_RULES: usize = 1000;
        self.registry.enabled().find_map(|e| {
            let id = &e.info.id;
            let listens = self.web_request.get(id).is_some_and(|v| {
                v.iter().any(|l| l.blocking && l.event == "onBeforeRequest" && l.tab_id.is_none() && PROBES.iter().all(|u| l.urls.matches(u)))
            });
            let rules = self.dnr.get(id).map_or(0, |s| s.static_rules.len() + s.dynamic_rules.len());
            (listens || rules >= MIN_DNR_RULES).then_some(e.manifest.name.as_str())
        })
    }

    pub fn has_web_request_listeners(&self) -> bool {
        !self.web_request.is_empty()
    }

    // ------------------------------------------------------------------
    // The API dispatcher
    // ------------------------------------------------------------------

    fn ui(&self, origin: &Origin, ctx: &str, call_id: i64, ext: &str, method: &str, args: Value) -> Handled {
        Handled::Later(vec![Effect::Ui { view: origin.view.clone(), ctx: ctx.into(), call_id, ext: ext.into(), method: method.into(), args }])
    }

    fn require(&self, e: &Extension, perm: &str) -> Result<(), String> {
        if e.has_permission(perm) {
            Ok(())
        } else {
            Err(format!("Missing \"{perm}\" permission"))
        }
    }

    fn resolve_tab(&self, origin: &Origin, ctx: &str, tab: Option<i64>) -> Result<i64, String> {
        if let Some(t) = tab.filter(|t| *t >= 0) {
            return if self.tabs.contains_key(&t) { Ok(t) } else { Err(format!("Invalid tab ID: {t}")) };
        }
        if let Some(c) = self.ctxs.get(ctx) {
            if c.tab_id >= 0 && c.kind != "popup" && c.kind != "background" {
                return Ok(c.tab_id);
            }
        }
        let wid = if origin.window_id >= 0 { origin.window_id } else { self.focused_window };
        self.tabs
            .values()
            .find(|t| t["active"] == true && t["windowId"].as_i64() == Some(wid))
            .or_else(|| self.tabs.values().find(|t| t["active"] == true))
            .and_then(|t| t["id"].as_i64())
            .ok_or_else(|| "No active tab".into())
    }

    /// JS run inside the extension's content world in a tab: the API shim
    /// prelude (so `browser` exists there) followed by `code`.
    fn world_code(&self, ext: &Extension, code: &str) -> String {
        format!("{}\n;{}", super::scripts::content_prelude(ext, self.token_of(&ext.info.id).unwrap_or("")), code)
    }

    fn read_ext_files(ext: &Extension, files: &[Value]) -> Result<Vec<String>, String> {
        files
            .iter()
            .filter_map(Value::as_str)
            .map(|f| ext.resolve(f).and_then(|p| std::fs::read_to_string(p).ok()).ok_or_else(|| format!("Unable to load file {f}")))
            .collect()
    }

    fn css_code(css: &str, key: &str, remove: bool) -> String {
        let key = serde_json::to_string(key).unwrap();
        if remove {
            format!("(function(){{for(const s of document.querySelectorAll('style[data-kite-css]'))if(s.dataset.kiteCss==={key})s.remove();}})();")
        } else {
            format!(
                "(function(){{const s=document.createElement('style');s.dataset.kiteCss={key};s.textContent={};(document.head||document.documentElement).appendChild(s);}})();",
                serde_json::to_string(css).unwrap()
            )
        }
    }

    fn dispatch(&mut self, origin: &Origin, ext_id: &str, ctx: &str, method: &str, args: &[Value], call_id: i64) -> Handled {
        let ext = self.registry.get(ext_id).cloned().expect("checked");
        let e = &ext;
        let (ns, name) = method.split_once('.').unwrap_or((method, ""));
        // Aliases: Firefox `menus` == `contextMenus`, MV3 `action` == `browserAction`.
        let ns = match ns {
            "contextMenus" => "menus",
            "action" => "browserAction",
            n => n,
        };
        macro_rules! now {
            ($e:expr) => {
                Handled::Now($e)
            };
        }
        match (ns, name) {
            // ---------------- internal ----------------
            // A context started (true) or stopped listening to an event of
            // LISTENED_EVENTS.
            ("_", "listen") => {
                let ev = arg_str(args, 0).to_owned();
                let on = arg(args, 1).as_bool().unwrap_or(false);
                let set = match self.ctxs.get_mut(ctx) {
                    Some(c) => &mut c.listens,
                    None => self.early_listens.entry(ctx.into()).or_default(),
                };
                if on {
                    set.insert(ev);
                } else {
                    set.remove(&ev);
                }
                now!(Ok(Value::Null))
            }
            ("_", "register") => {
                let info = arg(args, 0);
                log::debug!("context {ctx} registered: {} in {} (tab {}, frame {})", info["kind"], origin.view, origin.tab_id, info["frameId"]);
                let kind = info["kind"].as_str().unwrap_or("content").to_owned();
                let url = info["url"].as_str().unwrap_or_default().to_owned();
                self.ctxs.insert(
                    ctx.into(),
                    Context {
                        ext: ext_id.into(),
                        kind: kind.clone(),
                        view: origin.view.clone(),
                        tab_id: origin.tab_id,
                        window_id: origin.window_id,
                        frame_id: info["frameId"].as_i64().unwrap_or(0),
                        url,
                        listens: self.early_listens.remove(ctx).unwrap_or_default(),
                    },
                );
                let tab = (origin.tab_id >= 0).then(|| self.tabs.get(&origin.tab_id).cloned()).flatten();
                now!(Ok(json!({"tabId": origin.tab_id, "windowId": origin.window_id, "tab": tab})))
            }
            ("_", "unregister") => {
                log::debug!("context {ctx} unregistered by page");
                Handled::With(Ok(Value::Null), self.unregister(ctx))
            }
            ("_", "ready") => {
                self.ready.insert(ext_id.into(), ctx.into());
                let mut fx = vec![];
                if let Some(ev) = self.pending_startup.remove(ext_id) {
                    let payload = json!({"type": "event", "name": ev["event"], "args": ev["args"]});
                    fx.extend(self.deliver(&[ctx.to_owned()], payload));
                }
                // Deliver what was sent while the background page was loading.
                for q in self.waiting_for_background.remove(ext_id).unwrap_or_default() {
                    if !self.ctxs.contains_key(&q.ctx) {
                        continue;
                    }
                    let replay = match self.dispatch(&q.origin, ext_id, &q.ctx, &q.method, &q.args, q.call_id) {
                        Handled::Now(r) => vec![Effect::Reply { view: q.origin.view.clone(), ctx: q.ctx.clone(), call_id: q.call_id, result: r }],
                        Handled::Later(e) => e,
                        Handled::With(r, mut e) => {
                            e.insert(0, Effect::Reply { view: q.origin.view.clone(), ctx: q.ctx.clone(), call_id: q.call_id, result: r });
                            e
                        }
                    };
                    fx.extend(replay);
                }
                Handled::With(Ok(Value::Null), fx)
            }
            ("_", "respond") => {
                let id = arg_i64(args, 0).unwrap_or(0) as u64;
                Handled::With(Ok(Value::Null), self.respond(id, ctx, arg(args, 1), args.get(2).and_then(Value::as_bool).unwrap_or(true)))
            }
            ("_", "portPost") => {
                let port = arg_str(args, 0);
                log::debug!("port {port}: post from {ctx} (known: {})", self.ports.contains_key(&port));
                let Some(p) = self.ports.get(&port) else { return now!(Err("Attempt to postMessage on disconnected port".into())) };
                let targets = if p.opener == ctx { p.others.clone() } else { vec![p.opener.clone()] };
                Handled::With(Ok(Value::Null), self.deliver(&targets, json!({"type": "portMessage", "portId": port, "message": arg(args, 1)})))
            }
            ("_", "portDisconnect") => Handled::With(Ok(Value::Null), self.port_disconnect_from(&arg_str(args, 0), ctx, None)),
            ("_", "cosmetic") => now!(Ok(Value::Null)),
            // Cross-origin requests from extension pages with host permission
            // (Firefox exempts these from CORS). Performed natively by the GUI layer.
            ("_", "fetch") => {
                let d = arg(args, 0);
                let url = d["url"].as_str().unwrap_or_default();
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return now!(Err("Only http(s) URLs can be fetched".into()));
                }
                if !MatchSet::parse(e.host_patterns().iter().map(String::as_str)).matches(url) {
                    return now!(Err(format!("Missing host permission for {url}")));
                }
                self.ui(origin, ctx, call_id, ext_id, "net.fetch", d)
            }

            // ---------------- runtime ----------------
            ("runtime", "sendMessage") | ("extension", "sendMessage") => {
                let target_ext = arg_str(args, 0);
                let message = arg(args, 1);
                if !target_ext.is_empty() && target_ext != ext_id {
                    // runtime.onMessageExternal of another extension
                    let targets: Vec<String> = self.ctxs.iter().filter(|(_, c)| c.ext == target_ext && c.kind != "content").map(|(k, _)| k.clone()).collect();
                    return self.send_message(origin, ctx, call_id, ext_id, targets, message, "messageExternal");
                }
                let targets = self.page_targets(ext_id, ctx);
                // The background page is the usual receiver: wait for it.
                if self.background_starting(ext_id) {
                    self.queue_for_background(ext_id, origin, ctx, method, args, call_id);
                    return Handled::Later(vec![]);
                }
                self.send_message(origin, ctx, call_id, ext_id, targets, message, "message")
            }
            ("tabs", "sendMessage") => {
                let tab = arg_i64(args, 0).unwrap_or(-1);
                let frame = args.get(2).and_then(|o| o.get("frameId")).and_then(Value::as_i64);
                let targets: Vec<String> = self
                    .ctxs
                    .iter()
                    .filter(|(_, c)| c.ext == ext_id && c.kind == "content" && c.tab_id == tab && frame.map(|f| f == c.frame_id).unwrap_or(true))
                    .map(|(k, _)| k.clone())
                    .collect();
                self.send_message(origin, ctx, call_id, ext_id, targets, arg(args, 1), "message")
            }
            ("runtime", "connect") | ("tabs", "connect") => {
                let port = arg_str(args, 0);
                let (targets, info) = if ns == "tabs" {
                    let tab = arg_i64(args, 1).unwrap_or(-1);
                    let info = arg(args, 2);
                    let frame = info.get("frameId").and_then(Value::as_i64);
                    (
                        self.ctxs
                            .iter()
                            .filter(|(_, c)| c.ext == ext_id && c.kind == "content" && c.tab_id == tab && frame.map(|f| f == c.frame_id).unwrap_or(true))
                            .map(|(k, _)| k.clone())
                            .collect::<Vec<_>>(),
                        info,
                    )
                } else {
                    let target_ext = arg_str(args, 1);
                    let te = if target_ext.is_empty() { ext_id.to_owned() } else { target_ext };
                    let targets = self.page_targets(&te, ctx);
                    if te == ext_id && self.background_starting(ext_id) {
                        self.queue_for_background(ext_id, origin, ctx, method, args, call_id);
                        return Handled::Now(Ok(Value::Null));
                    }
                    (targets, arg(args, 2))
                };
                let Some(c) = self.ctxs.get(ctx).cloned() else { return now!(Err("context not registered".into())) };
                if targets.is_empty() {
                    let fx = self.deliver(&[ctx.to_owned()], json!({"type": "portDisconnect", "portId": port, "error": NO_RECEIVER}));
                    return Handled::With(Ok(Value::Null), fx);
                }
                log::debug!("port {port}: {ctx} connected to {targets:?}");
                self.ports.insert(port.clone(), Port { opener: ctx.into(), others: targets.clone() });
                let fx = self.deliver(&targets, json!({"type": "connect", "portId": port, "name": info["name"].as_str().unwrap_or(""), "sender": self.sender(&c, ext_id)}));
                Handled::With(Ok(Value::Null), fx)
            }
            ("runtime", "getPlatformInfo") => now!(Ok(json!({"os": "linux", "arch": std::env::consts::ARCH.replace("x86_64", "x86-64").replace("aarch64", "arm")}))),
            ("runtime", "getBrowserInfo") => now!(Ok(json!({"name": "Kite", "vendor": "KDE", "version": env!("CARGO_PKG_VERSION"), "buildID": "0"}))),
            ("runtime", "openOptionsPage") => match &e.manifest.options_page {
                Some(p) => {
                    let url = e.url(p);
                    let in_tab = e.manifest.options_open_in_tab;
                    self.ui(origin, ctx, call_id, ext_id, "openOptions", json!({"url": url, "inTab": in_tab, "name": e.manifest.name}))
                }
                None => now!(Err("No options page".into())),
            },
            ("runtime", "setUninstallURL") => {
                let u = arg_str(args, 0);
                if u.is_empty() {
                    self.uninstall_urls.remove(ext_id);
                } else if u.starts_with("http:") || u.starts_with("https:") {
                    self.uninstall_urls.insert(ext_id.into(), u);
                } else {
                    return now!(Err("Invalid URL".into()));
                }
                now!(Ok(Value::Null))
            }
            ("runtime", "reload") => {
                let fx = self.on_enabled_changed(ext_id, true);
                Handled::With(Ok(Value::Null), fx)
            }
            ("runtime", "requestUpdateCheck") => now!(Ok(json!(["no_update", {}]))),
            ("runtime", "getBackgroundPage") | ("extension", "getBackgroundPage") => {
                now!(Err("getBackgroundPage() is not supported in Kite; use runtime.sendMessage".into()))
            }
            ("runtime", "sendNativeMessage") | ("runtime", "connectNative") => now!(Err("Native messaging is not supported".into())),
            ("runtime", "getContexts") => now!(Ok(Value::Array(
                self.ctxs
                    .values()
                    .filter(|c| c.ext == ext_id && c.kind != "content")
                    .map(|c| json!({"contextType": c.kind.to_uppercase(), "documentUrl": c.url, "tabId": c.tab_id, "windowId": c.window_id, "frameId": c.frame_id, "incognito": false}))
                    .collect()
            ))),
            ("extension", "isAllowedIncognitoAccess") => now!(Ok(json!(false))),
            ("extension", "isAllowedFileSchemeAccess") => now!(Ok(json!(true))),

            // ---------------- i18n ----------------
            ("i18n", "getAcceptLanguages") => now!(Ok(json!([super::i18n::ui_locale().replace('_', "-"), "en"]))),
            ("i18n", "detectLanguage") => now!(Ok(json!({"isReliable": false, "languages": []}))),

            // ---------------- storage ----------------
            ("storage", _) => {
                let (area, op) = name.split_once('.').unwrap_or(("local", name));
                if !e.has_permission("storage") && !e.has_permission("unlimitedStorage") {
                    return now!(Err("Missing \"storage\" permission".into()));
                }
                let r = match op {
                    "get" => self.storage.get(ext_id, area, &arg(args, 0)).map_err(|e| e.to_string()),
                    "getBytesInUse" => self.storage.bytes_in_use(ext_id, area, &arg(args, 0)).map(|n| json!(n)).map_err(|e| e.to_string()),
                    "getKeys" => self.storage.get(ext_id, area, &Value::Null).map(|v| json!(v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()))).map_err(|e| e.to_string()),
                    "set" | "remove" | "clear" => {
                        let changes = match op {
                            "set" => self.storage.set(ext_id, area, &arg(args, 0)),
                            "remove" => self.storage.remove(ext_id, area, &arg(args, 0)),
                            _ => self.storage.clear(ext_id, area),
                        };
                        return match changes {
                            Ok(ch) if ch.is_empty() => now!(Ok(Value::Null)),
                            Ok(ch) => {
                                let ch = Value::Object(ch);
                                let mut fx = self.fire(ext_id, "storage.onChanged", json!([ch, area]), true);
                                fx.extend(self.fire(ext_id, &format!("storage.{area}.onChanged"), json!([ch]), true));
                                Handled::With(Ok(Value::Null), fx)
                            }
                            Err(err) => now!(Err(err.to_string())),
                        };
                    }
                    "setAccessLevel" => Ok(Value::Null),
                    _ => Err(format!("storage.{name} is not supported")),
                };
                now!(r)
            }

            // ---------------- tabs ----------------
            ("tabs", "query") => now!(Ok(Value::Array(self.query_tabs(e, &arg(args, 0), origin)))),
            ("tabs", "get") => {
                let id = arg_i64(args, 0).unwrap_or(-1);
                now!(self.tabs.get(&id).map(|t| self.tab_for(e, t)).ok_or_else(|| format!("Invalid tab ID: {id}")))
            }
            ("tabs", "getCurrent") => {
                let c = self.ctxs.get(ctx);
                now!(Ok(c.filter(|c| c.tab_id >= 0 && c.kind == "tab").and_then(|c| self.tabs.get(&c.tab_id)).map(|t| self.tab_for(e, t)).unwrap_or(Value::Null)))
            }
            ("tabs", "create") => {
                let mut p = arg(args, 0);
                if let Some(u) = p["url"].as_str() {
                    let abs = if u.contains(':') { u.to_owned() } else { e.url(u) };
                    if abs.starts_with("javascript:") || abs.starts_with("data:text/html") && false {
                        return now!(Err("Illegal URL".into()));
                    }
                    p["url"] = abs.into();
                }
                self.ui(origin, ctx, call_id, ext_id, "tabs.create", p)
            }
            ("tabs", "update") => {
                let (tab, mut props) = if args.len() >= 2 { (arg_i64(args, 0), arg(args, 1)) } else { (None, arg(args, 0)) };
                let tab = match self.resolve_tab(origin, ctx, tab) {
                    Ok(t) => t,
                    Err(err) => return now!(Err(err)),
                };
                if let Some(u) = props["url"].as_str() {
                    if !u.contains(':') {
                        props["url"] = e.url(u).into();
                    }
                }
                self.ui(origin, ctx, call_id, ext_id, "tabs.update", json!({"tabId": tab, "props": props}))
            }
            ("tabs", "remove") => {
                let ids: Vec<Value> = match arg(args, 0) {
                    Value::Array(a) => a,
                    v => vec![v],
                };
                self.ui(origin, ctx, call_id, ext_id, "tabs.remove", json!({"tabIds": ids}))
            }
            ("tabs", "reload") | ("tabs", "goBack") | ("tabs", "goForward") | ("tabs", "duplicate") | ("tabs", "discard") | ("tabs", "getZoom") | ("tabs", "print") | ("tabs", "saveAsPDF") | ("tabs", "toggleReaderMode") => {
                let tab = match self.resolve_tab(origin, ctx, arg_i64(args, 0)) {
                    Ok(t) => t,
                    Err(err) => return now!(Err(err)),
                };
                self.ui(origin, ctx, call_id, ext_id, method, json!({"tabId": tab, "props": arg(args, 1)}))
            }
            ("tabs", "setZoom") => {
                let (tab, z) = if args.len() >= 2 { (arg_i64(args, 0), arg(args, 1)) } else { (None, arg(args, 0)) };
                match self.resolve_tab(origin, ctx, tab) {
                    Ok(t) => self.ui(origin, ctx, call_id, ext_id, "tabs.setZoom", json!({"tabId": t, "zoom": z})),
                    Err(err) => now!(Err(err)),
                }
            }
            ("tabs", "getZoomSettings") => now!(Ok(json!({"mode": "automatic", "scope": "per-origin", "defaultZoomFactor": 1.0}))),
            ("tabs", "setZoomSettings") => now!(Ok(Value::Null)),
            ("tabs", "move") => {
                let ids: Vec<Value> = match arg(args, 0) {
                    Value::Array(a) => a,
                    v => vec![v],
                };
                self.ui(origin, ctx, call_id, ext_id, "tabs.move", json!({"tabIds": ids, "props": arg(args, 1)}))
            }
            ("tabs", "highlight") => {
                let p = arg(args, 0);
                let first = match &p["tabs"] {
                    Value::Array(a) => a.first().and_then(Value::as_i64),
                    v => v.as_i64(),
                };
                let wid = p["windowId"].as_i64();
                let tab = first.and_then(|idx| self.tabs.values().find(|t| t["index"].as_i64() == Some(idx) && wid.map(|w| t["windowId"].as_i64() == Some(w)).unwrap_or(true)).and_then(|t| t["id"].as_i64()));
                match tab {
                    Some(t) => self.ui(origin, ctx, call_id, ext_id, "tabs.update", json!({"tabId": t, "props": {"active": true}})),
                    None => now!(Err("No tab at index".into())),
                }
            }
            ("tabs", "captureVisibleTab") | ("tabs", "captureTab") => {
                let tab = if name == "captureTab" { arg_i64(args, 0) } else { None };
                match self.resolve_tab(origin, ctx, tab) {
                    Ok(t) => {
                        let opts = if name == "captureTab" { arg(args, 1) } else { args.iter().find(|a| a.is_object()).cloned().unwrap_or(Value::Null) };
                        self.ui(origin, ctx, call_id, ext_id, "tabs.capture", json!({"tabId": t, "options": opts}))
                    }
                    Err(err) => now!(Err(err)),
                }
            }
            ("tabs", "detectLanguage") => now!(Ok(json!("und"))),
            ("tabs", "executeScript") | ("tabs", "insertCSS") | ("tabs", "removeCSS") => {
                let (tab, details) = if args.len() >= 2 { (arg_i64(args, 0), arg(args, 1)) } else { (None, arg(args, 0)) };
                let tab = match self.resolve_tab(origin, ctx, tab) {
                    Ok(t) => t,
                    Err(err) => return now!(Err(err)),
                };
                let url = self.tabs.get(&tab).and_then(|t| t["url"].as_str()).unwrap_or_default().to_owned();
                if !self.can_inject(e, &url) {
                    return now!(Err(format!("Missing host permission for the tab")));
                }
                let src = if let Some(f) = details["file"].as_str() {
                    match Self::read_ext_files(e, &[json!(f)]) {
                        Ok(v) => v.join("\n"),
                        Err(err) => return now!(Err(err)),
                    }
                } else {
                    details["code"].as_str().unwrap_or_default().to_owned()
                };
                // uBO's scriptlets must run in the page's world (see compat).
                if name == "executeScript" {
                    if let Some(inj) = super::compat::split_main_world_injector(&src) {
                        return self.ui(origin, ctx, call_id, ext_id, "tabs.runInWorld", json!({"tabId": tab, "world": 0, "code": inj.main, "wrapResult": true, "allFrames": details["allFrames"]}));
                    }
                }
                let code = match name {
                    "executeScript" => self.world_code(e, &src),
                    _ => Self::css_code(&src, &src, name == "removeCSS"),
                };
                self.ui(origin, ctx, call_id, ext_id, "tabs.runInWorld", json!({"tabId": tab, "world": self.world_of(ext_id), "code": code, "wrapResult": name == "executeScript", "allFrames": details["allFrames"]}))
            }

            // ---------------- scripting (MV3) ----------------
            ("scripting", "executeScript") | ("scripting", "insertCSS") | ("scripting", "removeCSS") => {
                let d = arg(args, 0);
                let tab = d["target"]["tabId"].as_i64().unwrap_or(-1);
                let url = self.tabs.get(&tab).and_then(|t| t["url"].as_str()).unwrap_or_default().to_owned();
                if !self.tabs.contains_key(&tab) {
                    return now!(Err(format!("Invalid tab ID: {tab}")));
                }
                if !self.can_inject(e, &url) {
                    return now!(Err("Missing host permission for the tab".into()));
                }
                let files: Vec<Value> = d["files"].as_array().cloned().unwrap_or_default();
                let file_src = match Self::read_ext_files(e, &files) {
                    Ok(v) => v.join("\n"),
                    Err(err) => return now!(Err(err)),
                };
                let (code, world) = if name == "executeScript" {
                    let body = if let Some(func) = d["func"].as_str() {
                        format!("({func})(...{})", serde_json::to_string(&d["args"].as_array().cloned().unwrap_or_default()).unwrap())
                    } else {
                        file_src
                    };
                    if d["world"] == "MAIN" {
                        (body, 0)
                    } else {
                        (self.world_code(e, &body), self.world_of(ext_id))
                    }
                } else {
                    let css = d["css"].as_str().map(str::to_owned).unwrap_or(file_src);
                    (Self::css_code(&css, &css, name == "removeCSS"), self.world_of(ext_id))
                };
                self.ui(origin, ctx, call_id, ext_id, "tabs.runInWorld", json!({"tabId": tab, "world": world, "code": code, "wrapResult": name == "executeScript", "scripting": true, "allFrames": d["target"]["allFrames"]}))
            }
            ("scripting", "registerContentScripts") | ("contentScripts", "register") => {
                let list = if ns == "contentScripts" { vec![arg(args, 0)] } else { arg(args, 0).as_array().cloned().unwrap_or_default() };
                let entry = self.dynamic_scripts.entry(ext_id.into()).or_default();
                let mut ids = vec![];
                for mut s in list {
                    if s["id"].is_null() {
                        s["id"] = format!("kite-dyn-{}", token()).into();
                    }
                    // contentScripts.register uses {js:[{file|code}]}
                    ids.push(s["id"].clone());
                    entry.retain(|x| x["id"] != s["id"] && x["kiteParent"] != s["id"]);
                    // uBO's scriptlets go to a companion main-world script (see compat).
                    let mut main = vec![];
                    for item in s["js"].as_array_mut().into_iter().flatten() {
                        if let Some(inj) = item["code"].as_str().and_then(super::compat::split_main_world_injector) {
                            item["code"] = inj.isolated.into();
                            main.push(json!({"code": inj.main}));
                        }
                    }
                    if !main.is_empty() {
                        let mut m = s.clone();
                        m["id"] = format!("{}:main", s["id"].as_str().unwrap_or_default()).into();
                        m["kiteParent"] = s["id"].clone();
                        m["world"] = "MAIN".into();
                        m["js"] = main.into();
                        m.as_object_mut().map(|o| o.remove("css"));
                        entry.push(m);
                    }
                    entry.push(s);
                }
                Handled::With(Ok(if ns == "contentScripts" { json!({"id": ids[0]}) } else { Value::Null }), vec![Effect::Changed("scripts".into())])
            }
            ("scripting", "unregisterContentScripts") | ("contentScripts", "unregister") => {
                let f = arg(args, 0);
                let ids: Option<Vec<Value>> = if ns == "contentScripts" { Some(vec![arg(args, 0)]) } else { f["ids"].as_array().cloned() };
                if let Some(list) = self.dynamic_scripts.get_mut(ext_id) {
                    match ids {
                        Some(ids) => list.retain(|s| !ids.contains(&s["id"]) && !ids.contains(&s["kiteParent"])),
                        None => list.clear(),
                    }
                }
                Handled::With(Ok(Value::Null), vec![Effect::Changed("scripts".into())])
            }
            ("scripting", "getRegisteredContentScripts") => now!(Ok(Value::Array(self.dynamic_scripts(ext_id).into_iter().filter(|s| s["kiteParent"].is_null()).collect()))),
            ("scripting", "updateContentScripts") => {
                let list = arg(args, 0).as_array().cloned().unwrap_or_default();
                if let Some(cur) = self.dynamic_scripts.get_mut(ext_id) {
                    for upd in list {
                        if let Some(s) = cur.iter_mut().find(|s| s["id"] == upd["id"]) {
                            if let (Some(o), Some(u)) = (s.as_object_mut(), upd.as_object()) {
                                o.extend(u.clone());
                            }
                        }
                    }
                }
                Handled::With(Ok(Value::Null), vec![Effect::Changed("scripts".into())])
            }

            // ---------------- windows ----------------
            ("windows", "getAll") => {
                let populate = arg(args, 0)["populate"].as_bool().unwrap_or(false);
                now!(Ok(Value::Array(self.windows.values().map(|w| self.window_json(w, populate, e)).collect())))
            }
            ("windows", "get") | ("windows", "getCurrent") | ("windows", "getLastFocused") => {
                let (id, opts) = if name == "get" { (arg_i64(args, 0).unwrap_or(-1), arg(args, 1)) } else { (-2, arg(args, 0)) };
                let id = match (name, id) {
                    ("getLastFocused", _) => self.focused_window,
                    (_, -2) => {
                        if origin.window_id >= 0 {
                            origin.window_id
                        } else {
                            self.focused_window
                        }
                    }
                    (_, id) => id,
                };
                let populate = opts["populate"].as_bool().unwrap_or(false);
                now!(self.windows.get(&id).or_else(|| self.windows.values().next()).map(|w| self.window_json(w, populate, e)).ok_or_else(|| format!("Invalid window ID: {id}")))
            }
            ("windows", "create") | ("windows", "update") | ("windows", "remove") => self.ui(origin, ctx, call_id, ext_id, method, Value::Array(args.to_vec())),

            // ---------------- browserAction / pageAction / sidebarAction ----------------
            ("browserAction" | "pageAction" | "sidebarAction", _) => {
                let d = arg(args, 0);
                let tab = d.get("tabId").and_then(Value::as_i64).or_else(|| d.as_i64());
                let st = self.action_state(ns, e, tab);
                let ns_owned = ns.to_owned();
                match name {
                    "setTitle" => Handled::With(Ok(Value::Null), self.action_set(&ns_owned, ext_id, &d, |s, d| s.title = d["title"].as_str().map(str::to_owned))),
                    "getTitle" => now!(Ok(json!(st.title))),
                    "setPopup" | "setPanel" => {
                        let base = e.base_url();
                        Handled::With(
                            Ok(Value::Null),
                            self.action_set(&ns_owned, ext_id, &d, move |s, d| {
                                let p = d["popup"].as_str().or(d["panel"].as_str()).unwrap_or_default();
                                s.popup = Some(if p.is_empty() || p.contains("://") { p.to_owned() } else { format!("{base}{}", p.trim_start_matches('/')) });
                            }),
                        )
                    }
                    "getPopup" | "getPanel" => now!(Ok(json!(st.popup.map(|p| if p.is_empty() || p.contains("://") { p } else { e.url(&p) })))),
                    "setBadgeText" => Handled::With(Ok(Value::Null), self.action_set(&ns_owned, ext_id, &d, |s, d| s.badge_text = Some(d["text"].as_str().unwrap_or_default().to_owned()))),
                    "getBadgeText" => now!(Ok(json!(st.badge_text))),
                    "setBadgeBackgroundColor" | "setBadgeTextColor" => {
                        let fg = name == "setBadgeTextColor";
                        Handled::With(
                            Ok(Value::Null),
                            self.action_set(&ns_owned, ext_id, &d, move |s, d| {
                                let c = match &d["color"] {
                                    Value::String(c) => Some(c.clone()),
                                    Value::Array(a) if a.len() >= 3 => Some(format!(
                                        "rgba({},{},{},{})",
                                        a[0].as_i64().unwrap_or(0),
                                        a[1].as_i64().unwrap_or(0),
                                        a[2].as_i64().unwrap_or(0),
                                        a.get(3).and_then(Value::as_f64).map(|x| x / 255.0).unwrap_or(1.0)
                                    )),
                                    _ => None,
                                };
                                if fg {
                                    s.badge_fg = c
                                } else {
                                    s.badge_bg = c
                                }
                            }),
                        )
                    }
                    "getBadgeBackgroundColor" | "getBadgeTextColor" => now!(Ok(json!(if name == "getBadgeTextColor" { st.badge_fg } else { st.badge_bg }))),
                    "setIcon" => {
                        let base = e.base_url();
                        Handled::With(
                            Ok(Value::Null),
                            self.action_set(&ns_owned, ext_id, &d, move |s, d| {
                                // JS converts imageData to a data: URL in `_dataUrl`.
                                let icon = if let Some(u) = d["_dataUrl"].as_str() {
                                    Some(u.to_owned())
                                } else {
                                    match &d["path"] {
                                        Value::String(p) => Some(p.clone()),
                                        Value::Object(m) => m.iter().filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v.as_str()?.to_owned()))).filter(|(k, _)| *k >= 16).min_by_key(|(k, _)| *k).map(|(_, v)| v),
                                        _ => None,
                                    }
                                    .map(|p| if p.contains(':') { p } else { format!("{base}{}", p.trim_start_matches('/')) })
                                };
                                s.icon = icon;
                            }),
                        )
                    }
                    "enable" | "disable" => {
                        let on = name == "enable";
                        let d = if d.is_i64() { json!({"tabId": d}) } else { d };
                        Handled::With(Ok(Value::Null), self.action_set(&ns_owned, ext_id, &d, move |s, _| s.enabled = Some(on)))
                    }
                    "isEnabled" => now!(Ok(json!(st.enabled.unwrap_or(true)))),
                    "show" | "hide" => {
                        let on = name == "show";
                        let d = json!({"tabId": d.as_i64().unwrap_or(-1)});
                        Handled::With(Ok(Value::Null), self.action_set(&ns_owned, ext_id, &d, move |s, _| s.shown = Some(on)))
                    }
                    "isShown" => now!(Ok(json!(st.shown.unwrap_or(false)))),
                    "openPopup" => self.ui(origin, ctx, call_id, ext_id, "action.openPopup", json!({"kind": ns_owned})),
                    "open" | "close" | "toggle" | "isOpen" => self.ui(origin, ctx, call_id, ext_id, &format!("sidebar.{name}"), Value::Null),
                    "getUserSettings" => now!(Ok(json!({"isOnToolbar": true}))),
                    _ => now!(Err(format!("{method} is not supported"))),
                }
            }

            // ---------------- menus ----------------
            ("menus", "create") => {
                let p = arg(args, 0);
                let id = match &p["id"] {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => format!("kite-menu-{}", token()),
                };
                if self.menus.iter().any(|m| m.ext == ext_id && m.id == id) {
                    return now!(Err(format!("ID already exists: {id}")));
                }
                let item = MenuItem {
                    ext: ext_id.into(),
                    id: id.clone(),
                    parent: match &p["parentId"] {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    },
                    title: p["title"].as_str().unwrap_or_default().into(),
                    kind: p["type"].as_str().unwrap_or("normal").into(),
                    contexts: p["contexts"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect()).unwrap_or_else(|| vec!["page".into()]),
                    checked: p["checked"].as_bool().unwrap_or(false),
                    enabled: p["enabled"].as_bool().unwrap_or(true),
                    visible: p["visible"].as_bool().unwrap_or(true),
                    document_patterns: p["documentUrlPatterns"].as_array().map(|a| MatchSet::parse(a.iter().filter_map(Value::as_str))),
                    target_patterns: p["targetUrlPatterns"].as_array().map(|a| MatchSet::parse(a.iter().filter_map(Value::as_str))),
                    icon: e.icon_url(16),
                };
                self.menus.push(item);
                Handled::With(Ok(json!(id)), vec![Effect::Changed("menus".into())])
            }
            ("menus", "update") => {
                let id = match &arg(args, 0) {
                    Value::Number(n) => n.to_string(),
                    v => v.as_str().unwrap_or_default().to_owned(),
                };
                let p = arg(args, 1);
                let Some(m) = self.menus.iter_mut().find(|m| m.ext == ext_id && m.id == id) else { return now!(Err(format!("Cannot find menu item with id {id}"))) };
                if let Some(t) = p["title"].as_str() {
                    m.title = t.into();
                }
                if let Some(c) = p["checked"].as_bool() {
                    m.checked = c;
                }
                if let Some(c) = p["enabled"].as_bool() {
                    m.enabled = c;
                }
                if let Some(c) = p["visible"].as_bool() {
                    m.visible = c;
                }
                if let Some(c) = p["contexts"].as_array() {
                    m.contexts = c.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect();
                }
                if let Some(a) = p["documentUrlPatterns"].as_array() {
                    m.document_patterns = Some(MatchSet::parse(a.iter().filter_map(Value::as_str)));
                }
                Handled::With(Ok(Value::Null), vec![Effect::Changed("menus".into())])
            }
            ("menus", "remove") => {
                let id = match &arg(args, 0) {
                    Value::Number(n) => n.to_string(),
                    v => v.as_str().unwrap_or_default().to_owned(),
                };
                // remove item and its descendants
                let mut doomed = vec![id];
                let mut i = 0;
                while i < doomed.len() {
                    let p = doomed[i].clone();
                    doomed.extend(self.menus.iter().filter(|m| m.ext == ext_id && m.parent.as_deref() == Some(&p)).map(|m| m.id.clone()));
                    i += 1;
                }
                self.menus.retain(|m| !(m.ext == ext_id && doomed.contains(&m.id)));
                Handled::With(Ok(Value::Null), vec![Effect::Changed("menus".into())])
            }
            ("menus", "removeAll") => {
                self.menus.retain(|m| m.ext != ext_id);
                Handled::With(Ok(Value::Null), vec![Effect::Changed("menus".into())])
            }
            ("menus", "refresh") | ("menus", "overrideContext") => now!(Ok(Value::Null)),
            ("menus", "getTargetElement") => now!(Ok(Value::Null)),

            // ---------------- alarms ----------------
            ("alarms", "create") => {
                let (name_, info) = match (args.first(), args.get(1)) {
                    (Some(Value::String(n)), Some(i)) => (n.clone(), i.clone()),
                    (Some(Value::String(n)), None) => (n.clone(), json!({})),
                    (Some(i), _) => (String::new(), i.clone()),
                    _ => (String::new(), json!({})),
                };
                let now = now_ms() as f64;
                let period = info["periodInMinutes"].as_f64();
                let scheduled = info["when"].as_f64().or_else(|| info["delayInMinutes"].as_f64().map(|d| now + d * 60_000.0)).or_else(|| period.map(|p| now + p * 60_000.0)).unwrap_or(now);
                self.alarms.insert((ext_id.into(), name_), Alarm { scheduled, period });
                now!(Ok(Value::Null))
            }
            ("alarms", "get") => {
                let n = arg_str(args, 0);
                now!(Ok(self.alarms.get(&(ext_id.to_owned(), n.clone())).map(|a| json!({"name": n, "scheduledTime": a.scheduled, "periodInMinutes": a.period})).unwrap_or(Value::Null)))
            }
            ("alarms", "getAll") => now!(Ok(Value::Array(
                self.alarms.iter().filter(|((e, _), _)| e == ext_id).map(|((_, n), a)| json!({"name": n, "scheduledTime": a.scheduled, "periodInMinutes": a.period})).collect()
            ))),
            ("alarms", "clear") => {
                let n = arg_str(args, 0);
                now!(Ok(json!(self.alarms.remove(&(ext_id.to_owned(), n)).is_some())))
            }
            ("alarms", "clearAll") => {
                let before = self.alarms.len();
                self.alarms.retain(|(e, _), _| e != ext_id);
                now!(Ok(json!(before != self.alarms.len())))
            }

            // ---------------- permissions ----------------
            ("permissions", "getAll") => {
                let mut perms = e.manifest.permissions.clone();
                perms.extend(e.info.granted_optional.iter().filter(|p| !super::matchpattern::is_host_pattern(p)).cloned());
                now!(Ok(json!({"permissions": perms, "origins": e.host_patterns()})))
            }
            ("permissions", "contains") => {
                let p = arg(args, 0);
                let perms_ok = p["permissions"].as_array().map(|a| a.iter().filter_map(Value::as_str).all(|x| e.has_permission(x))).unwrap_or(true);
                let hosts = MatchSet::parse(e.host_patterns().iter().map(String::as_str));
                let origins_ok = p["origins"]
                    .as_array()
                    .map(|a| {
                        a.iter().filter_map(Value::as_str).all(|o| {
                            e.host_patterns().iter().any(|h| h == o || h == "<all_urls>") || {
                                let probe = o.replace("*.", "").replace('*', "x");
                                hosts.matches(&probe)
                            }
                        })
                    })
                    .unwrap_or(true);
                now!(Ok(json!(perms_ok && origins_ok)))
            }
            ("permissions", "request") => {
                let p = arg(args, 0);
                let mut wanted: Vec<String> = p["permissions"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default();
                wanted.extend(p["origins"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default());
                let optional = &e.manifest.optional_permissions;
                if let Some(bad) = wanted.iter().find(|w| !optional.contains(w) && !e.manifest.permissions.contains(w) && !e.manifest.host_permissions.contains(w)) {
                    return now!(Err(format!("Cannot request permission {bad} since it was not declared in optional_permissions")));
                }
                let missing: Vec<String> = wanted.into_iter().filter(|w| !e.has_permission(w) && !e.host_patterns().contains(w)).collect();
                if missing.is_empty() {
                    return now!(Ok(json!(true)));
                }
                let warnings = super::manifest::permission_warnings(
                    &missing.iter().filter(|m| !super::matchpattern::is_host_pattern(m)).cloned().collect::<Vec<_>>(),
                    &missing.iter().filter(|m| super::matchpattern::is_host_pattern(m)).cloned().collect::<Vec<_>>(),
                );
                self.ui(origin, ctx, call_id, ext_id, "permissions.request", json!({"name": e.manifest.name, "permissions": missing, "warnings": warnings}))
            }
            ("permissions", "remove") => {
                let p = arg(args, 0);
                let mut list: Vec<String> = p["permissions"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default();
                list.extend(p["origins"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default());
                let _ = self.registry.grant_optional(ext_id, &list, false);
                let fx = self.fire(ext_id, "permissions.onRemoved", json!([p]), false);
                Handled::With(Ok(json!(true)), fx)
            }

            // ---------------- management ----------------
            ("management", "getSelf") => now!(Ok(self.ext_info(e))),
            ("management", "getAll") => {
                if let Err(err) = self.require(e, "management") {
                    return now!(Err(err));
                }
                now!(Ok(Value::Array(self.registry.extensions.iter().map(|x| self.ext_info(x)).collect())))
            }
            ("management", "get") => {
                let id = arg_str(args, 0);
                if id != ext_id {
                    if let Err(err) = self.require(e, "management") {
                        return now!(Err(err));
                    }
                }
                now!(self.registry.get(&id).map(|x| self.ext_info(x)).ok_or_else(|| format!("No such addon {id}")))
            }
            ("management", "uninstallSelf") => self.ui(origin, ctx, call_id, ext_id, "management.uninstallSelf", arg(args, 0)),
            ("management", "getPermissionWarningsByManifest") => now!(Ok(json!(super::manifest::Manifest::parse(&arg_str(args, 0), None).map(|m| m.permission_warnings()).unwrap_or_default()))),

            // ---------------- commands ----------------
            ("commands", "getAll") => now!(Ok(Value::Array(e.manifest.commands.iter().map(|c| json!({"name": c.name, "description": c.description, "shortcut": c.shortcut})).collect()))),
            ("commands", "update") | ("commands", "reset") => now!(Ok(Value::Null)),

            // ---------------- cookies ----------------
            ("cookies", _) => {
                if let Err(err) = self.require(e, "cookies") {
                    return now!(Err(err));
                }
                let d = arg(args, 0);
                let hosts = MatchSet::parse(e.host_patterns().iter().map(String::as_str));
                match name {
                    "getAllCookieStores" => now!(Ok(json!([{"id": "firefox-default", "incognito": false, "tabIds": self.tabs.keys().collect::<Vec<_>>()}]))),
                    "get" | "getAll" => {
                        let url = d["url"].as_str();
                        if let Some(u) = url {
                            if !hosts.matches(u) {
                                return now!(Ok(if name == "get" { Value::Null } else { json!([]) }));
                            }
                        }
                        let jar = self.shared.cookies();
                        let list: Vec<Value> = jar
                            .iter()
                            .filter(|c| {
                                url.map(|u| c.matches_url(u)).unwrap_or_else(|| hosts.matches(&format!("https://{}/", c.domain.trim_start_matches('.'))))
                                    && d["name"].as_str().map(|n| n == c.name).unwrap_or(true)
                                    && d["domain"].as_str().map(|dm| {
                                        let cd = c.domain.trim_start_matches('.');
                                        let dm = dm.trim_start_matches('.');
                                        cd == dm || cd.ends_with(&format!(".{dm}"))
                                    }).unwrap_or(true)
                                    && d["path"].as_str().map(|p| p == c.path).unwrap_or(true)
                                    && d["secure"].as_bool().map(|s| s == c.secure).unwrap_or(true)
                                    && d["session"].as_bool().map(|s| s == c.session).unwrap_or(true)
                            })
                            .map(|c| c.to_api())
                            .collect();
                        if name == "get" {
                            let mut list = list;
                            list.sort_by_key(|c| std::cmp::Reverse(c["path"].as_str().map(str::len).unwrap_or(0)));
                            now!(Ok(list.into_iter().next().unwrap_or(Value::Null)))
                        } else {
                            now!(Ok(Value::Array(list)))
                        }
                    }
                    "set" | "remove" => {
                        let url = d["url"].as_str().unwrap_or_default();
                        if !hosts.matches(url) {
                            return now!(Err(format!("Permission denied to set cookie for {url}")));
                        }
                        self.ui(origin, ctx, call_id, ext_id, method, d)
                    }
                    _ => now!(Err(format!("{method} is not supported"))),
                }
            }

            // ---------------- webRequest ----------------
            ("webRequest", "_listen") => {
                let ev = arg_str(args, 0);
                let filter = arg(args, 1);
                let urls = MatchSet::parse(filter["urls"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>()).unwrap_or_default());
                let types = filter["types"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect()).unwrap_or_default();
                let blocking = arg(args, 2).as_array().is_some_and(|a| a.iter().any(|x| x == "blocking"));
                let l = WebRequestListener { event: ev, urls, types, tab_id: filter["tabId"].as_i64(), blocking };
                self.web_request.entry(ext_id.into()).or_default().push(l);
                now!(Ok(Value::Null))
            }
            ("webRequest", "_unlisten") => {
                let ev = arg_str(args, 0);
                let blocking = arg(args, 1).as_bool().unwrap_or(false);
                if let Some(v) = self.web_request.get_mut(ext_id) {
                    if let Some(i) = v.iter().position(|l| l.event == ev && l.blocking == blocking) {
                        v.remove(i);
                    }
                    if v.is_empty() {
                        self.web_request.remove(ext_id);
                    }
                }
                now!(Ok(Value::Null))
            }
            ("webRequest", "handlerBehaviorChanged") => now!(Ok(Value::Null)),
            ("webRequest", "filterResponseData") | ("webRequest", "getSecurityInfo") => now!(Err(format!("webRequest.{name} is not supported"))),

            // ---------------- webNavigation ----------------
            ("webNavigation", "getFrame") => {
                let d = arg(args, 0);
                let t = d["tabId"].as_i64().and_then(|t| self.tabs.get(&t));
                now!(Ok(t.map(|t| json!({"errorOccurred": false, "url": t["url"], "parentFrameId": -1, "frameId": 0})).unwrap_or(Value::Null)))
            }
            ("webNavigation", "getAllFrames") => {
                let d = arg(args, 0);
                let t = d["tabId"].as_i64().and_then(|t| self.tabs.get(&t));
                now!(Ok(t.map(|t| json!([{"errorOccurred": false, "url": t["url"], "parentFrameId": -1, "frameId": 0}])).unwrap_or(Value::Null)))
            }

            // ---------------- declarativeNetRequest ----------------
            ("declarativeNetRequest", _) => self.dnr_call(ext_id, name, args),

            // ---------------- history ----------------
            ("history", _) => {
                if let Err(err) = self.require(e, "history") {
                    return now!(Err(err));
                }
                self.history_call(name, args)
            }

            // ---------------- bookmarks ----------------
            ("bookmarks", _) => {
                if let Err(err) = self.require(e, "bookmarks") {
                    return now!(Err(err));
                }
                self.bookmarks_call(name, args)
            }

            // ---------------- topSites / sessions / search ----------------
            ("topSites", "get") => {
                if let Err(err) = self.require(e, "topSites") {
                    return now!(Err(err));
                }
                let n = arg(args, 0)["limit"].as_u64().unwrap_or(12) as usize;
                let sites = crate::history::top_sites(&self.shared.db(), n).unwrap_or_default();
                now!(Ok(Value::Array(sites.into_iter().map(|s| json!({"url": s.url, "title": s.title, "type": "url"})).collect())))
            }
            ("sessions", "getRecentlyClosed") => {
                if let Err(err) = self.require(e, "sessions") {
                    return now!(Err(err));
                }
                let s = self.shared.session();
                let mut out: Vec<Value> = s
                    .data
                    .closed_tabs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| json!({"lastModified": now_ms(), "tab": {"id": -1, "index": 0, "windowId": -1, "url": t.url, "title": t.title, "sessionId": format!("t{i}"), "active": false, "pinned": t.pinned, "highlighted": false, "incognito": false}}))
                    .collect();
                out.extend(s.data.closed_windows.iter().enumerate().map(|(i, w)| json!({"lastModified": now_ms(), "window": {"id": -1, "sessionId": format!("w{i}"), "tabs": w.tabs.iter().map(|t| json!({"url": t.url, "title": t.title})).collect::<Vec<_>>()}})));
                let max = arg(args, 0)["maxResults"].as_u64().unwrap_or(25) as usize;
                out.truncate(max);
                now!(Ok(Value::Array(out)))
            }
            ("sessions", "restore") => self.ui(origin, ctx, call_id, ext_id, "sessions.restore", arg(args, 0)),
            ("sessions", "setTabValue") | ("sessions", "setWindowValue") => {
                let key = format!("__kite_session_{}_{}_{}", name, arg(args, 0), arg_str(args, 1));
                let _ = self.storage.set(ext_id, "session", &json!({key: arg(args, 2)}));
                now!(Ok(Value::Null))
            }
            ("sessions", "getTabValue") | ("sessions", "getWindowValue") => {
                let key = format!("__kite_session_{}_{}_{}", name.replace("get", "set"), arg(args, 0), arg_str(args, 1));
                now!(Ok(self.storage.get(ext_id, "session", &json!(key.clone())).ok().and_then(|v| v.get(&key).cloned()).unwrap_or(Value::Null)))
            }
            ("sessions", "removeTabValue") | ("sessions", "removeWindowValue") => {
                let key = format!("__kite_session_{}_{}_{}", name.replace("remove", "set"), arg(args, 0), arg_str(args, 1));
                let _ = self.storage.remove(ext_id, "session", &json!(key));
                now!(Ok(Value::Null))
            }
            ("search", "get") => {
                let engines = crate::search::load_engines(&self.shared.db()).unwrap_or_default();
                let def = self.shared.settings().get_str("general.searchEngine");
                now!(Ok(Value::Array(engines.into_iter().map(|en| json!({"name": en.name, "isDefault": en.name == def, "alias": en.shortcut})).collect())))
            }
            ("search", "search") | ("search", "query") => self.ui(origin, ctx, call_id, ext_id, "search.search", arg(args, 0)),

            // ---------------- notifications ----------------
            ("notifications", "create") => {
                let (id, opts) = match (args.first(), args.get(1)) {
                    (Some(Value::String(id)), Some(o)) => (id.clone(), o.clone()),
                    (Some(o), _) if o.is_object() => (token(), o.clone()),
                    (_, Some(o)) => (token(), o.clone()),
                    _ => (token(), json!({})),
                };
                let mut opts = opts;
                if let Some(icon) = opts["iconUrl"].as_str() {
                    if !icon.contains(':') {
                        opts["iconUrl"] = e.url(icon).into();
                    }
                } else {
                    opts["iconUrl"] = json!(e.icon_url(64));
                }
                self.notifications.entry(ext_id.into()).or_default().push(id.clone());
                self.ui(origin, ctx, call_id, ext_id, "notifications.create", json!({"id": id, "options": opts, "name": e.manifest.name}))
            }
            ("notifications", "clear") => {
                let id = arg_str(args, 0);
                let existed = self.notifications.get_mut(ext_id).map(|l| {
                    let n = l.len();
                    l.retain(|x| *x != id);
                    n != l.len()
                });
                Handled::With(Ok(json!(existed.unwrap_or(false))), vec![])
            }
            ("notifications", "getAll") => now!(Ok(Value::Object(self.notifications.get(ext_id).map(|l| l.iter().map(|i| (i.clone(), json!({}))).collect()).unwrap_or_default()))),
            ("notifications", "update") => now!(Ok(json!(false))),

            // ---------------- downloads ----------------
            ("downloads", _) => {
                if let Err(err) = self.require(e, "downloads") {
                    return now!(Err(err));
                }
                match name {
                    "search" => {
                        let q = arg(args, 0);
                        let d = self.shared.downloads();
                        let list: Vec<Value> = d
                            .items
                            .iter()
                            .filter(|i| q["id"].as_u64().map(|x| x == i.id).unwrap_or(true) && q["url"].as_str().map(|u| u == i.url).unwrap_or(true) && q["state"].as_str().map(|s| s == i.state).unwrap_or(true))
                            .map(download_json)
                            .collect();
                        now!(Ok(Value::Array(list)))
                    }
                    "erase" => {
                        let q = arg(args, 0);
                        let ids: Vec<u64> = {
                            let mut d = self.shared.downloads();
                            let ids: Vec<u64> = d.items.iter().filter(|i| q["id"].as_u64().map(|x| x == i.id).unwrap_or(true) && i.state != "in_progress").map(|i| i.id).collect();
                            for id in &ids {
                                d.remove(*id);
                            }
                            ids
                        };
                        let fx: Vec<Effect> = ids.iter().flat_map(|id| self.fire_all("downloads.onErased", |e| e.has_permission("downloads").then(|| json!([id])))).collect();
                        Handled::With(Ok(json!(ids)), fx)
                    }
                    "getFileIcon" => now!(Ok(Value::Null)),
                    _ => self.ui(origin, ctx, call_id, ext_id, method, Value::Array(args.to_vec())),
                }
            }

            // ---------------- misc ----------------
            ("idle", "queryState") => now!(Ok(json!("active"))),
            ("idle", "setDetectionInterval") => now!(Ok(Value::Null)),
            ("theme", "getCurrent") => now!(Ok(json!({"colors": null, "images": null, "properties": null}))),
            ("theme", "update") | ("theme", "reset") => now!(Ok(Value::Null)),
            ("browserSettings" | "privacy" | "proxy", _) => now!(Ok(json!({"value": Value::Null, "levelOfControl": "not_controllable"}))),
            ("omnibox", "setDefaultSuggestion") => now!(Ok(Value::Null)),
            ("clipboard", "setImageData") => self.ui(origin, ctx, call_id, ext_id, "clipboard.setImageData", Value::Array(args.to_vec())),
            ("browsingData", _) => {
                if let Err(err) = self.require(e, "browsingData") {
                    return now!(Err(err));
                }
                if name == "settings" {
                    return now!(Ok(json!({"options": {"since": 0}, "dataToRemove": {"history": true, "cookies": true, "cache": true, "downloads": true}, "dataRemovalPermitted": {"history": true, "cookies": true, "cache": true, "downloads": true}})));
                }
                self.ui(origin, ctx, call_id, ext_id, method, Value::Array(args.to_vec()))
            }
            ("identity", "launchWebAuthFlow") => self.ui(origin, ctx, call_id, ext_id, method, arg(args, 0)),
            ("tabs", "warmup") | ("tabs", "hide") | ("tabs", "show") => now!(Ok(Value::Null)),
            _ => now!(Err(format!("{method} is not supported by Kite yet"))),
        }
    }

    fn can_inject(&self, e: &Extension, url: &str) -> bool {
        if url.starts_with(&e.base_url()) {
            return true;
        }
        if url.starts_with("kite:") || url.starts_with("about:") || url.starts_with("moz-extension:") || url.contains("addons.mozilla.org") {
            return false;
        }
        e.has_permission("activeTab") || MatchSet::parse(e.host_patterns().iter().map(String::as_str)).matches(url)
    }

    fn ext_info(&self, x: &Extension) -> Value {
        json!({
            "id": x.info.id, "name": x.manifest.name, "shortName": x.manifest.short_name, "description": x.manifest.description,
            "version": x.manifest.version, "enabled": x.info.enabled, "type": "extension", "mayDisable": true,
            "installType": if x.info.source == "temporary" { "development" } else { "normal" },
            "homepageUrl": x.manifest.homepage_url, "optionsUrl": x.manifest.options_page.as_ref().map(|p| x.url(p)).unwrap_or_default(),
            "permissions": x.manifest.permissions, "hostPermissions": x.host_patterns(),
            "icons": x.manifest.icons.iter().map(|(s, p)| json!({"size": s, "url": x.url(p)})).collect::<Vec<_>>(),
        })
    }

    fn dnr_call(&mut self, ext_id: &str, name: &str, args: &[Value]) -> Handled {
        let d = arg(args, 0);
        let set = self.dnr.entry(ext_id.into()).or_default();
        match name {
            "updateDynamicRules" | "updateSessionRules" => {
                let dynamic = name == "updateDynamicRules";
                let remove: Vec<i64> = d["removeRuleIds"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
                let (add, errors) = dnr::parse_rules(d.get("addRules").unwrap_or(&json!([])));
                if !errors.is_empty() {
                    return Handled::Now(Err(errors.join("; ")));
                }
                let list = if dynamic { &mut set.dynamic_rules } else { &mut set.session_rules };
                list.retain(|r| !remove.contains(&r.id));
                if let Some(dup) = add.iter().find(|r| list.iter().any(|x| x.id == r.id)) {
                    return Handled::Now(Err(format!("Duplicate rule ID: {}", dup.id)));
                }
                list.extend(add);
                if dynamic {
                    // Persist the raw rule JSON for the next session.
                    let mut raw: Vec<Value> = self
                        .storage
                        .get(ext_id, "local", &json!("__kite_dnr_dynamic"))
                        .ok()
                        .and_then(|v| v.get("__kite_dnr_dynamic").and_then(Value::as_array).cloned())
                        .unwrap_or_default();
                    raw.retain(|r| !r["id"].as_i64().is_some_and(|id| remove.contains(&id)));
                    raw.extend(d["addRules"].as_array().cloned().unwrap_or_default());
                    let _ = self.storage.set(ext_id, "local", &json!({"__kite_dnr_dynamic": raw}));
                } else {
                    let mut raw: Vec<Value> = self
                        .storage
                        .get(ext_id, "session", &json!("__kite_dnr_session"))
                        .ok()
                        .and_then(|v| v.get("__kite_dnr_session").and_then(Value::as_array).cloned())
                        .unwrap_or_default();
                    raw.retain(|r| !r["id"].as_i64().is_some_and(|id| remove.contains(&id)));
                    raw.extend(d["addRules"].as_array().cloned().unwrap_or_default());
                    let _ = self.storage.set(ext_id, "session", &json!({"__kite_dnr_session": raw}));
                }
                Handled::Now(Ok(Value::Null))
            }
            "getDynamicRules" | "getSessionRules" => {
                let (area, key) = if name == "getDynamicRules" { ("local", "__kite_dnr_dynamic") } else { ("session", "__kite_dnr_session") };
                let raw = self.storage.get(ext_id, area, &json!(key)).ok().and_then(|v| v.get(key).cloned()).unwrap_or(json!([]));
                Handled::Now(Ok(raw))
            }
            "updateEnabledRulesets" => {
                let dis = self.dnr_disabled.entry(ext_id.into()).or_default();
                for id in d["disableRulesetIds"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    dis.retain(|x| x != &format!("+{id}"));
                    dis.push(id.into());
                }
                for id in d["enableRulesetIds"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    dis.retain(|x| x != id);
                    dis.push(format!("+{id}"));
                }
                self.load_dnr(ext_id);
                Handled::Now(Ok(Value::Null))
            }
            "getEnabledRulesets" => {
                let dis = self.dnr_disabled.get(ext_id).cloned().unwrap_or_default();
                let ids: Vec<String> = self
                    .registry
                    .get(ext_id)
                    .map(|e| e.manifest.dnr_rulesets.iter().filter(|(id, en, _)| (*en && !dis.contains(id)) || dis.contains(&format!("+{id}"))).map(|(id, _, _)| id.clone()).collect())
                    .unwrap_or_default();
                Handled::Now(Ok(json!(ids)))
            }
            "isRegexSupported" => Handled::Now(Ok(json!({"isSupported": regex::Regex::new(d["regex"].as_str().unwrap_or("")).is_ok()}))),
            "getAvailableStaticRuleCount" => Handled::Now(Ok(json!(330_000))),
            "setExtensionActionOptions" | "getMatchedRules" => Handled::Now(Ok(json!({"rulesMatchedInfo": []}))),
            "testMatchOutcome" => {
                let req = dnr::Request::new(d["url"].as_str().unwrap_or(""), d["initiator"].as_str().unwrap_or(""), d["method"].as_str().unwrap_or("get"), d["type"].as_str().unwrap_or("other"));
                let o = set.evaluate(&req);
                Handled::Now(Ok(json!({"matchedRules": if o == dnr::Outcome::None { json!([]) } else { json!([{"ruleId": 0, "rulesetId": "_dynamic"}]) }})))
            }
            _ => Handled::Now(Err(format!("declarativeNetRequest.{name} is not supported"))),
        }
    }

    fn history_call(&mut self, name: &str, args: &[Value]) -> Handled {
        let db = self.shared.db();
        let d = arg(args, 0);
        let r: R = match name {
            "search" => {
                let max = d["maxResults"].as_u64().unwrap_or(100) as usize;
                let start = d["startTime"].as_f64().map(|v| v as i64).unwrap_or_else(|| if d["text"].as_str().unwrap_or("").is_empty() { now_ms() - 24 * 3600 * 1000 } else { 0 });
                let end = d["endTime"].as_f64().map(|v| v as i64).unwrap_or(0);
                crate::history::search(&db, d["text"].as_str().unwrap_or(""), start, end, max)
                    .map(|v| Value::Array(v.into_iter().map(|h| json!({"id": h.id.to_string(), "url": h.url, "title": h.title, "lastVisitTime": h.date, "visitCount": h.count, "typedCount": 0})).collect()))
                    .map_err(|e| e.to_string())
            }
            "getVisits" => Ok(crate::history::get(&db, d["url"].as_str().unwrap_or(""))
                .ok()
                .flatten()
                .map(|h| json!([{"id": h.id.to_string(), "visitId": h.id.to_string(), "visitTime": h.date, "referringVisitId": "-1", "transition": "link"}]))
                .unwrap_or(json!([]))),
            "addUrl" => crate::history::add_visit(&db, d["url"].as_str().unwrap_or(""), d["title"].as_str().unwrap_or("")).map(|_| Value::Null).map_err(|e| e.to_string()),
            "deleteUrl" => {
                let u = d["url"].as_str().unwrap_or("").to_owned();
                let r = crate::history::delete_url(&db, &u).map(|_| Value::Null).map_err(|e| e.to_string());
                drop(db);
                let fx = self.browser_event("historyRemoved", &json!({"allHistory": false, "urls": [u]}));
                return Handled::With(r, [fx, vec![Effect::Changed("history".into())]].concat());
            }
            "deleteRange" => crate::history::delete_range(&db, d["startTime"].as_f64().unwrap_or(0.0) as i64, d["endTime"].as_f64().unwrap_or(0.0) as i64).map(|_| Value::Null).map_err(|e| e.to_string()),
            "deleteAll" => {
                let r = crate::history::clear(&db).map(|_| Value::Null).map_err(|e| e.to_string());
                drop(db);
                let fx = self.browser_event("historyRemoved", &json!({"allHistory": true, "urls": []}));
                return Handled::With(r, [fx, vec![Effect::Changed("history".into())]].concat());
            }
            _ => Err(format!("history.{name} is not supported")),
        };
        Handled::Now(r)
    }

    fn bookmarks_call(&mut self, name: &str, args: &[Value]) -> Handled {
        use crate::bookmarks::{Kind, Node};
        let id_of = |v: &Value| -> Option<u64> {
            match v {
                Value::String(s) => s.parse().ok(),
                Value::Number(n) => n.as_u64(),
                _ => None,
            }
        };
        let mut b = self.shared.bookmarks();
        let r: Result<(Value, Option<Value>), String> = (|| match name {
            "getTree" => Ok((json!([b.to_json(&b.root, None, 0, true)]), None)),
            "get" => {
                let ids: Vec<Value> = match arg(args, 0) {
                    Value::Array(a) => a,
                    v => vec![v],
                };
                let out: Option<Vec<Value>> = ids.iter().map(|i| id_of(i).and_then(|i| b.node_json(i, false))).map(|v| v.map(|mut v| { v.as_object_mut().map(|o| o.remove("children")); v })).collect();
                out.map(|o| (Value::Array(o), None)).ok_or_else(|| "Bookmark not found".into())
            }
            "getChildren" => {
                let id = id_of(&arg(args, 0)).ok_or("invalid id")?;
                let n = b.node_json(id, true).ok_or("Bookmark not found")?;
                let children = n["children"].as_array().cloned().unwrap_or_default().into_iter().map(|mut c| { c.as_object_mut().map(|o| o.insert("children".into(), json!([]))); if c["type"] != "folder" { c.as_object_mut().map(|o| o.remove("children")); } c }).collect();
                Ok((Value::Array(children), None))
            }
            "getSubTree" => {
                let id = id_of(&arg(args, 0)).ok_or("invalid id")?;
                Ok((json!([b.node_json(id, true).ok_or("Bookmark not found")?]), None))
            }
            "getRecent" => {
                let n = arg_i64(args, 0).unwrap_or(10) as usize;
                let mut all: Vec<Value> = vec![];
                fn walk(b: &crate::bookmarks::Bookmarks, n: &Node, out: &mut Vec<Value>) {
                    if n.kind == Kind::Url {
                        if let Some(v) = b.node_json(n.id, false) {
                            out.push(v);
                        }
                    }
                    for c in &n.children {
                        walk(b, c, out);
                    }
                }
                walk(&b, &b.root, &mut all);
                all.sort_by_key(|v| std::cmp::Reverse(v["dateAdded"].as_i64().unwrap_or(0)));
                all.truncate(n);
                Ok((Value::Array(all), None))
            }
            "search" => {
                let q = arg(args, 0);
                let text = match &q {
                    Value::String(s) => s.clone(),
                    o => o["query"].as_str().or(o["title"].as_str()).or(o["url"].as_str()).unwrap_or("").to_owned(),
                };
                let ids: Vec<u64> = b.search(&text).iter().map(|n| n.id).collect();
                Ok((Value::Array(ids.into_iter().filter_map(|i| b.node_json(i, false)).collect()), None))
            }
            "create" => {
                let d = arg(args, 0);
                let parent = d.get("parentId").and_then(id_of).unwrap_or(b.unsorted_id());
                let node = match (d["type"].as_str(), d["url"].as_str()) {
                    (Some("separator"), _) => Node::separator(),
                    (Some("folder"), _) | (None, None) => Node::folder(d["title"].as_str().unwrap_or("")),
                    (_, Some(u)) => Node::url(d["title"].as_str().unwrap_or(""), u),
                    _ => Node::folder(d["title"].as_str().unwrap_or("")),
                };
                let id = b.insert(parent, d["index"].as_u64().map(|i| i as usize), node).map_err(|e| e.to_string())?;
                let j = b.node_json(id, false).unwrap();
                Ok((j.clone(), Some(json!({"name": "onCreated", "args": [id.to_string(), j]}))))
            }
            "update" => {
                let id = id_of(&arg(args, 0)).ok_or("invalid id")?;
                let ch = arg(args, 1);
                b.update(id, ch["title"].as_str(), ch["url"].as_str(), None, None).map_err(|e| e.to_string())?;
                let j = b.node_json(id, false).unwrap();
                Ok((j, Some(json!({"name": "onChanged", "args": [id.to_string(), ch]}))))
            }
            "move" => {
                let id = id_of(&arg(args, 0)).ok_or("invalid id")?;
                let dest = arg(args, 1);
                let (old_parent, old_index) = b.parent_of(id).ok_or("Bookmark not found")?;
                let parent = dest.get("parentId").and_then(id_of).unwrap_or(old_parent);
                b.move_to(id, parent, dest["index"].as_u64().map(|i| i as usize)).map_err(|e| e.to_string())?;
                let j = b.node_json(id, false).unwrap();
                let info = json!({"parentId": parent.to_string(), "index": j["index"], "oldParentId": old_parent.to_string(), "oldIndex": old_index});
                Ok((j, Some(json!({"name": "onMoved", "args": [id.to_string(), info]}))))
            }
            "remove" | "removeTree" => {
                let id = id_of(&arg(args, 0)).ok_or("invalid id")?;
                let (parent, index) = b.parent_of(id).ok_or("Bookmark not found")?;
                if name == "remove" && b.find(id).is_some_and(|n| n.kind == Kind::Folder && !n.children.is_empty()) {
                    return Err("Cannot remove a non-empty folder".into());
                }
                let node = b.node_json(id, true).unwrap();
                b.remove(id).map_err(|e| e.to_string())?;
                Ok((Value::Null, Some(json!({"name": "onRemoved", "args": [id.to_string(), {"parentId": parent.to_string(), "index": index, "node": node}]}))))
            }
            _ => Err(format!("bookmarks.{name} is not supported")),
        })();
        drop(b);
        match r {
            Ok((v, Some(ev))) => {
                let fx = self.browser_event("bookmarkEvent", &ev);
                Handled::With(Ok(v), fx)
            }
            Ok((v, None)) => Handled::Now(Ok(v)),
            Err(e) => Handled::Now(Err(e)),
        }
    }
}

/// Origin as browsers report it in `MessageSender.origin`. The WHATWG URL
/// spec gives non-special schemes (like moz-extension:) an opaque "null"
/// origin, but Firefox reports `moz-extension://<uuid>`, and extensions such
/// as uBlock Origin rely on that to recognise their own pages.
pub fn origin_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?;
    Some(match u.port() {
        Some(p) => format!("{}://{host}:{p}", u.scheme()),
        None => format!("{}://{host}", u.scheme()),
    })
}

pub fn download_json(i: &crate::downloads::DownloadRecord) -> Value {
    json!({
        "id": i.id, "url": i.url, "filename": i.path, "mime": i.mime, "totalBytes": i.total, "bytesReceived": i.received,
        "fileSize": i.total, "state": i.state, "paused": false, "canResume": false, "exists": std::path::Path::new(&i.path).exists(),
        "startTime": iso_time(i.started), "endTime": if i.finished > 0 { json!(iso_time(i.finished)) } else { Value::Null },
        "incognito": false, "danger": "safe",
    })
}

/// ISO-8601 UTC time for an epoch-ms timestamp.
pub fn iso_time(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, ms.rem_euclid(1000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webext::xpi::make_xpi;

    fn setup() -> (tempfile::TempDir, Runtime, String) {
        let dir = tempfile::tempdir().unwrap();
        let shared = Shared::temporary(dir.path()).unwrap();
        let mut rt = Runtime::new(shared).unwrap();
        let xpi = make_xpi(&[(
            "manifest.json",
            r#"{"manifest_version":2,"name":"T","version":"1","browser_specific_settings":{"gecko":{"id":"t@x"}},
               "permissions":["storage","tabs","menus","alarms","<all_urls>"],"browser_action":{"default_title":"Tee"},
               "background":{"scripts":["bg.js"]}}"#,
        )]);
        rt.registry.install_xpi(&xpi, "file", "").unwrap();
        rt.on_installed("t@x", false, "", false);
        let tok = rt.token_of("t@x").unwrap().to_owned();
        (dir, rt, tok)
    }

    fn reply(fx: &[Effect]) -> Result<Value, String> {
        fx.iter()
            .find_map(|e| match e {
                Effect::Reply { result, .. } => Some(result.clone()),
                _ => None,
            })
            .expect("a reply")
    }

    #[test]
    fn detects_content_blocking_extensions() {
        let (_d, mut rt, tok) = setup();
        let bg = Origin { view: "bg:t@x".into(), tab_id: -1, window_id: -1 };
        let listen = |rt: &mut Runtime, filter: Value, extra: Value| rt.call(&bg, &tok, "bg", "webRequest._listen", json!(["onBeforeRequest", filter, extra]), 1);
        assert_eq!(rt.content_blocker(), None);
        // Observing, or blocking only some sites, isn't content blocking.
        listen(&mut rt, json!({"urls": ["<all_urls>"]}), json!([]));
        listen(&mut rt, json!({"urls": ["*://*.example.com/*"]}), json!(["blocking"]));
        assert_eq!(rt.content_blocker(), None);
        // uBlock Origin's listener.
        listen(&mut rt, json!({"urls": ["http://*/*", "https://*/*"]}), json!(["blocking"]));
        assert_eq!(rt.content_blocker(), Some("T"));
        rt.call(&bg, &tok, "bg", "webRequest._unlisten", json!(["onBeforeRequest", true]), 2);
        rt.call(&bg, &tok, "bg", "webRequest._unlisten", json!(["onBeforeRequest", true]), 3);
        assert_eq!(rt.content_blocker(), None);
        // A large declarativeNetRequest ruleset (uBO Lite and other MV3 blockers).
        let rules: Vec<Value> = (1..=1000).map(|i| json!({"id": i, "action": {"type": "block"}, "condition": {"urlFilter": format!("ads{i}.")}})).collect();
        rt.dnr.get_mut("t@x").unwrap().dynamic_rules = dnr::parse_rules(&json!(rules)).0;
        assert_eq!(rt.content_blocker(), Some("T"));
    }

    #[test]
    fn auth_and_storage() {
        let (_d, mut rt, tok) = setup();
        let o = Origin { view: "bg:t@x".into(), tab_id: -1, window_id: -1 };
        assert!(reply(&rt.call(&o, "bogus", "c1", "storage.local.get", json!([null]), 1)).is_err());
        rt.call(&o, &tok, "c1", "_.register", json!([{"kind": "background", "url": "moz-extension://x/bg.html"}]), 1);
        let changed = |fx: &[Effect]| fx.iter().any(|e| matches!(e, Effect::Deliver { payload, .. } if payload["name"] == "storage.onChanged"));
        // Nobody listens: no event (its payload can be megabytes).
        let fx = rt.call(&o, &tok, "c1", "storage.local.set", json!([{"k": 0}]), 2);
        assert_eq!(reply(&fx), Ok(Value::Null));
        assert!(!changed(&fx));
        // A listener: the event comes.
        rt.call(&o, &tok, "c1", "_.listen", json!(["storage.onChanged", true]), 2);
        let fx = rt.call(&o, &tok, "c1", "storage.local.set", json!([{"k": 1}]), 2);
        assert!(changed(&fx));
        // A listener announced before the context registered counts too.
        rt.call(&o, &tok, "c2", "_.listen", json!(["storage.local.onChanged", true]), 2);
        rt.call(&o, &tok, "c2", "_.register", json!([{"kind": "popup", "url": "moz-extension://x/p.html"}]), 1);
        let fx = rt.call(&o, &tok, "c1", "storage.local.set", json!([{"k": 2}]), 2);
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { payload, ctxs, .. } if payload["name"] == "storage.local.onChanged" && ctxs == &vec!["c2".to_string()])));
        rt.call(&o, &tok, "c1", "_.listen", json!(["storage.onChanged", false]), 2);
        assert!(!changed(&rt.call(&o, &tok, "c1", "storage.local.set", json!([{"k": 1}]), 2)));
        assert_eq!(reply(&rt.call(&o, &tok, "c1", "storage.local.get", json!(["k"]), 3)), Ok(json!({"k": 1})));
    }

    #[test]
    fn messaging_and_ports() {
        let (_d, mut rt, tok) = setup();
        let bg = Origin { view: "bg:t@x".into(), tab_id: -1, window_id: 1 };
        let tab = Origin { view: "tab:5".into(), tab_id: 5, window_id: 1 };
        rt.browser_event("tabCreated", &json!({"tab": {"id": 5, "windowId": 1, "index": 0, "url": "https://kde.org/", "title": "KDE", "active": true}}));
        rt.call(&bg, &tok, "bg", "_.register", json!([{"kind": "background"}]), 1);
        rt.call(&bg, &tok, "bg", "_.ready", json!([]), 1);
        rt.call(&tab, &tok, "cs", "_.register", json!([{"kind": "content", "url": "https://kde.org/"}]), 1);
        // content -> background
        let fx = rt.call(&tab, &tok, "cs", "runtime.sendMessage", json!(["", {"hi": 1}]), 7);
        let (msg_id, sender) = fx
            .iter()
            .find_map(|e| match e {
                Effect::Deliver { view, payload, .. } if view == "bg:t@x" => Some((payload["msgId"].as_i64().unwrap(), payload["sender"].clone())),
                _ => None,
            })
            .unwrap();
        assert_eq!(sender["tab"]["id"], 5);
        let fx = rt.call(&bg, &tok, "bg", "_.respond", json!([msg_id, "pong", true]), 2);
        assert!(fx.iter().any(|e| matches!(e, Effect::Reply { ctx, call_id: 7, result: Ok(v), .. } if ctx == "cs" && v == "pong")));
        // background -> tab with no listener answering
        let fx = rt.call(&bg, &tok, "bg", "tabs.sendMessage", json!([5, "x"]), 3);
        let id = fx.iter().find_map(|e| if let Effect::Deliver { payload, .. } = e { payload["msgId"].as_i64() } else { None }).unwrap();
        let fx = rt.call(&tab, &tok, "cs", "_.respond", json!([id, null, false]), 4);
        assert!(fx.iter().any(|e| matches!(e, Effect::Reply { call_id: 3, result: Ok(Value::Null), .. })));
        // nobody there
        assert_eq!(reply(&rt.call(&bg, &tok, "bg", "tabs.sendMessage", json!([99, "x"]), 5)), Err(NO_RECEIVER.into()));
        // ports
        let fx = rt.call(&tab, &tok, "cs", "runtime.connect", json!(["p1", "", {"name": "n"}]), 6);
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { payload, .. } if payload["type"] == "connect" && payload["name"] == "n")));
        let fx = rt.call(&bg, &tok, "bg", "_.portPost", json!(["p1", 42]), 7);
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { view, payload, .. } if view == "tab:5" && payload["message"] == 42)));
        let fx = rt.view_closed("tab:5");
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { view, payload, .. } if view == "bg:t@x" && payload["type"] == "portDisconnect")));
    }

    #[test]
    fn tabs_actions_menus_alarms() {
        let (_d, mut rt, tok) = setup();
        let bg = Origin { view: "bg:t@x".into(), tab_id: -1, window_id: -1 };
        rt.call(&bg, &tok, "bg", "_.register", json!([{"kind": "background"}]), 1);
        rt.browser_event("windowCreated", &json!({"window": {"id": 1, "type": "normal", "state": "normal"}}));
        rt.browser_event("windowFocused", &json!({"windowId": 1}));
        for (id, active) in [(1, false), (2, true)] {
            rt.browser_event("tabCreated", &json!({"tab": {"id": id, "windowId": 1, "index": id - 1, "url": format!("https://s{id}.test/"), "title": "t", "active": active}}));
        }
        let r = reply(&rt.call(&bg, &tok, "bg", "tabs.query", json!([{"active": true, "currentWindow": true}]), 2)).unwrap();
        assert_eq!(r.as_array().unwrap().len(), 1);
        assert_eq!(r[0]["id"], 2);
        let r = reply(&rt.call(&bg, &tok, "bg", "tabs.query", json!([{"url": "*://s1.test/*"}]), 2)).unwrap();
        assert_eq!(r[0]["id"], 1);
        // tabs.create goes to the UI
        let fx = rt.call(&bg, &tok, "bg", "tabs.create", json!([{"url": "page.html"}]), 3);
        assert!(matches!(&fx[0], Effect::Ui { method, args, .. } if method == "tabs.create" && args["url"].as_str().unwrap().starts_with("moz-extension://")));
        // badge
        rt.call(&bg, &tok, "bg", "browserAction.setBadgeText", json!([{"text": "5"}]), 4);
        rt.call(&bg, &tok, "bg", "action.setBadgeText", json!([{"text": "9", "tabId": 2}]), 5);
        assert_eq!(rt.toolbar_json(1)[0]["badgeText"], "5");
        assert_eq!(rt.toolbar_json(2)[0]["badgeText"], "9");
        assert_eq!(rt.toolbar_json(2)[0]["title"], "Tee");
        // menus
        rt.call(&bg, &tok, "bg", "menus.create", json!([{"id": "a", "title": "Look up %s", "contexts": ["selection"]}]), 6);
        let m = rt.menus_json(&json!({"contexts": ["selection", "page"], "pageUrl": "https://x/", "selectionText": "kde"}));
        assert_eq!(m[0]["title"], "Look up kde");
        assert_eq!(rt.menus_json(&json!({"contexts": ["page"]})).as_array().unwrap().len(), 0);
        let fx = rt.menu_clicked("t@x", "a", &json!({"selectionText": "kde"}), 2);
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { payload, .. } if payload["name"] == "menus.onClicked")));
        // alarms
        rt.call(&bg, &tok, "bg", "alarms.create", json!(["a", {"when": 0}]), 7);
        let fx = rt.tick();
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { payload, .. } if payload["name"] == "alarms.onAlarm")));
        assert!(rt.tick().is_empty());
        // unsupported API rejects instead of hanging
        assert!(reply(&rt.call(&bg, &tok, "bg", "foo.bar", json!([]), 8)).is_err());
    }

    #[test]
    fn waits_for_background_page() {
        let (_d, mut rt, tok) = setup();
        let bg = Origin { view: "bg:t@x".into(), tab_id: -1, window_id: -1 };
        let popup = Origin { view: "popup:t@x".into(), tab_id: -1, window_id: -1 };
        rt.call(&popup, &tok, "pp", "_.register", json!([{"kind": "popup"}]), 1);
        // Another extension page is open (it must not swallow the messages).
        rt.call(&popup, &tok, "pp2", "_.register", json!([{"kind": "tab"}]), 1);
        // Background page exists but has not finished loading yet.
        rt.call(&bg, &tok, "bg", "_.register", json!([{"kind": "background"}]), 1);
        let fx = rt.call(&popup, &tok, "pp", "runtime.connect", json!(["P1", "", {"name": "n"}]), 2);
        assert!(!fx.iter().any(|e| matches!(e, Effect::Deliver { .. })), "connect must wait: {fx:?}");
        let fx = rt.call(&popup, &tok, "pp", "runtime.sendMessage", json!(["", "hi"]), 3);
        assert!(fx.is_empty(), "message must wait: {fx:?}");
        // Background finished loading: both are delivered.
        let fx = rt.call(&bg, &tok, "bg", "_.ready", json!([]), 4);
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { view, payload, .. } if view == "bg:t@x" && payload["type"] == "connect")));
        assert!(fx.iter().any(|e| matches!(e, Effect::Deliver { view, payload, .. } if view == "bg:t@x" && payload["type"] == "message")));
    }

    #[test]
    fn sender_origins() {
        assert_eq!(origin_of("moz-extension://abc-123/popup.html?x").as_deref(), Some("moz-extension://abc-123"));
        assert_eq!(origin_of("https://kde.org:8443/a").as_deref(), Some("https://kde.org:8443"));
        assert_eq!(origin_of("about:blank"), None);
    }

    #[test]
    fn iso() {
        assert_eq!(iso_time(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_time(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
    }
}
