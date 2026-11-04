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
        fn call(self: Pin<&mut Extensions>, view: &QString, tab_id: i32, window_id: i32, token: &QString, ctx: &QString, method: &QString, args: &QString, call_id: i32);
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
        fn toolbar(self: &Extensions, tab_id: i32) -> QString;
        #[qinvokable]
        fn sidebars(self: &Extensions) -> QString;
        #[qinvokable]
        fn menus(self: &Extensions, info: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "menuClicked"]
        fn menu_clicked(self: Pin<&mut Extensions>, ext: &QString, item: &QString, info: &QString, tab_id: i32);
        #[qinvokable]
        #[cxx_name = "actionClicked"]
        fn action_clicked(self: Pin<&mut Extensions>, ext: &QString, kind: &QString, tab_id: i32);
        #[qinvokable]
        fn commands(self: &Extensions) -> QString;
        #[qinvokable]
        fn command(self: Pin<&mut Extensions>, ext: &QString, name: &QString, tab_id: i32);
        #[qinvokable]
        #[cxx_name = "userScripts"]
        fn user_scripts(self: &Extensions, include_extensions: bool) -> QString;
        #[qinvokable]
        #[cxx_name = "backgroundPages"]
        fn background_pages(self: &Extensions) -> QString;
        #[qinvokable]
        #[cxx_name = "newTabOverride"]
        fn new_tab_override(self: &Extensions) -> QString;
        #[qinvokable]
        #[cxx_name = "extensionNameForUrl"]
        fn extension_name_for_url(self: &Extensions, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "omniboxKeywords"]
        fn omnibox_keywords(self: &Extensions) -> QString;

        /// False while extensions that block requests are still starting;
        /// tabs hold their first load until then.
        #[qinvokable]
        #[cxx_name = "networkReady"]
        fn network_ready(self: &Extensions) -> bool;

        // ---- management ----
        #[qinvokable]
        fn list(self: &Extensions) -> QString;
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
                    let _ = ext;
                    std::thread::spawn(move || {
                        let result = native_fetch(&args);
                        let fx = app().runtime().ui_reply(&view, &ctx, call_id, result);
                        apply_effects_async(fx);
                    });
                }
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
    fn call(self: Pin<&mut Self>, view: &QString, tab_id: i32, window_id: i32, token: &QString, ctx: &QString, method: &QString, args: &QString, call_id: i32) {
        let origin = Origin { view: view.to_string(), tab_id: tab_id as i64, window_id: window_id as i64 };
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
    fn toolbar(&self, tab_id: i32) -> QString {
        let v = app().runtime().toolbar_json(tab_id as i64);
        js(&qml_icons(v))
    }
    fn sidebars(&self) -> QString {
        let v = app().runtime().sidebars_json();
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
    fn commands(&self) -> QString {
        js(&app().runtime().commands_json())
    }
    fn command(self: Pin<&mut Self>, ext: &QString, name: &QString, tab_id: i32) {
        let fx = app().runtime().command(&ext.to_string(), &name.to_string(), tab_id as i64);
        self.apply(fx);
    }

    /// Profile-wide user scripts: the bridge plus every enabled extension's
    /// content scripts. QML installs these into `profile.userScripts`.
    fn user_scripts(&self, include_extensions: bool) -> QString {
        let a = app();
        let rt = a.runtime();
        let qwc = crate::ffi::qwebchannel_js();
        let mut out = vec![];
        let mut tokens = vec![];
        let ext_on = include_extensions && a.shared.settings().get_bool("extensions.enabled");
        if ext_on {
            for e in rt.registry.enabled() {
                let Some(token) = rt.token_of(&e.info.id) else { continue };
                tokens.push(token.to_owned());
                for s in scripts::content_scripts(e, token, rt.world_of(&e.info.id), &rt.dynamic_scripts(&e.info.id)) {
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

    fn new_tab_override(&self) -> QString {
        let rt = app().runtime();
        let url = rt.registry.enabled().find_map(|e| e.manifest.newtab_override.as_ref().map(|p| e.url(p)));
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

    fn omnibox_keywords(&self) -> QString {
        let rt = app().runtime();
        js(&Value::Array(rt.registry.enabled().filter_map(|e| e.manifest.omnibox_keyword.as_ref().map(|k| json!({"ext": e.info.id, "keyword": k, "name": e.manifest.name}))).collect()))
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
                    "homepage": m.homepage_url, "enabled": e.info.enabled, "source": e.info.source, "amoSlug": e.info.amo_slug,
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

    fn set_enabled(self: Pin<&mut Self>, id: &QString, enabled: bool) {
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
                    return Ok((bytes, "amo".into(), String::new()));
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
        let manifest = match Registry::inspect(&bytes) {
            Ok(m) => m,
            Err(e) => {
                self.install_status(q(json!({"state": "error", "message": format!("This add-on could not be installed: {e}")}).to_string()));
                return;
            }
        };
        let id = manifest.gecko_id.clone().unwrap_or_else(|| kite_core::webext::registry::fallback_id(&manifest.name));
        let existing = app().runtime().registry.get(&id).map(|e| e.manifest.version.clone());
        // Localized name for the prompt.
        let name = {
            let dir = std::env::temp_dir().join(format!("kite-inspect-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let n = kite_core::webext::xpi::extract(&bytes, &dir)
                .ok()
                .map(|_| {
                    let msgs = kite_core::webext::i18n::Messages::load(&dir, manifest.default_locale.as_deref());
                    msgs.localize(&manifest.name, &id)
                })
                .unwrap_or_else(|| manifest.name.clone());
            let _ = std::fs::remove_dir_all(&dir);
            n
        };
        let req = {
            let mut r = self.as_mut().rust_mut();
            r.next_request += 1;
            let req = r.next_request;
            r.pending.insert(req, PendingInstall { bytes, source, slug });
            req
        };
        let info = json!({
            "id": id, "name": name, "version": manifest.version, "warnings": manifest.permission_warnings(),
            "update": existing.is_some(), "previousVersion": existing, "manifestVersion": manifest.manifest_version,
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
                    if kite_core::webext::registry::version_newer(&d.version, &version) {
                        updates.push(json!({"id": id, "name": d.name, "current": version, "available": d.version, "slug": d.slug}));
                    }
                }
            }
            let _ = thread.queue(move |o| o.updates_found(js(&Value::Array(updates))));
        });
    }
}

/// HTTP request on behalf of an extension page (see runtime `_.fetch`).
fn native_fetch(d: &Value) -> Result<Value, String> {
    use base64::Engine;
    let url = d["url"].as_str().unwrap_or_default();
    let method = d["method"].as_str().unwrap_or("GET").to_uppercase();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();
    let body: Option<Vec<u8>> = d["body"].as_str().map(|b| base64::engine::general_purpose::STANDARD.decode(b).unwrap_or_default());
    let mut req = ureq::http::Request::builder().method(method.as_str()).uri(url).header("User-Agent", amo::USER_AGENT);
    if let Some(h) = d["headers"].as_array() {
        for pair in h {
            if let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str()) {
                req = req.header(k, v);
            }
        }
    }
    let resp = match body {
        Some(b) => agent.run(req.body(b).map_err(|e| e.to_string())?),
        None => agent.run(req.body(()).map_err(|e| e.to_string())?),
    }
    .map_err(|e| format!("NetworkError: {e}"))?;
    let status = resp.status().as_u16();
    let headers: Vec<Value> = resp.headers().iter().map(|(k, v)| json!([k.as_str(), v.to_str().unwrap_or("")])).collect();
    let mut resp = resp;
    let bytes = resp.body_mut().with_config().limit(256 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())?;
    Ok(json!({"status": status, "statusText": "", "url": url, "headers": headers, "body": base64::engine::general_purpose::STANDARD.encode(bytes)}))
}

/// downloads.download for http(s) and data: URLs, done natively.
fn native_download(d: &Value) -> Result<Value, String> {
    use base64::Engine;
    let url = d["url"].as_str().unwrap_or_default().to_owned();
    let a = app();
    let dir = std::path::PathBuf::from(a.shared.settings().get_str("downloads.directory"));
    let suggested = d["filename"].as_str().map(str::to_owned).unwrap_or_else(|| {
        url::Url::parse(&url).ok().and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(str::to_owned))).filter(|s| !s.is_empty()).unwrap_or_else(|| "download".into())
    });
    // Keep the file inside the downloads directory.
    let rel: std::path::PathBuf = suggested.split('/').filter(|c| !c.is_empty() && *c != "..").collect();
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
        dl.upsert(kite_core::downloads::DownloadRecord { id, url: url.clone(), path: path.to_string_lossy().into_owned(), mime: String::new(), total: 0, received: 0, state: "in_progress".into(), started: kite_core::util::now_ms(), finished: 0 });
        id
    };
    std::thread::spawn(move || {
        let result: anyhow::Result<Vec<u8>> = (|| {
            if let Some(rest) = url.strip_prefix("data:") {
                let (meta, data) = rest.split_once(',').ok_or_else(|| anyhow::anyhow!("bad data URL"))?;
                return Ok(if meta.ends_with(";base64") {
                    base64::engine::general_purpose::STANDARD.decode(data.trim())?
                } else {
                    percent_encoding::percent_decode_str(data).collect()
                });
            }
            let mut r = ureq::get(&url).header("User-Agent", amo::USER_AGENT).call()?;
            Ok(r.body_mut().with_config().limit(4 * 1024 * 1024 * 1024).read_to_vec()?)
        })();
        let state = match result.and_then(|b| {
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&path, &b)?;
            Ok(b.len() as i64)
        }) {
            Ok(n) => {
                let mut dl = app().shared.downloads();
                if let Some(r) = dl.items.iter().find(|r| r.id == id).cloned() {
                    dl.upsert(kite_core::downloads::DownloadRecord { total: n, received: n, state: "complete".into(), finished: kite_core::util::now_ms(), ..r });
                }
                "complete"
            }
            Err(e) => {
                log::warn!("download of {url} failed: {e}");
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
