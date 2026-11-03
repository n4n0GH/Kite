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

    function configureProfile(p) {
        const ua = setting("web.userAgent")
        if (ua) p.httpUserAgent = ua
        p.downloadPath = setting("downloads.directory")
        p.spellCheckEnabled = setting("web.spellcheck")
        p.spellCheckLanguages = setting("web.spellcheckLanguages")
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
            const first = urls.length ? urls : [startup === "blank" ? "about:blank" : setting("general.homepage")]
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
            w.setCurrentIndex(Math.min(opts.current || 0, w.count - 1))
        } else if (urls.length === 0 && !opts.empty) {
            w.openTab(newTabUrl(), {background: false})
        }
        for (const u of urls) w.openTab(u, {background: false})
        activeWindow = w
        return w
    }

    function restoreWindow(saved) {
        return newWindow({tabs: saved.tabs, current: saved.current, geometry: saved.geometry, maximized: saved.maximized})
    }

    function windowClosed(w) {
        windows = windows.filter(x => x !== w)
        Extensions.browserEvent("windowRemoved", JSON.stringify({windowId: w.windowId}))
        if (!quitting && !w.privateMode) Browser.pushClosedWindow(JSON.stringify(w.saveState()))
        if (activeWindow === w) activeWindow = windows.length ? windows[windows.length - 1] : null
        if (windows.length === 0 && !quitting) quit()
        else saveSession()
    }

    function windowActivated(w) {
        activeWindow = w
        Extensions.browserEvent("windowFocused", JSON.stringify({windowId: w.windowId}))
    }

    function newTabUrl() {
        const o = Extensions.newTabOverride()
        return o !== "" ? o : setting("general.newTabPage")
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

    function quit() {
        if (quitting) return
        quitting = true
        saveSession()
        Browser.setRunning(false)
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
    function installUserScripts() {
        for (const p of [profile, privateProfile]) {
            // Like Firefox, extensions don't run in private windows by default.
            const scripts = JSON.parse(Extensions.userScripts(p === profile))
            p.userScripts.clear()
            for (const s of scripts) {
                const ws = WebEngine.script()
                ws.name = s.name
                ws.sourceCode = s.sourceCode
                ws.worldId = s.worldId
                ws.runsOnSubFrames = s.runsOnSubFrames
                ws.injectionPoint = s.injectionPoint === 0 ? WebEngineScript.DocumentCreation
                                  : s.injectionPoint === 1 ? WebEngineScript.DocumentReady : WebEngineScript.Deferred
                p.userScripts.insert(ws)
            }
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
