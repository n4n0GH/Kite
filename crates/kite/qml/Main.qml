import QtQuick
import QtQuick.Window
import QtQuick.Controls as QQC2
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.notification
import org.kde.kite

// Application controller (Falkon's MainApplication): owns the web profiles,
// browser windows, session management, downloads, notifications, extension
// background pages and the handling of extension UI requests.
QtObject {
    id: app

    default property list<QtObject> resources

    property WebEngineProfilePrototype _proto: WebEngineProfilePrototype {
        storageName: Browser.isPrivate() ? "" : "default"
        persistentStoragePath: Browser.webEnginePath()
        persistentCookiesPolicy: WebEngineProfile.AllowPersistentCookies
        persistentPermissionsPolicy: WebEngineProfile.StoreOnDisk
    }
    property WebEngineProfilePrototype _privateProto: WebEngineProfilePrototype {}
    property WebEngineProfile profile: null
    property WebEngineProfile privateProfile: null

    // ---- container tabs ("privacy.containers") ------------------------------
    // Each container is a profile of its own (cookies, storage, cache,
    // permissions), created when its first tab opens. Not in private windows.
    readonly property bool containersEnabled: !!liveSettings["privacy.containers"] && !Browser.isPrivate()
    readonly property var containers: liveSettings["privacy.containerList"] || []
    function containerInfo(id) { return containers.find(c => c.id === id) || null }
    // id → WebEngineProfile (read by bootstrap.cpp to install network hooks).
    // Filled in place, without change notifications: tabs choose their
    // profile in a binding, which must not depend on this.
    readonly property var containerProfiles: ({})
    property Component _containerProto: Component {
        WebEngineProfilePrototype {
            persistentCookiesPolicy: WebEngineProfile.AllowPersistentCookies
            persistentPermissionsPolicy: WebEngineProfile.StoreOnDisk
        }
    }
    function containerProfile(id) {
        if (containerProfiles[id]) return containerProfiles[id]
        // Next to the main profile's storage: <profile>/webengine-containers/<id>
        const proto = _containerProto.createObject(app, {storageName: "container-" + id, persistentStoragePath: Browser.webEnginePath() + "-containers/" + id})
        const p = proto.instance()
        containerProfiles[id] = p
        Browser.hookContainerProfiles()
        configureProfile(p)
        installUserScripts()
        return p
    }
    // The profile for a tab of a window (private, a container, or the main
    // one). Only looks it up: openTab() creates a container's profile first.
    function profileFor(privateMode, container) {
        if (privateMode) return privateProfile
        return (container && containerProfiles[container]) || profile
    }
    function allProfiles() { return [profile, privateProfile].concat(Object.values(containerProfiles)) }

    property var windows: []
    property var activeWindow: null
    property int nextTabId: 1
    property int nextWindowId: 1
    property bool quitting: false
    property var settingsCache: ({})

    Component.onCompleted: {
        profile = _proto.instance()
        privateProfile = _privateProto.instance()
        for (const p of [profile, privateProfile]) configureProfile(p)
        // Objects containing web views need the profile to exist first.
        optionsDialog = _optionsComp.createObject(null, {profile: profile})
        authFlow = _authComp.createObject(null, {profile: profile})
    }

    function setting(key) { return JSON.parse(Browser.setting(key)) }

    // Settings apply live: web views and windows bind to `liveSettings`
    // (e.g. liveSettings["web.javascript"]) and profile settings are
    // re-applied, so a change in Preferences reaches open tabs and windows
    // right away.
    readonly property var liveKeys: ["web.javascript", "web.autoloadImages", "web.localStorage", "web.pluginsEnabled",
        "web.spatialNavigation", "web.smoothScrolling", "web.dnsPrefetch", "web.printBackgrounds", "appearance.forceDarkMode",
        "appearance.showBookmarksToolbar", "appearance.showStatusBar", "appearance.showMenuBar", "toolbar.items", "extensions.unpinned", "tabs.position", "tabs.hideWhenSingle", "privacy.webrtcPublicOnly", "privacy.containers", "privacy.containerList", "privacy.siteCookieRules", "privacy.antiFingerprinting", "general.homepageMode", "general.newTabPageMode", "startpage.home.file", "startpage.home.copy", "startpage.newtab.file", "startpage.newtab.copy"]
    readonly property var profileKeys: ["web.userAgent", "web.spellcheck", "web.spellcheckLanguages", "downloads.directory", "privacy.genericUserAgent"]
    property var liveSettings: liveKeys.reduce((o, k) => { o[k] = setting(k); return o }, {})
    property Connections _settingsConn: Connections {
        target: Browser
        function onSettingChanged(key, json) {
            if (app.liveKeys.includes(key)) {
                const o = Object.assign({}, app.liveSettings)
                o[key] = JSON.parse(json)
                app.liveSettings = o
            }
            if (app.profileKeys.includes(key)) for (const p of app.allProfiles()) app.applyProfileSettings(p)
            if (["web.doNotTrack", "privacy.globalPrivacyControl", "privacy.blockPings", "privacy.antiFingerprinting"].includes(key)) app.installUserScripts()
        }
    }

    // Screen Wake Lock decisions per site for this session (WebTab).
    property var wakeLockSites: ({})
    function rememberWakeLock(key, allowed) { const o = Object.assign({}, wakeLockSites); o[key] = allowed; wakeLockSites = o }
    function forgetWakeLock(key) { const o = Object.assign({}, wakeLockSites); delete o[key]; wakeLockSites = o }

    // Qt's own user agent, restored when the setting is cleared.
    property string defaultUserAgent: ""
    function applyProfileSettings(p) {
        // A user agent of the user's own, or QtWebEngine's; that one names
        // "QtWebEngine/6.x", which few browsers send and so helps identify
        // the user: without it, it reads like Chrome ("privacy.genericUserAgent").
        const ua = setting("web.userAgent")
        p.httpUserAgent = ua ? ua : setting("privacy.genericUserAgent") ? defaultUserAgent.replace(/ ?QtWebEngine\/[\d.]+/, "") : defaultUserAgent
        p.downloadPath = setting("downloads.directory")
        p.spellCheckEnabled = setting("web.spellcheck")
        p.spellCheckLanguages = setting("web.spellcheckLanguages")
    }

    function configureProfile(p) {
        if (defaultUserAgent === "") defaultUserAgent = p.httpUserAgent
        applyProfileSettings(p)
        p.downloadRequested.connect(onDownloadRequested)
        p.presentNotification.connect(onWebNotification)
    }

    // Called (queued) by the C++ bootstrap once the profiles are hooked up.
    function start() {
        installUserScripts()
        Browser.reloadAdblock(true)
        const urls = JSON.parse(Browser.initialUrls())
        const startup = setting("general.startup")
        const session = JSON.parse(Browser.lastSession())
        const crashed = Browser.crashedLastTime()
        Browser.setRunning(true)
        if (!Browser.isPrivate() && (startup === "restore" || crashed) && session.length > 0) {
            for (const w of session) restoreWindow(w)
            if (urls.length) for (const u of urls) activeWindow.openTab(u, {background: false})
        } else {
            // "Open start page": the new tab page; "Open home page": the home page.
            const first = urls.length ? urls : [startup === "blank" ? "about:blank" : startup === "speeddial" ? newTabUrl() : homeUrl()]
            newWindow({urls: first})
        }
        backgroundHost.reload()
    }

    // ---- windows ------------------------------------------------------------
    property Component windowComponent: Component { BrowserWindow {} }

    function newWindow(opts) {
        opts = opts || {}
        const w = windowComponent.createObject(null, {
            app: app,
            windowId: nextWindowId++,
            privateMode: !!opts.privateMode || Browser.isPrivate(),
        })
        windows = windows.concat([w])
        if (opts.geometry) { w.x = opts.geometry[0]; w.y = opts.geometry[1]; w.width = opts.geometry[2]; w.height = opts.geometry[3] }
        Extensions.browserEvent("windowCreated", JSON.stringify({window: w.extInfo()}))
        if (opts.maximized) w.showMaximized(); else w.show()
        const urls = opts.urls || []
        if (opts.tabs) {
            for (const t of opts.tabs) w.restoreTab(t)
            // (Split view: each pane's current tab too.)
            w.restorePanes(opts.saved || {current: opts.current})
        } else if (urls.length === 0 && !opts.empty) {
            w.openTab(newTabUrl(), {background: false})
        }
        for (const u of urls) w.openTab(u, {background: false})
        activeWindow = w
        return w
    }

    function restoreWindow(saved) {
        return newWindow({tabs: saved.tabs, current: saved.current, saved: saved, geometry: saved.geometry, maximized: saved.maximized})
    }

    function windowClosed(w) {
        // Closing the last window quits Kite: it is the session to restore,
        // so save it while it is still listed (not as a closed window).
        const last = !quitting && windows.length === 1 && windows[0] === w
        if (last) saveSession()
        windows = windows.filter(x => x !== w)
        Extensions.browserEvent("windowRemoved", JSON.stringify({windowId: w.windowId}))
        if (!quitting && !last && !w.privateMode) Browser.pushClosedWindow(JSON.stringify(w.saveState()))
        if (activeWindow === w) activeWindow = windows.length ? windows[windows.length - 1] : null
        if (last) quit(true)
        else if (!quitting) saveSession()
    }

    function windowActivated(w) {
        activeWindow = w
        Extensions.browserEvent("windowFocused", JSON.stringify({windowId: w.windowId}))
    }

    // ---- home and new tab page ----------------------------------------------
    // Each is an address or a local HTML file ("general.homepageMode",
    // "general.newTabPageMode": "url" | "file"). A file is used in place
    // (file:) or as its copy in the profile (kite:startpage/<target>/);
    // targets are "home" and "newtab".
    function startPageUrl(target) {
        if (liveSettings["startpage." + target + ".copy"]) return "kite:startpage/" + target + "/"
        const f = liveSettings["startpage." + target + ".file"] || ""
        return f ? "file://" + encodeURI(f) : ""
    }
    function startPageFile(target) {
        return liveSettings["startpage." + target + ".copy"] ? Browser.startPageCopyPath(target) : (liveSettings["startpage." + target + ".file"] || "")
    }
    function usesFile(target) {
        return liveSettings[target === "home" ? "general.homepageMode" : "general.newTabPageMode"] === "file" && startPageUrl(target) !== ""
    }
    function homeUrl() {
        return usesFile("home") ? startPageUrl("home") : setting("general.homepage")
    }
    function newTabUrl() {
        const o = Extensions.newTabOverride()
        if (o !== "") return o
        return usesFile("newtab") ? startPageUrl("newtab") : setting("general.newTabPage")
    }
    // Whether `url` is a custom start page in use (tabs report file: URLs
    // decoded).
    function isStartPage(url) {
        if (!url) return false
        const plain = u => { try { return decodeURI(u.split("#")[0]) } catch (e) { return u.split("#")[0] } }
        return ["home", "newtab"].some(t => usesFile(t) && plain(url) === plain(startPageUrl(t)))
    }
    // Saving a start page file (e.g. in an editor) reloads the tabs showing it.
    property var _startPageModified: ({})
    property Timer _startPageWatch: Timer {
        interval: 1500
        repeat: true
        running: app.usesFile("home") || app.usesFile("newtab")
        onTriggered: {
            for (const target of ["home", "newtab"]) {
                if (!app.usesFile(target)) continue
                const m = Browser.fileModified(app.startPageFile(target))
                const before = app._startPageModified[target]
                app._startPageModified[target] = m
                if (before === undefined || m === before) continue
                const url = app.startPageUrl(target)
                const plain = u => { try { return decodeURI(u.split("#")[0]) } catch (e) { return u } }
                for (const w of app.windows) for (let i = 0; i < w.count; ++i) {
                    const t = w.tabAt(i)
                    if (t && t.pendingUrl === "" && plain(t.url) === plain(url)) t.view.reload()
                }
            }
        }
    }

    function findTab(tabId) {
        for (const w of windows) { const t = w.tabById(tabId); if (t) return t }
        return null
    }
    function findWindow(windowId) {
        return windows.find(w => w.windowId === windowId) || null
    }
    function currentWindowFor(windowId) {
        return (windowId !== undefined && windowId >= 0 ? findWindow(windowId) : null) || activeWindow || windows[0]
    }

    function openUrl(url, where) {
        const w = activeWindow || newWindow({empty: true})
        if (where === "window") return newWindow({urls: [url]})
        if (where === "current" && w.currentTab) { w.currentTab.load(url); return w.currentTab }
        return w.openTab(url, {background: where === "background"})
    }

    function quit(sessionSaved) {
        if (quitting) return
        quitting = true
        if (!sessionSaved) saveSession()
        Browser.setRunning(false)
        Browser.deleteSessionCookies()
        if (setting("privacy.clearOnExit")) Browser.clearPrivateData(JSON.stringify({history: true, cookies: true, cache: true, downloads: true, closedTabs: true}))
        for (const w of windows.slice()) w.destroy()
        Qt.quit()
    }

    // ---- session --------------------------------------------------------------
    function saveSession() {
        if (Browser.isPrivate()) return
        Browser.saveSession(JSON.stringify(windows.filter(w => !w.privateMode).map(w => w.saveState())))
    }
    property Timer sessionTimer: Timer { interval: 15000; repeat: true; running: true; onTriggered: app.saveSession() }

    // ---- extensions -------------------------------------------------------------
    // Page scripts: extension content scripts and the extension bridge
    // (Extensions.userScripts) plus Kite's own. Only what changed is updated:
    // removing everything and inserting it again (on every extension script
    // change, e.g. uBlock Origin registering scriptlets while a page loads)
    // left a moment in which a page being created got no scripts at all.
    // Custom CSS and JavaScript of sites (Settings → Site Settings), as
    // scripts limited to the site's host. CSS goes in as the document is
    // created (no unstyled flash); JavaScript runs in the page's own world
    // once the DOM is ready, like a user script.
    // A seed per site for this session (fingerprint protection).
    readonly property int _fingerprintSalt: Math.floor(Math.random() * 0x7fffffff)
    function fingerprintSeed(host) {
        let h = _fingerprintSalt
        for (let i = 0; i < host.length; ++i) h = (Math.imul(h, 31) + host.charCodeAt(i)) >>> 0
        return h >>> 0
    }
    function siteScripts() {
        const out = []
        for (const e of JSON.parse(Browser.allSiteSettings())) {
            const header = "// ==UserScript==\n// @match *://" + e.host + "/*\n// ==/UserScript==\n"
            if (e.settings.css)
                out.push({name: "kite:site-css:" + e.host, worldId: 1, runsOnSubFrames: false, injectionPoint: 0, sourceCode: header
                    + "(() => { const add = () => { const s = document.createElement('style'); s.dataset.kite = 'site-css'; s.textContent = " + JSON.stringify(e.settings.css) + "; document.documentElement.appendChild(s) };"
                    + " if (document.documentElement) add(); else new MutationObserver((_, o) => { if (document.documentElement) { o.disconnect(); add() } }).observe(document, {childList: true}) })();"})
            if (e.settings.js)
                out.push({name: "kite:site-js:" + e.host, worldId: 0, runsOnSubFrames: false, injectionPoint: 1, sourceCode: header + e.settings.js})
            // Fingerprint protection: before the page's scripts, in all its
            // frames; the noise differs per site and per session.
            if (e.settings.fingerprint === 1 && setting("privacy.antiFingerprinting"))
                out.push({name: "kite:site-fp:" + e.host, worldId: 0, runsOnSubFrames: true, injectionPoint: 0, sourceCode: header + Browser.fingerprintScript(fingerprintSeed(e.host))})
        }
        return out
    }
    // A site's settings were changed (Settings, the site info popup): scripts
    // are rebuilt, open tabs of the site pick up zoom and dark mode.
    signal siteSettingsEdited(string host)
    function siteSettingsChanged(host) {
        installUserScripts()
        siteSettingsEdited(host)
    }
    function showSiteSettings(host) {
        const w = showPreferences("sites")
        w.openSite(host)
        return w
    }

    function installUserScripts() {
        const sites = siteScripts()
        for (const p of allProfiles()) {
            // Like Firefox, extensions don't run in private windows by default.
            const wanted = JSON.parse(Extensions.userScripts(p !== privateProfile))
            // navigator.wakeLock (QtWebEngine refuses wake locks itself).
            wanted.push({name: "kite:wakeLock", sourceCode: Browser.wakeLockScript(), worldId: 0, runsOnSubFrames: false, injectionPoint: 0})
            // Overlay scrollbars also for pages styling their own.
            const scrollbars = Browser.scrollbarScript()
            if (scrollbars) wanted.push({name: "kite:scrollbars", sourceCode: scrollbars, worldId: 1, runsOnSubFrames: true, injectionPoint: 0})
            // The DNT header is added by the request interceptor; pages also
            // see the preference in navigator.doNotTrack.
            if (setting("web.doNotTrack"))
                wanted.push({name: "kite:doNotTrack", worldId: 0, runsOnSubFrames: true, injectionPoint: 0,
                             sourceCode: "Object.defineProperty(Navigator.prototype, 'doNotTrack', { get() { return '1'; }, configurable: true, enumerable: true });"})
            // Global Privacy Control (the Sec-GPC header is added by the
            // request interceptor).
            if (setting("privacy.globalPrivacyControl"))
                wanted.push({name: "kite:gpc", worldId: 0, runsOnSubFrames: true, injectionPoint: 0,
                             sourceCode: "Object.defineProperty(Navigator.prototype, 'globalPrivacyControl', { get() { return true; }, configurable: true, enumerable: true });"})
            // <a ping> beacons: the attribute is dropped as a link is clicked
            // (blocking "ping" requests would also stop navigator.sendBeacon).
            if (setting("privacy.blockPings"))
                wanted.push({name: "kite:noPings", worldId: 1, runsOnSubFrames: true, injectionPoint: 0,
                             sourceCode: "for (const t of ['click', 'auxclick']) document.addEventListener(t, e => { const a = e.target instanceof Element && e.target.closest('a[ping], area[ping]'); if (a) a.removeAttribute('ping'); }, true);"})
            for (const s of sites) wanted.push(s)
            updateScripts(p.userScripts, wanted)
        }
    }
    function updateScripts(collection, wanted) {
        const point = i => i === 0 ? WebEngineScript.DocumentCreation : i === 1 ? WebEngineScript.DocumentReady : WebEngineScript.Deferred
        const names = new Set(wanted.map(s => s.name))
        for (const old of collection.collection) {
            if (!names.has(old.name)) collection.remove(old)
        }
        for (const s of wanted) {
            const have = collection.find(s.name)
            const unchanged = have.length === 1 && have[0].sourceCode === s.sourceCode && have[0].worldId === s.worldId
                && have[0].runsOnSubFrames === s.runsOnSubFrames && have[0].injectionPoint === point(s.injectionPoint)
            if (unchanged) continue
            for (const old of have) collection.remove(old)
            const ws = WebEngine.script()
            ws.name = s.name
            ws.sourceCode = s.sourceCode
            ws.worldId = s.worldId
            ws.runsOnSubFrames = s.runsOnSubFrames
            ws.injectionPoint = point(s.injectionPoint)
            collection.insert(ws)
        }
    }

    property Connections _extConn: Connections {
        target: Extensions
        function onChanged(what) {
            if (what === "scripts") app.installUserScripts()
            if (what === "background") backgroundHost.reload()
        }
        function onUiRequest(view, ctx, callId, ext, method, args) {
            let result
            try {
                result = app.handleExtensionRequest(view, ctx, callId, ext, method, JSON.parse(args))
            } catch (e) {
                Extensions.uiReply(view, ctx, callId, false, JSON.stringify(String(e.message || e)))
                return
            }
            // `undefined` means the handler replies asynchronously.
            if (result !== undefined) Extensions.uiReply(view, ctx, callId, true, JSON.stringify(result))
        }
        function onInstallPrompt(request, json) {
            installDialog.show(request, JSON.parse(json))
        }
        function onInstallStatus(json) {
            const s = JSON.parse(json)
            if (s.state === "downloading") app.toast(s.message)
            else if (s.state === "installed") app.toast(s.message, "dialog-ok")
            else if (s.state === "error") app.toast(s.message, "dialog-error")
        }
    }

    property Timer alarmTimer: Timer { interval: 1000; repeat: true; running: true; onTriggered: Extensions.tick() }

    // Console messages of web pages and extension pages, logged as
    // console::page / console::extension (see kite_log, RUST_LOG).
    property LoggingCategory pageConsole: LoggingCategory { name: "console.page"; defaultLogLevel: LoggingCategory.Debug }
    property LoggingCategory extensionConsole: LoggingCategory { name: "console.extension"; defaultLogLevel: LoggingCategory.Debug }
    function logConsole(extension, prefix, level, message, source, line) {
        const cat = extension ? extensionConsole : pageConsole
        const text = (prefix ? "[" + prefix + "] " : "") + message + (source ? " (" + source + ":" + line + ")" : "")
        if (level === WebEngineView.ErrorMessageLevel) console.error(cat, text)
        else if (level === WebEngineView.WarningMessageLevel) console.warn(cat, text)
        else console.info(cat, text)
    }

    // Web pages wait for content blockers (e.g. uBlock Origin) to load their
    // filters at startup, so the first pages aren't loaded unfiltered.
    property bool networkReady: false
    property Timer networkReadyTimer: Timer {
        interval: 50; repeat: true; running: !app.networkReady
        readonly property double started: Date.now()
        onTriggered: if (Extensions.networkReady() || Date.now() - started > 10000) app.networkReady = true
    }

    function tabOrThrow(id) {
        const t = findTab(id)
        if (!t) throw new Error("Invalid tab ID: " + id)
        return t
    }

    // Executes extension API calls that need the GUI. Return a value to reply
    // immediately, or `undefined` and call Extensions.uiReply later.
    function handleExtensionRequest(view, ctx, callId, ext, method, a) {
        const reply = (ok, v) => Extensions.uiReply(view, ctx, callId, ok, JSON.stringify(v === undefined ? null : v))
        switch (method) {
        case "tabs.create": {
            const w = currentWindowFor(a.windowId)
            const t = w.openTab(a.url || newTabUrl(), {background: a.active === false, index: a.index, pinned: a.pinned, openerTabId: a.openerTabId})
            return t.extInfo()
        }
        case "tabs.update": {
            const t = tabOrThrow(a.tabId), p = a.props || {}
            if (p.url) t.load(p.url)
            if (p.active || p.highlighted) t.activate()
            if (p.pinned !== undefined) t.setPinned(p.pinned)
            if (p.muted !== undefined) t.view.audioMuted = p.muted
            return t.extInfo()
        }
        case "tabs.remove":
            for (const id of a.tabIds) { const t = findTab(id); if (t) t.close() }
            return null
        case "tabs.reload": { const t = tabOrThrow(a.tabId); (a.props && a.props.bypassCache) ? t.view.reloadAndBypassCache() : t.view.reload(); return null }
        case "tabs.goBack": tabOrThrow(a.tabId).view.goBack(); return null
        case "tabs.goForward": tabOrThrow(a.tabId).view.goForward(); return null
        case "tabs.duplicate": { const t = tabOrThrow(a.tabId); return t.window.openTab(t.url, {background: false, index: t.index + 1}).extInfo() }
        case "tabs.getZoom": return tabOrThrow(a.tabId).view.zoomFactor
        case "tabs.setZoom": { const t = tabOrThrow(a.tabId); t.setZoom(a.zoom > 0 ? a.zoom : 1.0); return null }
        case "tabs.move": {
            const out = []
            for (const id of a.tabIds) {
                const t = tabOrThrow(id)
                t.window.moveTab(t.index, a.props.index < 0 ? t.window.count - 1 : a.props.index)
                out.push(t.extInfo())
            }
            return out
        }
        case "tabs.print": tabOrThrow(a.tabId).window.printTab(); return null
        case "tabs.saveAsPDF": tabOrThrow(a.tabId).window.printTab(); return "saved"
        case "tabs.toggleReaderMode": throw new Error("Reader mode is not supported")
        case "tabs.capture": {
            const t = tabOrThrow(a.tabId)
            t.view.grabToImage(r => {
                const fmt = (a.options && a.options.format === "jpeg") ? "jpg" : "png"
                const path = Browser.tempFile("capture-" + Date.now() + "." + fmt)
                r.saveToFile(path)
                reply(true, Browser.fileToDataUrl(path))
            })
            return undefined
        }
        case "tabs.runInWorld": {
            const t = tabOrThrow(a.tabId)
            t.view.runJavaScript(a.code, a.world, r => {
                if (a.scripting) reply(true, [{frameId: 0, result: r === undefined ? null : r}])
                else reply(true, a.wrapResult ? [r === undefined ? null : r] : [])
            })
            return undefined
        }
        case "windows.create": {
            const d = a[0] || {}
            const urls = d.url ? (Array.isArray(d.url) ? d.url : [d.url]) : []
            const w = newWindow({urls: urls, privateMode: d.incognito, empty: urls.length === 0 && d.tabId !== undefined})
            if (d.tabId !== undefined) { const t = findTab(d.tabId); if (t) w.openTab(t.url, {}) ; if (t) t.close() }
            if (d.width) w.width = d.width
            if (d.height) w.height = d.height
            if (d.state === "maximized") w.showMaximized()
            if (d.state === "fullscreen") w.showFullScreen()
            return w.extInfo(true)
        }
        case "windows.update": {
            const w = findWindow(a[0]); if (!w) throw new Error("Invalid window ID: " + a[0])
            const d = a[1] || {}
            if (d.focused) { w.raise(); w.requestActivate() }
            if (d.state === "minimized") w.showMinimized()
            if (d.state === "maximized") w.showMaximized()
            if (d.state === "normal") w.showNormal()
            if (d.state === "fullscreen") w.showFullScreen()
            if (d.width) w.width = d.width
            if (d.height) w.height = d.height
            if (d.left !== undefined) w.x = d.left
            if (d.top !== undefined) w.y = d.top
            return w.extInfo()
        }
        case "windows.remove": { const w = findWindow(a[0]); if (w) w.close(); return null }
        case "openOptions": {
            if (a.inTab) openUrl(a.url, "tab")
            else optionsDialog.open(a.url, a.name)
            return null
        }
        case "action.openPopup": {
            const w = activeWindow
            if (w) w.openExtensionPopup(ext)
            return null
        }
        case "sidebar.open": case "sidebar.toggle": case "sidebar.close": case "sidebar.isOpen": {
            const w = activeWindow; if (!w) return false
            if (method === "sidebar.isOpen") return w.sidebarExtension === ext
            if (method === "sidebar.close" || (method === "sidebar.toggle" && w.sidebarExtension === ext)) w.showSidebar("")
            else w.showSidebar("ext:" + ext)
            return null
        }
        case "notifications.create": {
            showNotification(a.options.title || a.name, a.options.message || "", a.options.iconUrl, () => {
                Extensions.browserEvent("notificationClicked", JSON.stringify({ext: ext, id: a.id}))
            }, () => Extensions.browserEvent("notificationClosed", JSON.stringify({ext: ext, id: a.id, byUser: true})))
            return a.id
        }
        case "permissions.request":
            permissionDialog.ask(a, ok => {
                if (ok) Extensions.grantPermissions(ext, JSON.stringify(a.permissions))
                reply(true, ok)
            })
            return undefined
        case "management.uninstallSelf":
            Extensions.uninstall(ext)
            return null
        case "downloads.open": case "downloads.show": {
            const rec = JSON.parse(Browser.downloads()).find(r => r.id === a[0])
            if (rec) Qt.openUrlExternally("file://" + (method === "downloads.show" ? rec.path.replace(/\/[^/]*$/, "") : rec.path))
            return null
        }
        case "downloads.showDefaultFolder": Qt.openUrlExternally("file://" + setting("downloads.directory")); return null
        case "downloads.pause": case "downloads.resume": case "downloads.cancel": {
            const d = activeDownloads[a[0]]
            if (d) { if (method === "downloads.pause") d.pause(); else if (method === "downloads.resume") d.resume(); else d.cancel() }
            return null
        }
        case "search.search": {
            const url = Browser.searchUrl(a.engine || setting("general.searchEngine"), a.query || a.text || "")
            const where = a.disposition === "NEW_WINDOW" ? "window" : a.disposition === "CURRENT_TAB" ? "current" : "tab"
            if (a.tabId !== undefined) tabOrThrow(a.tabId).load(url); else openUrl(url, where)
            return null
        }
        case "sessions.restore": {
            const t = Browser.popClosedTab(0)
            const tab = JSON.parse(t)
            if (tab && tab.url) return activeWindow.restoreTab(tab).extInfo()
            return null
        }
        case "clipboard.setImageData": return null
        case "identity.launchWebAuthFlow":
            authFlow.start(a, url => reply(true, url), err => reply(false, err))
            return undefined
        }
        throw new Error(method + " is not supported by Kite yet")
    }

    // Background pages: one hidden WebEngineView per extension.
    property Window backgroundHost: Window {
        id: bgWin
        visible: false
        width: 400; height: 300
        title: "Kite extension background pages"
        property var pages: []
        function reload() { pages = JSON.parse(Extensions.backgroundPages()) }
        Repeater {
            model: bgWin.pages
            delegate: WebEngineView {
                required property var modelData
                width: 400; height: 300
                profile: app.profile
                url: modelData.url
                settings.localContentCanAccessRemoteUrls: true
                settings.javascriptCanAccessClipboard: true
                webChannel: WebChannel { registeredObjects: [bgBridge] }
                ExtBridge { id: bgBridge; viewKey: "bg:" + modelData.ext; viewKind: "background" }
                onJavaScriptConsoleMessage: (level, message, line, source) => app.logConsole(true, modelData.name, level, message, source, line)
                onRenderProcessTerminated: (status, code) => { console.warn("Background page of " + modelData.name + " crashed; reloading"); reload() }
            }
        }
    }

    // ---- downloads --------------------------------------------------------------
    property var activeDownloads: ({})
    property var pendingDownloadNames: ({})

    function onDownloadRequested(download) {
        const url = download.url.toString()
        // Firefox add-ons: install instead of saving.
        if (url.endsWith(".xpi") || download.mimeType === "application/x-xpinstall") {
            download.cancel()
            Extensions.installFromUrl(url)
            return
        }
        const dir = setting("downloads.directory")
        const name = pendingDownloadNames[url] || download.suggestedFileName || download.downloadFileName
        delete pendingDownloadNames[url]
        const path = Browser.uniqueDownloadPath(dir, name)
        download.downloadDirectory = path.substring(0, path.lastIndexOf("/"))
        download.downloadFileName = path.substring(path.lastIndexOf("/") + 1)
        if (setting("downloads.askWhereToSave")) {
            saveDialog.ask(download)
            return
        }
        startDownload(download)
    }

    function startDownload(download) {
        const id = Browser.downloadUpdate(JSON.stringify({id: 0, url: download.url.toString(),
            path: download.downloadDirectory + "/" + download.downloadFileName, mime: download.mimeType,
            total: download.totalBytes, received: 0, state: "in_progress"}))
        activeDownloads[id] = download
        download.accept()
        let last = 0
        download.receivedBytesChanged.connect(() => {
            const now = Date.now()
            if (now - last < 500) return
            last = now
            Browser.downloadUpdate(JSON.stringify({id: id, received: download.receivedBytes, total: download.totalBytes}))
        })
        download.isFinishedChanged.connect(() => {
            if (!download.isFinished) return
            const st = download.state === WebEngineDownloadRequest.DownloadCompleted ? "complete"
                     : download.state === WebEngineDownloadRequest.DownloadCancelled ? "cancelled" : "interrupted"
            Browser.downloadUpdate(JSON.stringify({id: id, received: download.receivedBytes, total: download.totalBytes, state: st}))
            delete activeDownloads[id]
            if (st === "complete") showNotification("Download complete", download.downloadFileName, "", () => Qt.openUrlExternally("file://" + download.downloadDirectory + "/" + download.downloadFileName))
        })
        if (activeWindow) activeWindow.downloadStarted()
    }

    // ---- notifications ------------------------------------------------------------
    function onWebNotification(n) {
        showNotification(n.title, n.message, "", () => n.click(), () => n.close())
        n.show()
    }

    property Component notificationComponent: Component {
        Notification {
            componentName: "kite"
            eventId: "notification"
            iconName: "internet-web-browser"
            autoDelete: true
        }
    }
    function showNotification(title, text, iconUrl, onClick, onClose) {
        const n = notificationComponent.createObject(app, {title: title, text: text})
        if (onClick) {
            const action = n.defaultAction
            if (action) action.activated.connect(onClick)
        }
        if (onClose) n.closed.connect(onClose)
        n.sendEvent()
    }

    function toast(message, icon) {
        if (activeWindow) activeWindow.toast(message, icon || "")
        else console.log(message)
    }

    // ---- dialogs (shared) --------------------------------------------------------------
    property InstallDialog installDialog: InstallDialog {}
    property PermissionDialog permissionDialog: PermissionDialog {}
    property MasterPasswordDialog masterPasswordDialog: MasterPasswordDialog {}
    function unlockPasswords(cb) { masterPasswordDialog.ask(cb || (() => {})) }
    property var optionsDialog: null
    property Component _optionsComp: Component { ExtensionOptionsDialog {} }
    property SaveDialog saveDialog: SaveDialog { onAccepted2: d => app.startDownload(d) }
    property var authFlow: null
    property Component _authComp: Component { AuthFlow {} }

    property Component libraryComponent: Component { LibraryWindow {} }
    property Component downloadsComponent: Component { DownloadsWindow {} }
    property Component preferencesComponent: Component { PreferencesWindow {} }
    property Component addonsComponent: Component { AddonsWindow {} }
    property var _singletons: ({})
    function showSingleton(name, comp, props) {
        let w = _singletons[name]
        if (!w) {
            w = comp.createObject(null, Object.assign({app: app}, props || {}))
            _singletons[name] = w
            w.closing.connect(() => { delete _singletons[name]; w.destroy() })
        } else if (props) {
            for (const k in props) w[k] = props[k]
        }
        w.show(); w.raise(); w.requestActivate()
        return w
    }
    function showLibrary(page) { return showSingleton("library", libraryComponent, {page: page || "history"}) }
    function showDownloads() { return showSingleton("downloads", downloadsComponent) }
    function showPreferences(page) { return showSingleton("preferences", preferencesComponent, {page: page || "general"}) }
    function showAddons(page) { return showSingleton("addons", addonsComponent, {page: page || "installed"}) }

}
