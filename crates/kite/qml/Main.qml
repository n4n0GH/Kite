import QtCore
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

    // Optional features (crates/kite/plugins/, and those installed from a file).
    // What a plugin hasn't had set comes from its plugin.json.
    property var plugins: JSON.parse(Browser.plugins())
    function reloadPlugins() {
        plugins = JSON.parse(Browser.plugins())
        pluginActions = enabledPluginActions()
        installUserScripts()
    }
    // Deletes an installed plugin and what's stored for it in this profile.
    function removePlugin(plugin) {
        const error = Browser.removePlugin(plugin.id)
        if (error) return error
        reloadPlugins()
        return ""
    }
    // --no-plugins: all of them count as off, and what's stored stays as it is.
    readonly property bool pluginsDisabled: Browser.pluginsDisabled()
    function pluginEnabled(plugin) {
        if (pluginsDisabled) return false
        const on = setting("plugins." + plugin.id + ".enabled")
        return on === null ? plugin.defaultEnabled : !!on
    }
    function setPluginEnabled(plugin, on) { if (!pluginsDisabled) Browser.setSetting("plugins." + plugin.id + ".enabled", JSON.stringify(on)) }
    function pluginSetting(plugin, key) {
        const v = setting("plugins." + plugin.id + "." + key)
        return v === null ? plugin.settings[key] : v
    }
    function setPluginSetting(plugin, key, value) { Browser.setSetting("plugins." + plugin.id + "." + key, JSON.stringify(value)) }
    function pluginUrl(plugin, file) { return plugin.baseUrl + file }
    // Plugins' items for the whole browser, while they're on.
    property Instantiator pluginApps: Instantiator {
        model: app.plugins.filter(p => p.app)
        delegate: Loader {
            id: pluginAppLoader
            required property var modelData
            function sync() {
                if (!app.pluginEnabled(modelData)) source = ""
                else if (!item) setSource(app.pluginUrl(modelData, modelData.app), {app: app, plugin: modelData})
            }
            Component.onCompleted: sync()
            property Connections settingWatch: Connections {
                target: Browser
                function onSettingChanged(key) { if (key === "plugins." + pluginAppLoader.modelData.id + ".enabled") pluginAppLoader.sync() }
            }
        }
    }
    // The page scripts of the plugins that are on. They get their plugin's settings,
    // and a way to tell the window: a console message with the secret, as forms in
    // frames do, which pages can neither see nor imitate. It reaches
    // BrowserWindow.pluginMessage.
    function pluginScripts() {
        const out = []
        for (const p of plugins) {
            if (!pluginEnabled(p)) continue
            const settings = {}
            for (const key of Object.keys(p.settings)) settings[key] = pluginSetting(p, key)
            p.pageScripts.forEach((s, i) => {
                const files = s.files.map(f => Browser.pluginFile(f).replace(".pragma library", "")).join("\n")
                // The page's own world gets neither: the page could read them there,
                // and would then be able to imitate the plugin's messages.
                const send = "const kitePluginSend = m => console.debug(" + JSON.stringify(frameSecret + "plugin:") + " + JSON.stringify({plugin: " + JSON.stringify(p.id) + ", m}));"
                const given = s.worldId === 0 ? "" : "const KITE_PLUGIN_SETTINGS = " + JSON.stringify(settings) + ";\n" + send + "\n"
                out.push({name: "kite:plugin:" + p.id + ":" + i, worldId: s.worldId, runsOnSubFrames: s.runsOnSubFrames, injectionPoint: s.injectionPoint,
                          sourceCode: "(() => {\n" + given + files + "\n})();"})
            })
        }
        return out
    }

    // ---- picture-in-picture (PipWindow.qml, bridge.js __kitePip) ----
    // One at a time: the tab whose page is shown in the picture-in-picture
    // window. Its WebEngineView moves into the window and back.
    property var pipTab: null
    readonly property var pipPlugin: plugins.find(p => p.id === "pip")
    readonly property bool pipAvailable: pluginActions.some(a => a.id === "pip.toggle")
    property PipWindow pipWindow: null
    property Component _pipComp: Component { PipWindow {} }
    // Show the video at (x, y) of the tab's page (CSS px; -1: its main video).
    function startPip(tab, x, y) {
        if (!tab || tab.unloaded) return
        if (pipTab === tab) { stopPip(true); return }
        if (pipTab) stopPip(false)
        tab.view.runJavaScript("globalThis.__kitePip ? __kitePip.enter(" + (x === undefined ? -1 : x) + ", " + (y === undefined ? -1 : y) + ") : {ok: false}", 1, r => {
            if (!r || !r.ok) { tab.window.toast("No video to show in picture-in-picture on this page", "dialog-information"); return }
            if (!pipWindow) {
                pipWindow = _pipComp.createObject(null)
                pipWindow.backToTab.connect(() => app.stopPip(true))
                pipWindow.closeRequested.connect(() => app.stopPip(false, true))
            }
            // A quarter of the screen's width, the video's shape.
            const sw = tab.window.screen ? tab.window.screen.desktopAvailableWidth : 1920
            const sh = tab.window.screen ? tab.window.screen.desktopAvailableHeight : 1080
            const w = Math.round(Math.max(320, sw / 4))
            pipWindow.width = w
            pipWindow.height = Math.round(w * r.height / r.width)
            pipWindow.x = sw - pipWindow.width - 24
            pipWindow.y = sh - pipWindow.height - 24
            pipWindow.pageTitle = tab.title
            pipTab = tab
            tab.moveViewTo(pipWindow.area)
            pipWindow.show()
            pipWindow.raise()
            // Wayland doesn't let a window ask for any of this by itself.
            Browser.placePipWindow(pipWindow.title, pipWindow.minimumWidth, pluginSetting(pipPlugin, "allDesktops"))
        })
    }
    // `back`: show the tab again (the video keeps playing); `pause`: closed.
    function stopPip(back, pause) {
        const tab = pipTab
        if (!tab) return
        pipTab = null
        tab.moveViewTo(null)
        tab.view.runJavaScript("globalThis.__kitePip && __kitePip.exit(" + (pause ? "true" : "false") + ")", 1)
        if (pipWindow) pipWindow.hide()
        if (back) tab.activate()
    }

    // ---- keyboard shortcuts (Settings → Shortcuts) ----
    // Every action with its default keys (portable key sequences); the user's
    // changes are in "shortcuts" (id → keys). BrowserWindow binds them.
    // Plugins' actions are there while they are on.
    readonly property var shortcutActions: coreActions.concat(pluginActions)
    property var pluginActions: enabledPluginActions()
    function enabledPluginActions() { return plugins.filter(p => pluginEnabled(p)).reduce((all, p) => all.concat(p.actions), []) }
    readonly property var coreActions: [
        {id: "newTab", group: "Tabs", name: "New tab", keys: ["Ctrl+T"]},
        {id: "closeTab", group: "Tabs", name: "Close tab", keys: ["Ctrl+W", "Ctrl+F4"]},
        {id: "reopenTab", group: "Tabs", name: "Reopen closed tab", keys: ["Ctrl+Shift+T"]},
        {id: "nextTab", group: "Tabs", name: "Next tab", keys: ["Ctrl+Tab", "Ctrl+PgDown"]},
        {id: "previousTab", group: "Tabs", name: "Previous tab", keys: ["Ctrl+Shift+Tab", "Ctrl+PgUp"]},
        {id: "tab1", group: "Tabs", name: "Switch to tab 1", keys: ["Alt+1"]},
        {id: "tab2", group: "Tabs", name: "Switch to tab 2", keys: ["Alt+2"]},
        {id: "tab3", group: "Tabs", name: "Switch to tab 3", keys: ["Alt+3"]},
        {id: "tab4", group: "Tabs", name: "Switch to tab 4", keys: ["Alt+4"]},
        {id: "tab5", group: "Tabs", name: "Switch to tab 5", keys: ["Alt+5"]},
        {id: "tab6", group: "Tabs", name: "Switch to tab 6", keys: ["Alt+6"]},
        {id: "tab7", group: "Tabs", name: "Switch to tab 7", keys: ["Alt+7"]},
        {id: "tab8", group: "Tabs", name: "Switch to tab 8", keys: ["Alt+8"]},
        {id: "tabLast", group: "Tabs", name: "Switch to the last tab", keys: ["Alt+9"]},
        {id: "muteTab", group: "Tabs", name: "Mute or unmute tab", keys: ["Ctrl+M"]},
        {id: "tabToOtherPane", group: "Tabs", name: "Move tab to split view", keys: []},
        {id: "tabSearch", group: "Tabs", name: "Search tabs", keys: ["Ctrl+Shift+E"]},
        {id: "newWindow", group: "Windows", name: "New window", keys: ["Ctrl+N"]},
        {id: "newPrivateWindow", group: "Windows", name: "New private window", keys: ["Ctrl+Shift+N"]},
        {id: "fullScreen", group: "Windows", name: "Full screen", keys: ["F11"]},
        {id: "quit", group: "Windows", name: "Quit Kite", keys: ["Ctrl+Q"]},
        {id: "focusAddress", group: "Navigation", name: "Go to the address bar", keys: ["Ctrl+L", "F6", "Alt+D"]},
        {id: "searchWeb", group: "Navigation", name: "Search the web", keys: ["Ctrl+K"]},
        {id: "back", group: "Navigation", name: "Back", keys: ["Alt+Left", "Back"]},
        {id: "forward", group: "Navigation", name: "Forward", keys: ["Alt+Right", "Forward"]},
        {id: "home", group: "Navigation", name: "Home page", keys: ["Alt+Home"]},
        {id: "reload", group: "Navigation", name: "Reload", keys: ["Ctrl+R", "F5"]},
        {id: "reloadBypass", group: "Navigation", name: "Reload, ignoring the cache", keys: ["Ctrl+Shift+R", "Ctrl+F5"]},
        {id: "find", group: "Page", name: "Find in page", keys: ["Ctrl+F"]},
        {id: "findNext", group: "Page", name: "Find next", keys: ["F3", "Ctrl+G"]},
        {id: "findPrevious", group: "Page", name: "Find previous", keys: ["Shift+F3", "Ctrl+Shift+G"]},
        {id: "zoomIn", group: "Page", name: "Zoom in", keys: ["Ctrl++", "Ctrl+="]},
        {id: "zoomOut", group: "Page", name: "Zoom out", keys: ["Ctrl+-"]},
        {id: "zoomReset", group: "Page", name: "Reset zoom", keys: ["Ctrl+0"]},
        {id: "viewSource", group: "Page", name: "View page source", keys: ["Ctrl+U"]},
        {id: "print", group: "Page", name: "Print", keys: ["Ctrl+P"]},
        {id: "savePage", group: "Page", name: "Save page", keys: ["Ctrl+S"]},
        {id: "openFile", group: "Page", name: "Open file", keys: ["Ctrl+O"]},
        {id: "devTools", group: "Page", name: "Developer tools", keys: ["F12", "Ctrl+Shift+I"]},
        {id: "saveSession", group: "Windows", name: "Save current session", keys: []},
        {id: "manageSessions", group: "Windows", name: "Manage sessions", keys: []},
        {id: "commandPalette", group: "Tools", name: "Command palette", keys: ["Ctrl+Shift+P", "Ctrl+Alt+I"]},
        {id: "readerView", group: "Page", name: "Reader view", keys: ["Ctrl+Alt+R"]},
        {id: "bookmarkPage", group: "Bookmarks and history", name: "Bookmark this page", keys: ["Ctrl+D"]},
        {id: "bookmarkAllTabs", group: "Bookmarks and history", name: "Bookmark all tabs", keys: ["Ctrl+Shift+D"]},
        {id: "bookmarksSidebar", group: "Bookmarks and history", name: "Bookmarks sidebar", keys: ["Ctrl+B"]},
        {id: "historySidebar", group: "Bookmarks and history", name: "History sidebar", keys: ["Ctrl+H"]},
        {id: "bookmarksLibrary", group: "Bookmarks and history", name: "Manage bookmarks", keys: ["Ctrl+Shift+O"]},
        {id: "historyLibrary", group: "Bookmarks and history", name: "Show all history", keys: ["Ctrl+Shift+H"]},
        {id: "downloads", group: "Tools", name: "Downloads", keys: ["Ctrl+Shift+Y", "Ctrl+J"]},
        {id: "addons", group: "Tools", name: "Extensions", keys: ["Ctrl+Shift+A"]},
        {id: "settings", group: "Tools", name: "Settings", keys: ["Ctrl+Shift+,", "Ctrl+,"]},
    ]
    // The keys of an action: the user's, or the defaults.
    function shortcutKeys(id, defaults) {
        const o = liveSettings["shortcuts"] || {}
        if (o[id] !== undefined) return o[id]
        if (defaults !== undefined) return defaults
        const a = shortcutActions.find(x => x.id === id)
        return a ? a.keys : []
    }


    // Unloading inactive tabs ("tabs.unloadInactive"): tabs not shown for
    // "tabs.unloadAfterMinutes" are unloaded (WebTab.unload() keeps those
    // playing sound, holding unsent form input, … loaded).
    readonly property bool unloadInactive: !!liveSettings["tabs.unloadInactive"]
    property Timer _unloadTimer: Timer {
        interval: 30000; repeat: true; running: app.unloadInactive
        onTriggered: {
            const limit = Math.max(1, app.setting("tabs.unloadAfterMinutes")) * 60000
            const now = Date.now()
            for (const w of app.windows) for (const id in w.tabItems) {
                const t = w.tabItems[id]
                if (t && now - t.lastAccessed >= limit) t.unload()
            }
        }
    }

    // Settings apply live: web views and windows bind to `liveSettings`
    // (e.g. liveSettings["web.javascript"]) and profile settings are
    // re-applied, so a change in Preferences reaches open tabs and windows
    // right away.
    readonly property var liveKeys: ["web.javascript", "web.autoloadImages", "web.localStorage", "web.pluginsEnabled",
        "web.spatialNavigation", "web.smoothScrolling", "web.dnsPrefetch", "web.printBackgrounds", "appearance.forceDarkMode",
        "appearance.showBookmarksToolbar", "appearance.showStatusBar", "appearance.showMenuBar", "toolbar.items", "extensions.unpinned", "tabs.position", "tabs.hideWhenSingle", "tabs.fillWidth", "tabs.tree", "reader.fontSize", "reader.width", "reader.font", "reader.theme", "shortcuts", "privacy.webrtcPublicOnly", "privacy.containers", "privacy.containerList", "privacy.siteCookieRules", "privacy.antiFingerprinting", "tabs.unloadInactive", "general.homepageMode", "general.newTabPageMode", "startpage.home.file", "startpage.home.copy", "startpage.newtab.file", "startpage.newtab.copy"]
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
            if (["web.doNotTrack", "privacy.globalPrivacyControl", "privacy.blockPings", "privacy.antiFingerprinting", "web.googleSignIn", "web.localStorage"].includes(key)) app.installUserScripts()
            if (key.startsWith("plugins.")) {
                app.pluginActions = app.enabledPluginActions()
                app.installUserScripts()
            }
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
        p.downloadRequested.connect(d => {
            if (p === privateProfile) _privateDownloads.push(d)
            onDownloadRequested(d)
        })
        p.presentNotification.connect(n => onWebNotification(n, p === privateProfile))
    }

    // Called (queued) by the C++ bootstrap once the profiles are hooked up.
    function start() {
        installUserScripts()
        Browser.refreshAdblock()
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
        const notice = Browser.startupNotice()
        // (Long enough to read a file's name, and to notice it after starting up.)
        if (notice && activeWindow) activeWindow.toast(notice, "dialog-information", 30000)
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
            w.treeRestoring = true
            for (const t of opts.tabs) w.restoreTab(t, true)
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

    // ---- sessions kept under a name (SessionMenu.qml, SessionDialogs.qml) ------
    // The regular windows, as for the session restored at startup; answers with
    // an error message, or "".
    function saveNamedSession(name) {
        return Browser.saveNamedSession(name, JSON.stringify(windows.filter(w => !w.privateMode).map(w => w.saveState())))
    }
    // Its windows beside the open ones, or instead of them (`replace`; private
    // windows stay, and the others go to Recently Closed as usual).
    function openNamedSession(name, replace) {
        const saved = JSON.parse(Browser.namedSession(name)).filter(w => w.tabs.length > 0)
        if (!saved.length) return
        const before = windows.filter(w => !w.privateMode)
        for (const w of saved) restoreWindow(w)
        if (replace) for (const w of before) w.close()
    }

    function windowClosed(w) {
        // Closing the last window quits Kite: it is the session to restore,
        // so save it while it is still listed (not as a closed window).
        const last = !quitting && windows.length === 1 && windows[0] === w
        if (last) saveSession()
        windows = windows.filter(x => x !== w)
        Extensions.browserEvent("windowRemoved", JSON.stringify({windowId: w.windowId}))
        if (w.privateMode && !windows.some(x => x.privateMode)) Browser.forgetPrivateDownloads()
        if (!quitting && !last && !w.privateMode) Browser.pushClosedWindow(JSON.stringify(w.saveState()))
        if (activeWindow === w) activeWindow = windows.length ? windows[windows.length - 1] : null
        if (last) quit(true)
        else if (!quitting) saveSession()
    }

    function windowActivated(w) {
        activeWindow = w
        Extensions.browserEvent("windowFocused", JSON.stringify({windowId: w.windowId}))
    }

    // ---- printing (cpp/print.cpp) --------------------------------------------
    // The dialog first, as the page is laid out for the paper chosen there.
    property var _printingTab: null
    property string _printPdf: ""
    // The page as a PDF, with margins. printToPdf takes none and leaves the page
    // edge to edge, into what printers can't reach, and Chromium ignores @page
    // margins for it. So the page gets padding while it is made, which "clone"
    // repeats on each sheet.
    function makePdf(tab, path, pageSize, landscape, margins) {
        const css = "@media print { html { padding: " + margins.map(m => m + "mm").join(" ") + " !important; box-decoration-break: clone !important; -webkit-box-decoration-break: clone !important; box-sizing: border-box !important; } }"
        const add = "(() => { const s = document.createElement('style'); s.id = '__kite_print_margins'; s.textContent = " + JSON.stringify(css) + "; const h = document.head || document.documentElement; h.insertBefore(s, h.firstChild); })()"
        tab.view.runJavaScript(add, 1, () => tab.view.printToPdf(path, pageSize, landscape ? WebEngineView.Landscape : WebEngineView.Portrait))
    }
    // (A page saved without asking for paper: half-inch margins on A4.)
    function savePdf(tab, path) { makePdf(tab, path, WebEngineView.A4, false, [12.7, 12.7, 12.7, 12.7]) }
    function printTab(tab) {
        if (!tab || _printingTab) return
        _printingTab = tab
        Browser.printDialog(tab.title)
    }
    function pdfFinished(tab, path, ok) {
        tab.view.runJavaScript("(() => { const s = document.getElementById('__kite_print_margins'); if (s) s.remove(); })()", 1)
        if (path === _printPdf && _printPdf !== "") {
            _printPdf = ""
            _printingTab = null
            if (ok) { Browser.printSubmit(); return }
            Browser.printCancel()
            tab.window.toast("The page could not be prepared for printing", "dialog-error")
            return
        }
        tab.window.toast(ok ? "Saved " + path : "Could not save " + path, ok ? "document-save" : "dialog-error")
    }
    property Connections _printConn: Connections {
        target: Browser
        function onPrintChosen(json) {
            const t = app._printingTab
            if (!json || !t) {
                app._printingTab = null
                if (json) Browser.printCancel()
                return
            }
            const c = JSON.parse(json)
            app._printPdf = c.path
            app.makePdf(t, c.path, c.pageSize, c.landscape, c.margins)
        }
        function onPrintDone(ok, message) { if (app.activeWindow) app.activeWindow.toast(message, ok ? "document-print" : "dialog-error") }
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
        const o = Extensions.newTabOverride(false)
        return o !== "" ? o : usesFile("newtab") ? startPageUrl("newtab") : setting("general.newTabPage")
    }
    // With only an extension allowed in private windows taking it over.
    function privateNewTabUrl() {
        const o = Extensions.newTabOverride(true)
        return o !== "" ? o : usesFile("newtab") ? startPageUrl("newtab") : setting("general.newTabPage")
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

    // For extension requests. Private windows exist only for extensions
    // allowed there (set by handleExtensionRequest).
    property bool _extSeesPrivate: false
    function _extWindows() { return _extSeesPrivate ? windows : windows.filter(w => !w.privateMode) }
    function findTab(tabId) {
        for (const w of _extWindows()) { const t = w.tabById(tabId); if (t) return t }
        return null
    }
    function findWindow(windowId) {
        return _extWindows().find(w => w.windowId === windowId) || null
    }
    function currentWindowFor(windowId) {
        const asked = windowId !== undefined && windowId >= 0 ? findWindow(windowId) : null
        if (asked) return asked
        const open = _extWindows()
        if (activeWindow && open.includes(activeWindow)) return activeWindow
        return open.length ? open[open.length - 1] : newWindow()
    }

    // (`win`: the window to use, else the active one.)
    function openUrl(url, where, win) {
        const w = win || activeWindow || newWindow({empty: true})
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
        const fromPlugins = pluginScripts()
        for (const p of allProfiles()) {
            // Like Firefox, extensions don't run in private windows unless allowed.
            const wanted = JSON.parse(Extensions.userScripts(p === privateProfile))
            // Logins and form autofill in a page's frames.
            wanted.push({name: "kite:frameForms", sourceCode: Browser.frameFormsScript(), worldId: 1, runsOnSubFrames: true, injectionPoint: 0})
            // navigator.wakeLock (QtWebEngine refuses wake locks itself).
            wanted.push({name: "kite:wakeLock", sourceCode: Browser.wakeLockScript(), worldId: 0, runsOnSubFrames: false, injectionPoint: 0})
            // Third-party frames are refused storage along with cookies, which crashes
            // players embedded from elsewhere. (Not if the user turned storage off.)
            if (setting("web.localStorage"))
                wanted.push({name: "kite:thirdPartyStorage", sourceCode: Browser.thirdPartyStorageScript(), worldId: 0, runsOnSubFrames: true, injectionPoint: 0})
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
            for (const s of fromPlugins) wanted.push(s)
            if (setting("web.googleSignIn"))
                wanted.push({name: "kite:googleSignIn", worldId: 0, runsOnSubFrames: true, injectionPoint: 0,
                             sourceCode: "// ==UserScript==\n// @match https://accounts.google.com/*\n// ==/UserScript==\n" + Browser.googleSignInScript()})
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
    // What frames tell WebTab (it may hold a password): never logged.
    readonly property string frameSecret: Browser.frameSecret()
    function logConsole(extension, prefix, level, message, source, line) {
        if (message.startsWith(frameSecret)) return
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
        _extSeesPrivate = Extensions.runsInPrivate(ext)
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
        case "tabs.discard": { for (const id of [].concat(a.tabId)) { const t = tabOrThrow(id); if (!t.visible) t.unload() } return null }
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
        case "tabs.print": { const t = tabOrThrow(a.tabId); t.window.printTab(t); return null }
        case "tabs.saveAsPDF": tabOrThrow(a.tabId).window.savePdf(); return "saved"
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
            if (d.incognito && !_extSeesPrivate) throw new Error("Extension does not have permission for incognito mode")
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
            if (a.inTab) openUrl(a.url, "tab", currentWindowFor())
            else optionsDialog.open(a.url, a.name)
            return null
        }
        case "action.openPopup": {
            currentWindowFor().openExtensionPopup(ext)
            return null
        }
        case "sidebar.open": case "sidebar.toggle": case "sidebar.close": case "sidebar.isOpen": {
            const w = currentWindowFor()
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
            if (a.tabId !== undefined) tabOrThrow(a.tabId).load(url); else openUrl(url, where, currentWindowFor())
            return null
        }
        case "sessions.restore": {
            const t = Browser.popClosedTab(0)
            const tab = JSON.parse(t)
            if (tab && tab.url) return currentWindowFor().restoreTab(tab).extInfo()
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

    // Where a page's next file picker opens: the last one's folder until Kite closes
    // ("browsing.rememberFileFolder"), else home. Private windows don't leave theirs behind.
    property string _lastFileFolder: ""
    function fileFolder() {
        const home = StandardPaths.writableLocation(StandardPaths.HomeLocation).toString()
        return setting("browsing.rememberFileFolder") && _lastFileFolder !== "" && Browser.folderExists(_lastFileFolder) ? _lastFileFolder : home
    }
    function rememberFileFolder(folder, inPrivate) { if (!inPrivate) _lastFileFolder = folder }

    // ---- downloads --------------------------------------------------------------
    property var activeDownloads: ({})
    property var pendingDownloadNames: ({})

    // Downloads from private windows, until startDownload takes them.
    property var _privateDownloads: []
    // "Save … As" in a page's menu asks where to, whatever the setting says. Qt doesn't
    // say which download came from it, but it comes soon after. (If something else
    // came first, it's only a question too many.)
    property double askUntil: 0
    function askBeforeNextDownload() { askUntil = Date.now() + 60000 }
    // For plugins. Those that say they'll tell the user themselves put the download's
    // id in claimedDownloads when it begins.
    signal downloadBegan(int id, var download, bool inPrivate)
    signal downloadProgress(int id, var download)
    signal downloadPausedChanged(int id, var download)
    signal downloadEnded(int id, var download, string state)
    property var claimedDownloads: ({})
    function fileUrl(path) { return "file://" + encodeURI(path).replace(/[?#]/g, encodeURIComponent) }
    // `nameless`: from a private window, whose file names stay out of the desktop's
    // notification history. (What the buttons open isn't told to it.)
    function notifyDownloadComplete(path, nameless) {
        const open = () => Qt.openUrlExternally(fileUrl(path))
        showNotification("Download complete", nameless ? "" : path.substring(path.lastIndexOf("/") + 1), "", open, null, [
            {label: "Open", run: open},
            {label: "Open Folder", run: () => Qt.openUrlExternally(fileUrl(path.substring(0, path.lastIndexOf("/")) || "/"))},
        ])
    }
    function onDownloadRequested(download) {
        const url = download.url.toString()
        // Firefox add-ons: install instead of saving.
        if (url.endsWith(".xpi") || download.mimeType === "application/x-xpinstall") {
            download.cancel()
            Extensions.installFromUrl(url)
            return
        }
        // Dangerous-site protection: known malware isn't downloaded.
        const threat = Browser.safeBrowsingCheck(url)
        if (threat) {
            download.cancel()
            if (activeWindow) activeWindow.toast("Download blocked: " + Browser.hostOf(url) + " is listed for " + (threat === "phishing" ? "phishing" : threat === "unwanted" ? "unwanted software" : "malware"), "dialog-warning")
            return
        }
        const asked = Date.now() < askUntil
        askUntil = 0
        const dir = setting("downloads.directory")
        const name = pendingDownloadNames[url] || download.suggestedFileName || download.downloadFileName
        delete pendingDownloadNames[url]
        const path = Browser.uniqueDownloadPath(dir, name)
        download.downloadDirectory = path.substring(0, path.lastIndexOf("/"))
        download.downloadFileName = path.substring(path.lastIndexOf("/") + 1)
        if (asked || setting("downloads.askWhereToSave")) {
            saveDialog.ask(download)
            return
        }
        startDownload(download)
    }

    function startDownload(download) {
        const inPrivate = _privateDownloads.includes(download)
        _privateDownloads = _privateDownloads.filter(d => d !== download)
        const id = Browser.downloadUpdate(JSON.stringify({id: 0, private: inPrivate, url: download.url.toString(),
            path: download.downloadDirectory + "/" + download.downloadFileName, mime: download.mimeType,
            total: download.totalBytes, received: 0, state: "in_progress"}))
        activeDownloads[id] = download
        download.accept()
        downloadBegan(id, download, inPrivate)
        download.isPausedChanged.connect(() => downloadPausedChanged(id, download))
        let last = 0
        download.receivedBytesChanged.connect(() => {
            const now = Date.now()
            if (now - last < 500) return
            last = now
            Browser.downloadUpdate(JSON.stringify({id: id, received: download.receivedBytes, total: download.totalBytes}))
            downloadProgress(id, download)
        })
        let ended = false
        const end = () => {
            if (ended) return
            ended = true
            const st = download.state === WebEngineDownloadRequest.DownloadCompleted ? "complete"
                     : download.state === WebEngineDownloadRequest.DownloadCancelled ? "cancelled" : "interrupted"
            Browser.downloadUpdate(JSON.stringify({id: id, received: download.receivedBytes, total: download.totalBytes, state: st}))
            delete activeDownloads[id]
            downloadEnded(id, download, st)
            if (st === "complete" && !claimedDownloads[id] && (!inPrivate || setting("privacy.privateNotifications"))) notifyDownloadComplete(download.downloadDirectory + "/" + download.downloadFileName, inPrivate)
            delete claimedDownloads[id]
        }
        download.isFinishedChanged.connect(() => { if (download.isFinished) end() })
        // (Interrupted, as by a lost connection, it never counts as finished.)
        download.stateChanged.connect(() => { if (download.state === WebEngineDownloadRequest.DownloadInterrupted) end() })
        if (activeWindow) activeWindow.downloadStarted()
    }

    // ---- notifications ------------------------------------------------------------
    function onWebNotification(n, inPrivate) {
        // (Pages aren't let ask for this either, see WebTab. One that was allowed before
        // the setting changed still gets here.)
        if (inPrivate && !setting("privacy.privateNotifications")) { n.close(); return }
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
    property Component notificationActionComponent: Component { NotificationAction {} }
    // `buttons`: a list of {label, run}.
    function showNotification(title, text, iconUrl, onClick, onClose, buttons) {
        const n = notificationComponent.createObject(app, {title: title, text: text})
        if (onClick) {
            const action = notificationActionComponent.createObject(n, {label: "Open"})
            action.activated.connect(onClick)
            n.defaultAction = action
        }
        for (const b of buttons || []) {
            const action = notificationActionComponent.createObject(n, {label: b.label})
            action.activated.connect(b.run)
            n.actions.push(action)
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
    function showPluginSettings(plugin) { showPreferences("extensions").openPluginSettings(plugin) }
    function showAddons(page) { return showSingleton("addons", addonsComponent, {page: page || "installed"}) }

}
