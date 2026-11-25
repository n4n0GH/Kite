//! `Extensions` — QML singleton hosting the WebExtension runtime: API call
//! routing between extension contexts and the Rust runtime, add-on
//! installation from addons.mozilla.org / files, and add-on management.

use crate::state::app;
use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use kite_core::webext::{amo, registry::Registry, runtime::Effect, runtime::Origin, scripts};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::OnceLock;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, revision)]
        type Extensions = super::ExtensionsRust;

        // ---- API plumbing ----
        #[qinvokable]
        /// `document`: the address of the page the view shows, from the
        /// engine (qml/extcall.js, from the bridge's host). It decides whether the caller is
        /// one of the extension's pages or a content script.
        fn call(self: Pin<&mut Extensions>, view: &QString, tab_id: i32, window_id: i32, document: &QString, token: &QString, ctx: &QString, method: &QString, args: &QString, call_id: i32);
        #[qinvokable]
        #[cxx_name = "uiReply"]
        fn ui_reply(self: Pin<&mut Extensions>, view: &QString, ctx: &QString, call_id: i32, ok: bool, json: &QString);
        #[qinvokable]
        #[cxx_name = "browserEvent"]
        fn browser_event(self: Pin<&mut Extensions>, name: &QString, json: &QString);
        #[qinvokable]
        #[cxx_name = "viewClosed"]
        fn view_closed(self: Pin<&mut Extensions>, view: &QString);
        #[qinvokable]
        fn tick(self: Pin<&mut Extensions>);
        /// Messages for `view` are waiting: its bridge takes them with
        /// takeDelivery(), so only the view's own bridge converts them,
        /// however many views listen.
        #[qsignal]
        #[cxx_name = "deliveryReady"]
        fn delivery_ready(self: Pin<&mut Extensions>, view: QString);
        /// The next message for `view`, or "" when there is none.
        #[qinvokable]
        #[cxx_name = "takeDelivery"]
        fn take_delivery(self: Pin<&mut Extensions>, view: &QString) -> QString;
        #[qsignal]
        #[cxx_name = "uiRequest"]
        fn ui_request(self: Pin<&mut Extensions>, view: QString, ctx: QString, call_id: i32, ext: QString, method: QString, args: QString);
        #[qsignal]
        fn changed(self: Pin<&mut Extensions>, what: QString);

        // ---- UI integration ----
        #[qinvokable]
        fn toolbar(self: &Extensions, tab_id: i32, in_private: bool) -> QString;
        #[qinvokable]
        fn sidebars(self: &Extensions, in_private: bool) -> QString;
        #[qinvokable]
        fn menus(self: &Extensions, info: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "menuClicked"]
        fn menu_clicked(self: Pin<&mut Extensions>, ext: &QString, item: &QString, info: &QString, tab_id: i32);
        #[qinvokable]
        #[cxx_name = "actionClicked"]
        fn action_clicked(self: Pin<&mut Extensions>, ext: &QString, kind: &QString, tab_id: i32);
        #[qinvokable]
        fn commands(self: &Extensions, in_private: bool) -> QString;
        #[qinvokable]
        fn command(self: Pin<&mut Extensions>, ext: &QString, name: &QString, tab_id: i32);
        #[qinvokable]
        #[cxx_name = "userScripts"]
        fn user_scripts(self: &Extensions, in_private: bool) -> QString;
        #[qinvokable]
        #[cxx_name = "backgroundPages"]
        fn background_pages(self: &Extensions) -> QString;
        #[qinvokable]
        #[cxx_name = "newTabOverride"]
        fn new_tab_override(self: &Extensions, in_private: bool) -> QString;
        #[qinvokable]
        #[cxx_name = "extensionNameForUrl"]
        fn extension_name_for_url(self: &Extensions, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "omniboxKeywords"]
        fn omnibox_keywords(self: &Extensions, in_private: bool) -> QString;

        /// False while extensions that block requests are still starting;
        /// tabs hold their first load until then.
        #[qinvokable]
        #[cxx_name = "networkReady"]
        fn network_ready(self: &Extensions) -> bool;

        // ---- management ----
        #[qinvokable]
        fn list(self: &Extensions) -> QString;
        /// Let the extension run in private windows (saved with it), or not.
        #[qinvokable]
        #[cxx_name = "setAllowPrivate"]
        fn set_allow_private(self: Pin<&mut Extensions>, id: &QString, allow: bool);
        /// Whether the extension runs in private windows.
        #[qinvokable]
        #[cxx_name = "runsInPrivate"]
        fn runs_in_private(self: &Extensions, id: &QString) -> bool;
        /// Whether the extension owning a moz-extension: address does.
        #[qinvokable]
        #[cxx_name = "urlRunsInPrivate"]
        fn url_runs_in_private(self: &Extensions, url: &QString) -> bool;
        /// Whether a moz-extension: address (or origin) is an enabled extension's that
        /// has the API permission, from its manifest or granted since.
        #[qinvokable]
        #[cxx_name = "urlHasPermission"]
        fn url_has_permission(self: &Extensions, url: &QString, permission: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "setEnabled"]
        fn set_enabled(self: Pin<&mut Extensions>, id: &QString, enabled: bool);
        #[qinvokable]
        fn uninstall(self: Pin<&mut Extensions>, id: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "grantPermissions"]
        fn grant_permissions(self: Pin<&mut Extensions>, id: &QString, perms: &QString);
        #[qinvokable]
        #[cxx_name = "optionsUrl"]
        fn options_url(self: &Extensions, id: &QString) -> QString;

        // ---- installation ----
        #[qinvokable]
        #[cxx_name = "installFromAmo"]
        fn install_from_amo(self: Pin<&mut Extensions>, id_or_url: &QString);
        #[qinvokable]
        #[cxx_name = "installFromUrl"]
        fn install_from_url(self: Pin<&mut Extensions>, url: &QString);
        #[qinvokable]
        #[cxx_name = "installFromFile"]
        fn install_from_file(self: Pin<&mut Extensions>, path: &QString);
        #[qinvokable]
        #[cxx_name = "confirmInstall"]
        fn confirm_install(self: Pin<&mut Extensions>, request: i32, accept: bool);
        #[qinvokable]
        #[cxx_name = "loadTemporary"]
        fn load_temporary(self: Pin<&mut Extensions>, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "searchAmo"]
        fn search_amo(self: Pin<&mut Extensions>, query: &QString, page: i32, sort: &QString);
        #[qinvokable]
        #[cxx_name = "amoDetails"]
        fn amo_details(self: Pin<&mut Extensions>, slug: &QString);
        #[qinvokable]
        #[cxx_name = "checkUpdates"]
        fn check_updates(self: Pin<&mut Extensions>);
        #[qsignal]
        #[cxx_name = "installPrompt"]
        fn install_prompt(self: Pin<&mut Extensions>, request: i32, json: QString);
        #[qsignal]
        #[cxx_name = "installStatus"]
        fn install_status(self: Pin<&mut Extensions>, json: QString);
        #[qsignal]
        #[cxx_name = "amoResults"]
        fn amo_results(self: Pin<&mut Extensions>, json: QString);
        #[qsignal]
        #[cxx_name = "amoDetail"]
        fn amo_detail(self: Pin<&mut Extensions>, json: QString);
        #[qsignal]
        #[cxx_name = "updatesFound"]
        fn updates_found(self: Pin<&mut Extensions>, json: QString);
    }

    impl cxx_qt::Threading for Extensions {}
    impl cxx_qt::Initialize for Extensions {}
}

struct PendingInstall {
    bytes: Vec<u8>,
    source: String,
    slug: String,
}

#[derive(Default)]
pub struct ExtensionsRust {
    revision: i32,
    /// Messages waiting for each view's bridge (see deliveryReady).
    outbox: HashMap<String, std::collections::VecDeque<String>>,
    pending: HashMap<i32, PendingInstall>,
    next_request: i32,
}

static THREAD: OnceLock<CxxQtThread<qobject::Extensions>> = OnceLock::new();

fn q(s: impl AsRef<str>) -> QString {
    QString::from(s.as_ref())
}
fn js(v: &Value) -> QString {
    q(v.to_string())
}
fn parse(s: &QString) -> Value {
    serde_json::from_str(&s.to_string()).unwrap_or(Value::Null)
}

/// QML's Image cannot load moz-extension:// URLs: rewrite `icon` fields to
/// file:// URLs of the unpacked extension.
fn qml_icons(mut v: Value) -> Value {
    fn walk(v: &mut Value, rt: &kite_core::webext::runtime::Runtime) {
        match v {
            Value::Object(o) => {
                for (k, x) in o.iter_mut() {
                    if k == "icon" {
                        if let Some(u) = x.as_str().and_then(|s| url::Url::parse(s).ok()).filter(|u| u.scheme() == "moz-extension") {
                            if let Some(p) = rt.registry.by_uuid(u.host_str().unwrap_or("")).and_then(|e| e.resolve(u.path())) {
                                *x = Value::String(format!("file://{}", p.to_string_lossy()));
                            }
                        }
                    } else {
                        walk(x, rt);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, rt)),
            _ => {}
        }
    }
    walk(&mut v, &app().runtime());
    v
}

/// Apply runtime effects from any context (interceptor, other QObjects):
/// queued onto the Qt event loop.
pub fn apply_effects_async(fx: Vec<Effect>) {
    if fx.is_empty() {
        return;
    }
    if let Some(t) = THREAD.get() {
        let _ = t.queue(move |o| o.apply(fx));
    }
}

impl cxx_qt::Initialize for qobject::Extensions {
    fn initialize(self: Pin<&mut Self>) {
        let _ = THREAD.set(self.qt_thread());
    }
}

/// Parental controls: a child's profile installs, removes and switches extensions only
/// if it may.
fn extensions_allowed() -> bool {
    app().parental.allows(kite_core::parental::Permission::Extensions)
}

const EXTENSIONS_LOCKED: &str = "Extensions can't be installed or changed in this profile (parental controls).";

impl qobject::Extensions {
    /// Queue a message for a view's bridge and tell the bridges (the signal
    /// carries only the view's name; its bridge takes the messages). Views
    /// without a bridge (yet) keep only the latest messages.
    fn post(mut self: Pin<&mut Self>, view: String, json: String) {
        {
            let mut r = self.as_mut().rust_mut();
            let queue = r.outbox.entry(view.clone()).or_default();
            if queue.len() >= 256 {
                queue.pop_front();
            }
            queue.push_back(json);
        }
        self.delivery_ready(q(view));
    }

    fn take_delivery(mut self: Pin<&mut Self>, view: &QString) -> QString {
        let view = view.to_string();
        let mut r = self.as_mut().rust_mut();
        let Some(queue) = r.outbox.get_mut(&view) else { return QString::default() };
        let next = queue.pop_front();
        if queue.is_empty() {
            r.outbox.remove(&view);
        }
        next.map(q).unwrap_or_default()
    }

    fn apply(mut self: Pin<&mut Self>, fx: Vec<Effect>) {
        for e in fx {
            match e {
                Effect::Reply { view, ctx, call_id, result } => {
                    let payload = match result {
                        Ok(v) => json!({"kind": "reply", "ctx": ctx, "callId": call_id, "ok": true, "value": v}),
                        Err(e) => json!({"kind": "reply", "ctx": ctx, "callId": call_id, "ok": false, "error": e}),
                    };
                    if view.starts_with(crate::rpc::VIEW_PREFIX) {
                        crate::rpc::deliver(&view, payload.to_string());
                    } else {
                        self.as_mut().post(view, payload.to_string());
                    }
                }
                Effect::Deliver { view, ctxs, payload } => {
                    let payload = json!({"kind": "deliver", "ctxs": ctxs, "payload": payload});
                    if view.starts_with(crate::rpc::VIEW_PREFIX) {
                        crate::rpc::deliver(&view, payload.to_string());
                    } else {
                        self.as_mut().post(view, payload.to_string());
                    }
                }
                Effect::Ui { view, ctx, call_id, ext, method, args } if method == "net.fetch" => {
                    std::thread::spawn(move || {
                        let result = native_fetch(&ext, &args);
                        let fx = app().runtime().ui_reply(&view, &ctx, call_id, result);
                        apply_effects_async(fx);
                    });
                }
                Effect::Ui { view, ctx, call_id, ext, method, args } if method == "native.send" => {
                    std::thread::spawn(move || {
                        let result = crate::native_hosts::send_once(&ext, &args);
                        let fx = app().runtime().ui_reply(&view, &ctx, call_id, result);
                        apply_effects_async(fx);
                    });
                }
                Effect::Native(op) => crate::native_hosts::run(op),
                Effect::Ui { view, ctx, call_id, ext, method, args } => {
                    if let Some(result) = handle_ui_in_rust(&method, &args) {
                        let more = app().runtime().ui_reply(&view, &ctx, call_id as i64, result);
                        self.as_mut().apply(more);
                    } else {
                        self.as_mut().ui_request(q(view), q(ctx), call_id as i32, q(ext), q(method), js(&args));
                    }
                }
                Effect::Changed(what) => {
                    if what == "extensions" || what == "scripts" || what == "background" {
                        let r = *self.revision() + 1;
                        self.as_mut().set_revision(r);
                    }
                    self.as_mut().changed(q(what));
                }
            }
        }
    }

    // ---- API plumbing -------------------------------------------------------
    #[allow(clippy::too_many_arguments)]
    fn call(self: Pin<&mut Self>, view: &QString, tab_id: i32, window_id: i32, document: &QString, token: &QString, ctx: &QString, method: &QString, args: &QString, call_id: i32) {
        let origin = Origin { view: view.to_string(), tab_id: tab_id as i64, window_id: window_id as i64, document: document.to_string() };
        let args: Value = serde_json::from_str(&args.to_string()).unwrap_or(Value::Array(vec![]));
        let fx = app().runtime().call(&origin, &token.to_string(), &ctx.to_string(), &method.to_string(), args, call_id as i64);
        self.apply(fx);
    }

    fn ui_reply(self: Pin<&mut Self>, view: &QString, ctx: &QString, call_id: i32, ok: bool, json: &QString) {
        let v = parse(json);
        let result = if ok { Ok(v) } else { Err(v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())) };
        let fx = app().runtime().ui_reply(&view.to_string(), &ctx.to_string(), call_id as i64, result);
        self.apply(fx);
    }

    fn browser_event(self: Pin<&mut Self>, name: &QString, json: &QString) {
        let fx = app().runtime().browser_event(&name.to_string(), &parse(json));
        self.apply(fx);
    }

    fn view_closed(mut self: Pin<&mut Self>, view: &QString) {
        self.as_mut().rust_mut().outbox.remove(&view.to_string());
        let fx = app().runtime().view_closed(&view.to_string());
        self.apply(fx);
    }

    fn tick(self: Pin<&mut Self>) {
        let mut fx = app().runtime().tick();
        // Sub-frame extension pages that stopped polling are gone.
        for view in crate::rpc::sweep() {
            fx.extend(app().runtime().view_closed(&view));
        }
        self.apply(fx);
    }

    // ---- UI integration -------------------------------------------------------
    fn toolbar(&self, tab_id: i32, in_private: bool) -> QString {
        let v = app().runtime().toolbar_json(tab_id as i64, in_private);
        js(&qml_icons(v))
    }
    fn sidebars(&self, in_private: bool) -> QString {
        let v = app().runtime().sidebars_json(in_private);
        js(&qml_icons(v))
    }
    fn menus(&self, info: &QString) -> QString {
        let v = app().runtime().menus_json(&parse(info));
        js(&qml_icons(v))
    }
    fn menu_clicked(self: Pin<&mut Self>, ext: &QString, item: &QString, info: &QString, tab_id: i32) {
        let fx = app().runtime().menu_clicked(&ext.to_string(), &item.to_string(), &parse(info), tab_id as i64);
        self.apply(fx);
    }
    fn action_clicked(self: Pin<&mut Self>, ext: &QString, kind: &QString, tab_id: i32) {
        let fx = app().runtime().action_clicked(&ext.to_string(), &kind.to_string(), tab_id as i64);
        self.apply(fx);
    }
    fn commands(&self, in_private: bool) -> QString {
        js(&app().runtime().commands_json(in_private))
    }
    fn command(self: Pin<&mut Self>, ext: &QString, name: &QString, tab_id: i32) {
        let fx = app().runtime().command(&ext.to_string(), &name.to_string(), tab_id as i64);
        self.apply(fx);
    }

    /// Profile-wide user scripts: the bridge plus every enabled extension's
    /// content scripts. QML installs these into `profile.userScripts`.
    /// (`in_private`: for private windows, with only the extensions allowed there.)
    fn user_scripts(&self, in_private: bool) -> QString {
        let a = app();
        let rt = a.runtime();
        let qwc = crate::ffi::qwebchannel_js();
        let mut out = vec![];
        let mut tokens = vec![];
        if a.shared.settings().get_bool("extensions.enabled") {
            for e in rt.registry.enabled().filter(|e| !in_private || e.runs_in_private()) {
                let Some(token) = rt.token_of(&e.info.id) else { continue };
                // (None: the engine has no world left for this one's scripts.)
                let Some(world) = rt.world_of(&e.info.id) else { continue };
                tokens.push(token.to_owned());
                for s in scripts::content_scripts(e, token, world, &rt.dynamic_scripts(&e.info.id)) {
                    out.push(json!({"name": s.name, "sourceCode": s.source, "worldId": s.world, "runsOnSubFrames": s.all_frames, "injectionPoint": s.injection_point}));
                }
            }
        }
        // The bridge must be inserted first so it listens before content scripts run.
        out.insert(
            0,
            json!({"name": "kite:bridge", "sourceCode": scripts::bridge_js(&tokens, qwc), "worldId": kite_core::webext::CONTENT_WORLD, "runsOnSubFrames": false, "injectionPoint": 0}),
        );
        js(&Value::Array(out))
    }

    fn background_pages(&self) -> QString {
        let a = app();
        if !a.shared.settings().get_bool("extensions.enabled") {
            return q("[]");
        }
        let rt = a.runtime();
        let list: Vec<Value> = rt.registry.enabled().filter_map(|e| scripts::background_url(e).map(|u| json!({"ext": e.info.id, "name": e.manifest.name, "url": u}))).collect();
        js(&Value::Array(list))
    }

    fn network_ready(&self) -> bool {
        app().runtime().network_ready()
    }

    fn new_tab_override(&self, in_private: bool) -> QString {
        let rt = app().runtime();
        let url = rt.registry.enabled().filter(|e| !in_private || e.runs_in_private()).find_map(|e| e.manifest.newtab_override.as_ref().map(|p| e.url(p)));
        q(url.unwrap_or_default())
    }

    fn extension_name_for_url(&self, url: &QString) -> QString {
        let url = url.to_string();
        let Ok(u) = url::Url::parse(&url) else { return q("") };
        if u.scheme() != "moz-extension" {
            return q("");
        }
        let rt = app().runtime();
        q(rt.registry.by_uuid(u.host_str().unwrap_or("")).map(|e| e.manifest.name.clone()).unwrap_or_default())
    }

    fn omnibox_keywords(&self, in_private: bool) -> QString {
        let rt = app().runtime();
        js(&Value::Array(rt.registry.enabled().filter(|e| !in_private || e.runs_in_private()).filter_map(|e| e.manifest.omnibox_keyword.as_ref().map(|k| json!({"ext": e.info.id, "keyword": k, "name": e.manifest.name}))).collect()))
    }

    // ---- management -----------------------------------------------------------
    fn list(&self) -> QString {
        let rt = app().runtime();
        let mut out: Vec<Value> = rt
            .registry
            .extensions
            .iter()
            .map(|e| {
                let m = &e.manifest;
                json!({
                    "id": e.info.id, "name": m.name, "version": m.version, "description": m.description, "author": m.author,
                    "homepage": m.homepage_url, "enabled": e.info.enabled, "allowPrivate": e.runs_in_private(), "privateNotAllowed": e.private_not_allowed(), "source": e.info.source, "amoSlug": e.info.amo_slug,
                    "icon": e.icon_url(64), "optionsUrl": m.options_page.as_ref().map(|p| e.url(p)),
                    "optionsInTab": m.options_open_in_tab,
                    "permissions": m.permissions, "hostPermissions": e.host_patterns(), "warnings": m.permission_warnings(),
                    "optionalPermissions": m.optional_permissions, "granted": e.info.granted_optional,
                    "temporary": e.info.source == "temporary", "manifestVersion": m.manifest_version,
                    "installedAt": e.info.installed_at, "updatedAt": e.info.updated_at,
                    "unsupported": kite_core::webext::manifest::unsupported_keys(m.raw.as_object().unwrap_or(&Default::default())),
                    "hasSidebar": m.sidebar_action.is_some(), "path": e.info.path.to_string_lossy(),
                })
            })
            .collect();
        out.extend(rt.registry.broken.iter().map(|(i, err)| json!({"id": i.id, "name": i.id, "version": i.version, "enabled": false, "broken": true, "error": err, "source": i.source})));
        out.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_lowercase());
        drop(rt);
        js(&qml_icons(Value::Array(out)))
    }

    fn set_allow_private(self: Pin<&mut Self>, id: &QString, allow: bool) {
        if app().runtime().registry.set_allow_private(&id.to_string(), allow).is_err() {
            return;
        }
        // Private windows' content scripts and toolbars change.
        self.apply(vec![Effect::Changed("extensions".into()), Effect::Changed("scripts".into()), Effect::Changed("actions".into())]);
    }

    fn runs_in_private(&self, id: &QString) -> bool {
        app().runtime().runs_in_private(&id.to_string())
    }

    fn url_runs_in_private(&self, url: &QString) -> bool {
        let Ok(u) = url::Url::parse(&url.to_string()) else { return false };
        app().runtime().registry.by_uuid(u.host_str().unwrap_or("")).is_some_and(|e| e.info.enabled && e.runs_in_private())
    }

    fn url_has_permission(&self, url: &QString, permission: &QString) -> bool {
        let Ok(u) = url::Url::parse(&url.to_string()) else { return false };
        u.scheme() == "moz-extension" && app().runtime().registry.by_uuid(u.host_str().unwrap_or("")).is_some_and(|e| e.info.enabled && e.has_permission(&permission.to_string()))
    }

    fn set_enabled(self: Pin<&mut Self>, id: &QString, enabled: bool) {
        if !extensions_allowed() {
            return;
        }
        let id = id.to_string();
        let fx = {
            let mut rt = app().runtime();
            if rt.registry.set_enabled(&id, enabled).is_err() {
                return;
            }
            rt.on_enabled_changed(&id, enabled)
        };
        self.apply(fx);
    }

    /// Returns the extension's uninstall URL (if it set one) for the UI to open.
    fn uninstall(self: Pin<&mut Self>, id: &QString) -> QString {
        if !extensions_allowed() {
            return QString::default();
        }
        let id = id.to_string();
        let url = {
            let mut rt = app().runtime();
            let url = rt.on_uninstall(&id);
            let _ = rt.registry.uninstall(&id);
            url
        };
        self.apply(vec![
            Effect::Changed("extensions".into()),
            Effect::Changed("scripts".into()),
            Effect::Changed("background".into()),
            Effect::Changed("actions".into()),
            Effect::Changed("menus".into()),
        ]);
        q(url.unwrap_or_default())
    }

    fn grant_permissions(self: Pin<&mut Self>, id: &QString, perms: &QString) {
        let list: Vec<String> = serde_json::from_str(&perms.to_string()).unwrap_or_default();
        let id = id.to_string();
        let _ = app().runtime().registry.grant_optional(&id, &list, true);
        self.apply(vec![Effect::Changed("scripts".into())]);
    }

    fn options_url(&self, id: &QString) -> QString {
        let rt = app().runtime();
        q(rt.registry.get(&id.to_string()).and_then(|e| e.manifest.options_page.as_ref().map(|p| e.url(p))).unwrap_or_default())
    }

    // ---- installation -----------------------------------------------------------
    fn start_download(self: Pin<&mut Self>, what: String, direct_url: Option<String>) {
        if !extensions_allowed() {
            self.install_status(q(json!({"state": "error", "message": EXTENSIONS_LOCKED}).to_string()));
            return;
        }
        let thread = self.qt_thread();
        let status = |s: &str, m: &str| json!({"state": s, "message": m}).to_string();
        let _ = thread.queue({
            let msg = status("downloading", "Downloading add-on…");
            move |o| o.install_status(q(msg))
        });
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<(Vec<u8>, String, String)> {
                if let Some(url) = direct_url {
                    let bytes = amo::download(&url, "")?;
                    // Only Mozilla's site counts as AMO: any other is a website
                    // offering a file nobody has reviewed.
                    let from_amo = url::Url::parse(&url).is_ok_and(|u| u.scheme() == "https" && u.host_str() == Some("addons.mozilla.org"));
                    let source = if from_amo { "amo".to_owned() } else { format!("web:{}", kite_core::util::host_of(&url)) };
                    return Ok((bytes, source, String::new()));
                }
                let id = amo::slug_from_url(&what).unwrap_or(what.clone());
                let d = amo::details(&id)?;
                if d.kind != "extension" && !d.kind.is_empty() {
                    anyhow::bail!("\"{}\" is a {} — Kite only supports extensions", d.name, d.kind);
                }
                if d.file_url.is_empty() {
                    anyhow::bail!("\"{}\" has no compatible file", d.name);
                }
                let bytes = amo::download(&d.file_url, &d.file_hash)?;
                Ok((bytes, "amo".into(), d.slug))
            })();
            let _ = thread.queue(move |mut o| match result {
                Ok((bytes, source, slug)) => o.as_mut().prompt_install(bytes, source, slug),
                Err(e) => o.install_status(q(json!({"state": "error", "message": e.to_string()}).to_string())),
            });
        });
    }

    fn prompt_install(mut self: Pin<&mut Self>, bytes: Vec<u8>, source: String, slug: String) {
        if !extensions_allowed() {
            self.install_status(q(json!({"state": "error", "message": EXTENSIONS_LOCKED}).to_string()));
            return;
        }
        let manifest = match Registry::inspect(&bytes) {
            Ok(m) => m,
            Err(e) => {
                self.install_status(q(json!({"state": "error", "message": format!("This add-on could not be installed: {e}")}).to_string()));
                return;
            }
        };
        let id = manifest.gecko_id.clone().unwrap_or_else(|| kite_core::webext::registry::fallback_id(&manifest.name));
        let (existing, existing_source) = app().runtime().registry.get(&id).map(|e| (Some(e.manifest.version.clone()), e.info.source.clone())).unwrap_or_default();
        // A website's file can't replace an add-on from addons.mozilla.org:
        // without signatures, its id is only a claim, and a fake "update"
        // would take over the real add-on (its data, its native apps).
        let site = source.strip_prefix("web:").map(str::to_owned);
        if site.is_some() && existing_source == "amo" {
            self.install_status(q(json!({"state": "error", "message": format!("{} offered a file claiming to be an update of an add-on you installed from addons.mozilla.org. Kite didn't install it: update it from Add-ons instead.", site.unwrap_or_default())}).to_string()));
            return;
        }
        // Localized name for the prompt.
        let name = {
            let dir = kite_core::util::private_temp_dir().map(|d| d.join("inspect"));
            if let Ok(dir) = &dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            let n = dir
                .as_ref()
                .ok()
                .filter(|dir| kite_core::webext::xpi::extract(&bytes, dir).is_ok())
                .map(|dir| {
                    let msgs = kite_core::webext::i18n::Messages::load(dir, manifest.default_locale.as_deref());
                    msgs.localize(&manifest.name, &id)
                })
                .unwrap_or_else(|| manifest.name.clone());
            if let Ok(dir) = &dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            n
        };
        let req = {
            let mut r = self.as_mut().rust_mut();
            r.next_request += 1;
            let req = r.next_request;
            r.pending.insert(req, PendingInstall { bytes, source: source.clone(), slug });
            req
        };
        let info = json!({
            "id": id, "name": name, "version": manifest.version, "warnings": manifest.permission_warnings(),
            "update": existing.is_some(), "previousVersion": existing, "manifestVersion": manifest.manifest_version,
            "source": if source == "amo" { "amo" } else if site.is_some() { "web" } else { "file" }, "site": site,
            // It takes the place of one from elsewhere (its id is only what the file says):
            // "amo", "web" or "file", for the prompt to say so.
            "replaces": (existing.is_some() && existing_source != source).then(|| if existing_source == "amo" { "amo" } else if existing_source.starts_with("web:") { "web" } else { "file" }),
        });
        self.install_prompt(req, js(&info));
    }

    fn install_from_amo(self: Pin<&mut Self>, id_or_url: &QString) {
        let s = id_or_url.to_string();
        let direct = (amo::is_amo_download(&s) || s.ends_with(".xpi")).then(|| s.clone());
        self.start_download(s, direct);
    }

    fn install_from_url(self: Pin<&mut Self>, url: &QString) {
        let url = url.to_string();
        self.start_download(url.clone(), Some(url));
    }

    fn install_from_file(mut self: Pin<&mut Self>, path: &QString) {
        let p = path.to_string();
        let p = p.strip_prefix("file://").unwrap_or(&p).to_owned();
        let p = percent_encoding::percent_decode_str(&p).decode_utf8_lossy().into_owned();
        match std::fs::read(&p) {
            Ok(bytes) => self.as_mut().prompt_install(bytes, "file".into(), String::new()),
            Err(e) => self.install_status(q(json!({"state": "error", "message": format!("Cannot read {p}: {e}")}).to_string())),
        }
    }

    fn confirm_install(mut self: Pin<&mut Self>, request: i32, accept: bool) {
        let Some(p) = self.as_mut().rust_mut().pending.remove(&request) else { return };
        if accept && !extensions_allowed() {
            self.install_status(q(json!({"state": "error", "message": EXTENSIONS_LOCKED}).to_string()));
            return;
        }
        if !accept {
            self.install_status(q(json!({"state": "cancelled", "message": "Installation cancelled"}).to_string()));
            return;
        }
        let res = {
            let mut rt = app().runtime();
            let prev = Registry::inspect(&p.bytes).ok().and_then(|m| m.gecko_id).and_then(|id| rt.registry.get(&id).map(|e| e.manifest.version.clone()));
            match rt.registry.install_xpi(&p.bytes, &p.source, &p.slug) {
                Ok(r) => {
                    let fx = rt.on_installed(&r.id, r.updated, prev.as_deref().unwrap_or(""), false);
                    Ok((r, fx))
                }
                Err(e) => Err(e),
            }
        };
        match res {
            Ok((r, fx)) => {
                self.as_mut().apply(fx);
                let verb = if r.updated { "updated" } else { "added" };
                self.install_status(q(json!({"state": "installed", "id": r.id, "message": format!("{} {} has been {verb}", r.name, r.version)}).to_string()));
            }
            Err(e) => self.install_status(q(json!({"state": "error", "message": format!("Installation failed: {e}")}).to_string())),
        }
    }

    fn load_temporary(mut self: Pin<&mut Self>, path: &QString) -> QString {
        if !extensions_allowed() {
            return q(EXTENSIONS_LOCKED);
        }
        let p = path.to_string();
        let p = p.strip_prefix("file://").unwrap_or(&p).to_owned();
        let mut dir = std::path::PathBuf::from(&p);
        if dir.is_file() {
            dir.pop();
        }
        let res = {
            let mut rt = app().runtime();
            rt.registry.load_temporary(&dir).map(|r| {
                let fx = rt.on_installed(&r.id, false, "", true);
                (r, fx)
            })
        };
        match res {
            Ok((r, fx)) => {
                self.as_mut().apply(fx);
                q(format!("Loaded temporary add-on {} {}", r.name, r.version))
            }
            Err(e) => q(format!("Could not load add-on: {e}")),
        }
    }

    fn search_amo(self: Pin<&mut Self>, query: &QString, page: i32, sort: &QString) {
        let (query, sort) = (query.to_string(), sort.to_string());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let out = match amo::search(&query, page.max(1) as u32, &sort) {
                Ok((list, count)) => json!({"query": query, "page": page, "count": count, "results": list}),
                Err(e) => json!({"query": query, "page": page, "error": e.to_string(), "results": []}),
            };
            let _ = thread.queue(move |o| o.amo_results(js(&out)));
        });
    }

    fn amo_details(self: Pin<&mut Self>, slug: &QString) {
        let slug = slug.to_string();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let out = match amo::details(&slug) {
                Ok(d) => serde_json::to_value(d).unwrap_or_default(),
                Err(e) => json!({"error": e.to_string()}),
            };
            let _ = thread.queue(move |o| o.amo_detail(js(&out)));
        });
    }

    fn check_updates(self: Pin<&mut Self>) {
        let list: Vec<(String, String, String)> = {
            let rt = app().runtime();
            rt.registry.extensions.iter().filter(|e| e.info.source == "amo").map(|e| (e.info.id.clone(), if e.info.amo_slug.is_empty() { e.info.id.clone() } else { e.info.amo_slug.clone() }, e.manifest.version.clone())).collect()
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let mut updates = vec![];
            for (id, slug, version) in list {
                if let Ok(d) = amo::details(&slug) {
                    // (The add-on that is installed, not another one now listed under its name there.)
                    if d.guid == id && kite_core::webext::registry::version_newer(&d.version, &version) {
                        updates.push(json!({"id": id, "name": d.name, "current": version, "available": d.version, "slug": d.slug}));
                    }
                }
            }
            let _ = thread.queue(move |o| o.updates_found(js(&Value::Array(updates))));
        });
    }
}

/// Where a fetch goes after an answer that sends it on (`status`, with its `Location`):
/// the address, and whether the request stays what it was (307, 308) or becomes a
/// plain GET, as browsers make it.
fn fetch_sent_on(from: &str, status: u16, location: Option<&str>) -> Option<(String, bool)> {
    if !matches!(status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    let to = url::Url::parse(from).ok()?.join(location?).ok()?;
    Some((to.into(), matches!(status, 307 | 308)))
}

/// HTTP request on behalf of an extension page (see runtime `_.fetch`).
fn native_fetch(ext: &str, d: &Value) -> Result<Value, String> {
    use base64::Engine;
    const REFUSED: &str = "NetworkError when attempting to fetch resource.";
    let mut url = d["url"].as_str().unwrap_or_default().to_owned();
    let mut method = d["method"].as_str().unwrap_or("GET").to_uppercase();
    // Redirects are followed here, one at a time: each address is held to what the
    // first one was (below), where the agent would have followed them wherever they led.
    let agent: ureq::Agent = kite_core::net::config()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();
    let mut body: Option<Vec<u8>> = d["body"].as_str().map(|b| base64::engine::general_purpose::STANDARD.decode(b).unwrap_or_default());
    let first_host = kite_core::util::host_of(&url);
    let mut hops = 0;
    let resp = loop {
        // The extension's hosts only: an address on one of them that answers "go there"
        // would else read any other site for it.
        if !app().runtime().may_fetch(ext, &url) {
            log::warn!("{ext}: a fetch was sent on to {}, which isn't one of its hosts", kite_core::util::host_of(&url));
            return Err(REFUSED.into());
        }
        // (Kite fetches this itself, past the request interceptor: a child's profile is
        // kept from the same sites here.)
        if app().parental.filter.is_on() && app().parental.filter.check(&kite_core::util::host_of(&url)).is_some() {
            return Err(REFUSED.into());
        }
        let mut req = ureq::http::Request::builder().method(method.as_str()).uri(url.as_str()).header("User-Agent", amo::USER_AGENT);
        // (What says who is asking stays with the host it was meant for.)
        let same_host = kite_core::util::host_of(&url) == first_host;
        for pair in d["headers"].as_array().into_iter().flatten() {
            if let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str()) {
                if same_host || !matches!(k.to_ascii_lowercase().as_str(), "authorization" | "cookie" | "proxy-authorization") {
                    req = req.header(k, v);
                }
            }
        }
        let resp = match &body {
            Some(b) => agent.run(req.body(b.clone()).map_err(|e| e.to_string())?),
            None => agent.run(req.body(()).map_err(|e| e.to_string())?),
        }
        .map_err(|e| format!("NetworkError: {e}"))?;
        let location = resp.headers().get("location").and_then(|l| l.to_str().ok());
        let Some((to, same_request)) = fetch_sent_on(&url, resp.status().as_u16(), location) else { break resp };
        hops += 1;
        if hops > 10 {
            return Err("NetworkError: too many redirects".into());
        }
        if !same_request {
            method = if method == "HEAD" { method } else { "GET".into() };
            body = None;
        }
        url = to;
    };
    let status = resp.status().as_u16();
    let headers: Vec<Value> = resp.headers().iter().map(|(k, v)| json!([k.as_str(), v.to_str().unwrap_or("")])).collect();
    let mut resp = resp;
    let bytes = resp.body_mut().with_config().limit(256 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())?;
    Ok(json!({"status": status, "statusText": "", "url": url, "headers": headers, "body": base64::engine::general_purpose::STANDARD.encode(bytes)}))
}

/// Whether a child's profile is kept from this address (parental controls). Kite's own
/// fetches for extensions go past the request interceptor, which is where pages are held to it.
fn kept_from_child(url: &str) -> bool {
    app().parental.filter.is_on() && app().parental.filter.check(&kite_core::util::host_of(url)).is_some()
}

/// A part of the path an extension names for a download: no hidden file or folder, and
/// nothing that isn't a letter of a name.
fn download_name_part(part: &str) -> String {
    kite_core::util::download_file_name(part)
}

/// downloads.download for http(s) and data: URLs, done natively.
fn native_download(d: &Value) -> Result<Value, String> {
    use base64::Engine;
    let url = d["url"].as_str().unwrap_or_default().to_owned();
    if !url.starts_with("data:") && kept_from_child(&url) {
        return Err("Download blocked".into());
    }
    let a = app();
    let dir = std::path::PathBuf::from(a.shared.settings().get_str("downloads.directory"));
    let suggested = d["filename"].as_str().map(str::to_owned).unwrap_or_else(|| {
        url::Url::parse(&url).ok().and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(str::to_owned))).filter(|s| !s.is_empty()).unwrap_or_else(|| "download".into())
    });
    // Keep the file inside the downloads directory.
    let rel: std::path::PathBuf = suggested.split(['/', '\\']).filter(|c| !c.is_empty() && *c != "..").map(download_name_part).collect();
    let mut path = dir.join(&rel);
    if path.exists() && d["conflictAction"].as_str() != Some("overwrite") {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let mut i = 1;
        while path.exists() {
            path = path.with_file_name(format!("{stem} ({i}){ext}"));
            i += 1;
        }
    }
    let id = {
        let mut dl = a.shared.downloads();
        let id = dl.next_id();
        dl.upsert(kite_core::downloads::DownloadRecord { private: false, id, url: url.clone(), path: path.to_string_lossy().into_owned(), mime: String::new(), total: 0, received: 0, state: "in_progress".into(), started: kite_core::util::now_ms(), finished: 0 });
        id
    };
    std::thread::spawn(move || {
        let result: anyhow::Result<i64> = (|| {
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p)?;
            }
            if let Some(rest) = url.strip_prefix("data:") {
                let (meta, data) = rest.split_once(',').ok_or_else(|| anyhow::anyhow!("bad data URL"))?;
                let bytes: Vec<u8> = if meta.ends_with(";base64") { base64::engine::general_purpose::STANDARD.decode(data.trim())? } else { percent_encoding::percent_decode_str(data).collect() };
                std::fs::write(&path, &bytes)?;
                return Ok(bytes.len() as i64);
            }
            // Redirects one at a time, each address held to what the first was; and
            // written as it arrives, not held in memory first.
            // (However long it takes: a large file is no stalled one. Connecting and the
            // start of an answer keep their limits.)
            let agent: ureq::Agent = kite_core::net::config().max_redirects(0).http_status_as_error(false).timeout_global(None).build().into();
            let mut at = url.clone();
            let mut hops = 0;
            let mut r = loop {
                let r = agent.get(&at).header("User-Agent", amo::USER_AGENT).call()?;
                let location = r.headers().get("location").and_then(|l| l.to_str().ok());
                let Some((to, _)) = fetch_sent_on(&at, r.status().as_u16(), location) else { break r };
                anyhow::ensure!(to.starts_with("http://") || to.starts_with("https://"), "sent on to an address that isn't the web's");
                anyhow::ensure!(!kept_from_child(&to), "sent on to a site this profile is kept from");
                hops += 1;
                anyhow::ensure!(hops <= 10, "sent on too many times");
                at = to;
            };
            anyhow::ensure!(r.status().is_success(), "the server answered {}", r.status());
            let mut file = std::fs::File::create(&path)?;
            Ok(std::io::copy(&mut r.body_mut().with_config().limit(4 * 1024 * 1024 * 1024).reader(), &mut file)? as i64)
        })();
        let state = match result {
            Ok(n) => {
                let mut dl = app().shared.downloads();
                if let Some(r) = dl.items.iter().find(|r| r.id == id).cloned() {
                    dl.upsert(kite_core::downloads::DownloadRecord { total: n, received: n, state: "complete".into(), finished: kite_core::util::now_ms(), ..r });
                }
                "complete"
            }
            Err(e) => {
                log::warn!("download of {} failed: {e}", &url[..url.floor_char_boundary(120)]);
                let mut dl = app().shared.downloads();
                if let Some(r) = dl.items.iter().find(|r| r.id == id).cloned() {
                    dl.upsert(kite_core::downloads::DownloadRecord { state: "interrupted".into(), ..r });
                }
                "interrupted"
            }
        };
        let fx = app().runtime().browser_event("downloadChanged", &json!({"args": [{"id": id, "state": {"current": state}}]}));
        apply_effects_async(fx);
    });
    Ok(json!(id))
}

/// Some UI requests don't need QML at all.
fn handle_ui_in_rust(method: &str, args: &Value) -> Option<Result<Value, String>> {
    match method {
        "downloads.download" => {
            let d = args.get(0).cloned().unwrap_or(Value::Null);
            let url = d["url"].as_str().unwrap_or("");
            if url.starts_with("http:") || url.starts_with("https:") || url.starts_with("data:") {
                return Some(native_download(&d));
            }
            Some(Err(format!("Cannot download {url}")))
        }
        "cookies.set" => {
            let mut c = args.clone();
            if c["path"].is_null() {
                c["path"] = "/".into();
            }
            if c["domain"].is_null() {
                // host-only cookie for the URL's host
                c["domain"] = kite_core::util::host_of(c["url"].as_str().unwrap_or("")).into();
            }
            crate::ffi::set_cookie(&c.to_string());
            let out = json!({"name": c["name"], "value": c["value"], "domain": c["domain"], "path": c["path"], "secure": c["secure"].as_bool().unwrap_or(false),
                             "httpOnly": c["httpOnly"].as_bool().unwrap_or(false), "session": c["expirationDate"].is_null(), "hostOnly": !c["domain"].as_str().unwrap_or("").starts_with('.'),
                             "storeId": "firefox-default", "sameSite": c["sameSite"].as_str().unwrap_or("no_restriction"), "expirationDate": c["expirationDate"], "firstPartyDomain": ""});
            Some(Ok(out))
        }
        "cookies.remove" => {
            let url = args["url"].as_str().unwrap_or("");
            let name = args["name"].as_str().unwrap_or("");
            let matches: Vec<kite_core::shared::Cookie> = app().shared.cookies().iter().filter(|c| c.name == name && c.matches_url(url)).cloned().collect();
            for c in &matches {
                let mut j = serde_json::to_value(c).unwrap_or_default();
                j["url"] = url.into();
                crate::ffi::delete_cookie(&j.to_string());
            }
            Some(Ok(if matches.is_empty() { Value::Null } else { json!({"url": url, "name": name, "storeId": "firefox-default", "firstPartyDomain": ""}) }))
        }
        m if m.starts_with("browsingData.") => {
            let what = &m["browsingData.".len()..];
            let since = args.get(0).and_then(|o| o["since"].as_f64()).unwrap_or(0.0) as i64;
            let types = args.get(1).cloned().unwrap_or(Value::Null);
            let want = |k: &str| what == "remove" && types[k].as_bool() == Some(true) || what.eq_ignore_ascii_case(&format!("remove{k}"));
            let a = app();
            if want("history") {
                let db = a.shared.db();
                let _ = kite_core::history::delete_range(&db, since, i64::MAX);
            }
            if want("cookies") {
                crate::ffi::delete_all_cookies();
            }
            if want("cache") {
                crate::ffi::clear_http_cache();
            }
            if want("downloads") {
                a.shared.downloads().clear_finished();
            }
            Some(Ok(Value::Null))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fetch_is_sent_on_only_by_a_redirect_with_somewhere_to_go() {
        let on = |status, location| fetch_sent_on("https://one.example/a/b?c", status, location);
        assert_eq!(on(302, Some("https://two.example/x")), Some(("https://two.example/x".into(), false)));
        assert_eq!(on(301, Some("/login")), Some(("https://one.example/login".into(), false)));
        assert_eq!(on(303, Some("d")), Some(("https://one.example/a/d".into(), false)));
        // These keep the request as it was (its method and what it sends).
        assert_eq!(on(307, Some("//two.example/")), Some(("https://two.example/".into(), true)));
        assert_eq!(on(308, Some("https://two.example/")), Some(("https://two.example/".into(), true)));
        // The address it is sent on to is the caller's to check: any kind may be named.
        assert_eq!(on(302, Some("file:///etc/passwd")).map(|(to, _)| to).as_deref(), Some("file:///etc/passwd"));
        for (status, location) in [(200, Some("https://two.example/")), (304, Some("/x")), (404, Some("/x")), (302, None)] {
            assert_eq!(on(status, location), None, "{status} {location:?}");
        }
    }

    #[test]
    fn a_downloads_name_is_no_hidden_file() {
        assert_eq!(download_name_part("report.pdf"), "report.pdf");
        assert_eq!(download_name_part(".bashrc"), "bashrc");
        assert_eq!(download_name_part("...config"), "config");
        assert_eq!(download_name_part(" a\nb\u{202e}.txt "), "ab.txt");
        assert_eq!(download_name_part("."), "download");
        assert_eq!(download_name_part(""), "download");
    }
}
