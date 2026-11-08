import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite

// A browser wt (Falkon's WebTab + WebView + WebPage).
Item {
    id: wt

    required property int tabId
    // Container tab id ("" = none); fixed for the tab's life (its profile).
    required property string container
    // Split view pane: 0 = left, 1 = right (BrowserWindow).
    required property int pane
    property int tabIndex: 0
    property var window
    property var app
    property alias view: web
    property string pendingUrl: ""
    // A web page load held until app.networkReady.
    property string heldUrl: ""
    property int openerTabId: -1
    property bool pinned: false
    property string hoveredLink: ""
    // Where the pointer is over the page (view coordinates), for the link
    // preview to keep out of its way.
    readonly property point pointer: pointerWatch.point.position
    readonly property bool pointerInside: pointerWatch.hovered
    // (Reader view: the page the article is from, for everything but the view
    // itself: the address bar, bookmarks, the saved session, extensions.)
    property string url: heldUrl !== "" ? heldUrl : pendingUrl !== "" ? pendingUrl : sourceOf(web.url.toString())
    // Tabs that haven't loaded yet (restored ones load when first activated)
    // show the title and icon they had before.
    property string title: web.title !== "" ? web.title : ((pendingUrl !== "" || heldUrl !== "") && _restoredTitle !== "" ? _restoredTitle : (url === "" || url === "about:blank" ? "New Tab" : url))
    property string _restoredTitle: ""
    property string _restoredIcon: ""
    function restoreLook(savedTitle, savedUrl) {
        _restoredTitle = savedTitle || ""
        _restoredIcon = Browser.cachedFavicon(savedUrl || "")
        if (_restoredIcon !== "") window.setTabProperty(tabId, "icon", _restoredIcon)
    }
    property bool loading: web.loading
    property string status: web.loading ? "loading" : "complete"
    property var lastAccessed: Date.now()
    property string lastHost: ""
    property bool crashed: false
    property var pendingPermissions: []
    property var certError: null
    property string certFingerprint: ""
    // Only a tab being shown holds a certificate warning open: while one waits for an
    // answer, Chromium keeps every other tab's requests to that site waiting too, so a
    // warning in a tab behind would leave the shown one hanging. A tab that isn't shown
    // says no for now, and asks again when it is.
    property string certRetryUrl: ""
    readonly property bool shownInPane: window && window.paneTab.includes(tabId)
    onShownInPaneChanged: {
        // (Next to the active pane's tab, it's on view without being activated.)
        if (shownInPane) wake()
        if (!shownInPane && certError) {
            certRetryUrl = certError.url.toString()
            certError.rejectCertificate()
            certError = null
        } else if (shownInPane && certRetryUrl !== "") {
            const u = certRetryUrl
            certRetryUrl = ""
            load(u)
        }
    }
    // Password manager
    property var logins: []
    property bool loginForm: false
    property var savePrompt: null

    // Form autofill
    property var addressPrompt: null
    function answerAddressPrompt(choice) {
        const p = addressPrompt
        addressPrompt = null
        if (p) FormFill.answer(p.request, choice)
    }
    function refreshLogins() {
        const list = loginForm ? JSON.parse(Passwords.logins(web.url.toString())) : []
        const top = originOf(web.url.toString())
        for (const id in loginFrames) {
            const f = loginFrames[id]
            if (!f.isValid) continue
            const url = f.url.toString()
            // (A frame of another site is named, not to be mistaken for this one.)
            for (const l of JSON.parse(Passwords.logins(url))) list.push(Object.assign({}, l, {frameId: id, frameHost: originOf(url) === top ? "" : Browser.hostOf(url)}))
        }
        logins = list
    }
    // (`frameId`: for a login form in that frame.)
    // Filling as a page loads, without a click: only over https. Logins are
    // stored by host alone (Falkon's format), so one saved on https://site
    // would otherwise go into http://site too, where anyone on the network
    // can add a script that reads it. On http a login is filled when picked.
    function autofillHere() {
        return Passwords.autofillEnabled() && web.url.toString().startsWith("https://")
    }

    function fillLogin(id, frameId) {
        if (frameId === undefined) {
            const data = Passwords.fillData(web.url.toString(), id)
            if (data !== "null") bridge.fillLogin(data)
            return
        }
        const f = loginFrames[frameId]
        if (!f || !f.isValid) return
        const data = Passwords.fillData(f.url.toString(), id)
        if (data !== "null") frameDown(f, "fillLogin", data)
    }

    // ---- forms in frames (webext/js/frame_transport.js) ----
    // A frame's address comes from Qt, never from the frame or the page. Logins
    // are filled by themselves only in frames of the page's own site: one of
    // another site could be hidden, or placed where it fools the user, so that
    // takes picking it from the key menu.
    property var loginFrames: ({})      // frame id → frame with a login form
    property bool frameLoginForm: false
    property var _knownFrames: ({})     // frame id → frame
    function originOf(u) { const m = /^(https?:\/\/[^/?#]+)/i.exec(u); return m ? m[1].toLowerCase() : "" }
    function allFrames(frame, out) {
        for (const c of frame.children) { out.push(c); allFrames(c, out) }
        return out
    }
    // Find the frame by asking each for the id its script chose.
    function withFrame(id, then) {
        const known = _knownFrames[id]
        if (known && known.isValid) { then(known); return }
        for (const f of allFrames(web.mainFrame, [])) {
            f.runJavaScript("globalThis.__kiteFrameId", 1, r => {
                if (r !== id || _knownFrames[id]) return
                _knownFrames[id] = f
                then(f)
            })
        }
    }
    // Run code in the isolated world of the page and all its frames, for plugins;
    // `each(frame, result)` with what each answers. (Qt's frames don't run it
    // without a callback.)
    function runInFrames(code, each) {
        // (A tab not loaded has no page to run it in.)
        if (!web.mainFrame || unloaded) return
        for (const f of [web.mainFrame].concat(allFrames(web.mainFrame, []))) {
            const frame = f
            frame.runJavaScript(code, 1, r => { if (each) each(frame, r) })
        }
    }
    function runInMainFrame(code) { if (web.mainFrame) web.mainFrame.runJavaScript(code, 1, () => {}) }
    function frameDown(frame, name, arg) {
        const call = "globalThis.__kiteFrameDown && __kiteFrameDown(" + JSON.stringify(name) + ", " + JSON.stringify(arg) + ", " + JSON.stringify(originOf(frame.url.toString())) + ")"
        // (Qt's frames don't run it without a callback.)
        frame.runJavaScript(call, 1, () => {})
    }
    // Where a frame is in the page: its element there, if the top frame can
    // tell which that is. Nested ones and doubtful cases go by the pointer.
    function frameOffset(frame, then) {
        const list = "JSON.stringify(Array.from(document.querySelectorAll('iframe, frame')).map(e => { const r = e.getBoundingClientRect(); return {src: e.src, x: r.left + e.clientLeft, y: r.top + e.clientTop, w: e.clientWidth, h: e.clientHeight} }))"
        web.runJavaScript(list, 1, json => {
            const all = JSON.parse(json || "[]")
            let hits = all.filter(e => e.src === frame.url.toString())
            if (hits.length !== 1) hits = all.filter(e => e.w === frame.size.width && e.h === frame.size.height)
            then(hits.length === 1 ? hits[0] : null)
        })
    }
    // ---- the content blocker (Main.prepareAdblock) ----
    // (`tick`: for a binding to ask again.)
    function canBlockElements(tick) {
        return /^https?:/i.test(url) && app.setting("adblock.enabled") && Browser.contentBlocker(window.privateMode) === "" && !Browser.adblockAllowed(Browser.hostOf(url))
    }
    function blockElement(x, y) { elementPicker.start(x, y) }
    // A frame's classes and ids, and what generic rules hide among them.
    function adblockFound(json) {
        const f = JSON.parse(json).f
        const selectors = Browser.adblockGeneric(json)
        if (selectors === "[]") return
        runInFrames("globalThis.__kiteAbFill && __kiteAbFill(" + JSON.stringify(f) + ", " + selectors + ")")
    }

    function frameMessage(text) {
        const msg = JSON.parse(text)
        withFrame(msg.f, frame => {
            if (!frame.isValid) return
            const url = frame.url.toString()
            const arg = msg.a[0]
            switch (msg.m) {
            case "loginFormFound": {
                const all = Object.assign({}, loginFrames)
                all[msg.f] = frame
                loginFrames = all
                frameLoginForm = true
                refreshLogins()
                const own = logins.find(l => l.frameId === msg.f && !l.frameHost)
                if (arg && own && originOf(url) === originOf(web.url.toString()) && autofillHere()) fillLogin(own.id, msg.f)
                break
            }
            case "formSubmitted": {
                const r = JSON.parse(Passwords.formSubmitted(url, wt.window.privateMode, arg))
                if (r) savePrompt = r
                break
            }
            case "autofillSubmit": {
                const r = JSON.parse(FormFill.formSubmitted(url, wt.window.privateMode, arg))
                if (r) addressPrompt = r
                break
            }
            case "autofillFocus":
                frameOffset(frame, at => {
                    const f = JSON.parse(arg)
                    f.url = url
                    if (at) { f.x += at.x; f.y += at.y }
                    else {
                        const zoom = web.zoomFactor || 1
                        f.x = pointer.x / zoom; f.y = pointer.y / zoom; f.w = 0; f.h = 0
                    }
                    frameBridge.frame = frame
                    autofillPopup.bridge = frameBridge
                    autofillPopup.show(f)
                })
                break
            case "autofillBlur": autofillPopup.close(); break
            case "autofillKey": autofillPopup.key(arg); break
            case "formDirty": formDirty = arg; break
            }
        })
    }
    // What the suggestions call on the top frame's bridge, for a frame.
    QtObject {
        id: frameBridge
        property var frame
        function autofillMenu(open) { if (frame && frame.isValid) wt.frameDown(frame, "autofillMenu", open) }
        function fillAddress(json) { if (frame && frame.isValid) wt.frameDown(frame, "fillAddress", json) }
        function fillPassword(password) { if (frame && frame.isValid) wt.frameDown(frame, "fillPassword", password) }
    }
    function answerSavePrompt(choice) {
        const p = savePrompt
        savePrompt = null
        if (!p) return
        const finish = () => { const err = Passwords.answer(p.request, choice); if (err) wt.window.toast(err, "dialog-error"); else refreshLogins() }
        if (choice === "save" && Passwords.locked) wt.app.unlockPasswords(ok => { if (ok) finish(); else Passwords.answer(p.request, "dismiss") })
        else finish()
    }
    readonly property bool isExtensionPage: url.startsWith("moz-extension:")
    readonly property bool isSecure: url.startsWith("https:") || url.startsWith("kite:") || isExtensionPage
    signal activated()
    Connections {
        target: wt.app
        function onNetworkReadyChanged() { if (wt.heldUrl !== "" && wt.app.networkReady) wt.load(wt.heldUrl) }
    }

    function load(u) {
        redirectHops = 0
        pendingUrl = ""
        if (!app.networkReady && /^(https?|wss?):/i.test(u)) { heldUrl = u; return }
        heldUrl = ""
        crashed = false
        certError = null
        web.url = u
    }

    // ---- addresses for other programs ----
    // A link to smb://server/share and the like could make the file manager
    // connect wherever the page wants, sending the user's login, so a click
    // on one asks first. (Pages doing it without a click get nowhere.)
    property string externalPrompt: ""

    // ---- reader view (kite_core::reader) ----
    // The article is extracted in a world of its own, shown at a kite:reader
    // address (one step in the tab's history: Back returns to the page).
    readonly property int readerWorld: 250
    readonly property bool isReader: web.url.toString().startsWith("kite:reader?")
    // Whether the page looks like an article (the address bar offers it).
    property bool readerAvailable: false
    function sourceOf(u) { return u.startsWith("kite:reader?") ? (Browser.readerSource(u) || u) : u }
    function canRead() { return isReader || /^https?:/i.test(url) }
    function showSiteIcon() { window.setTabProperty(tabId, "icon", Browser.cachedFavicon(sourceOf(web.url.toString()))) }
    function toggleReader() {
        if (isReader) {
            if (web.canGoBack) web.goBack()
            else load(url)
            return
        }
        if (!canRead()) return
        const source = url
        web.runJavaScript(Browser.readerScript(), readerWorld, json => {
            if (wt.url !== source) return   // gone elsewhere meanwhile
            const t = Kirigami.Theme
            const reader = json ? Browser.readerOpen(source, json, t.backgroundColor.toString(), t.textColor.toString(), t.linkColor.toString()) : ""
            if (reader === "") wt.window.toast("Kite found no article on this page", "dialog-information")
            else web.url = reader
        })
    }
    function probeReader() {
        if (!visible || isReader || discarded || !/^https?:/i.test(web.url.toString())) return
        const page = web.url.toString()
        web.runJavaScript(Browser.readerProbeScript(), readerWorld, ok => { if (web.url.toString() === page) wt.readerAvailable = ok === true })
    }
    // Twice: pages often fill in only after loading.
    Timer { id: readerProbe; interval: 300; onTriggered: { wt.probeReader(); readerProbeLate.restart() } }
    Timer { id: readerProbeLate; interval: 2500; onTriggered: if (!wt.readerAvailable) wt.probeReader() }
    // The settings reach open reader pages at once.
    readonly property string readerLook: ["reader.fontSize", "reader.width", "reader.font", "reader.theme"].map(k => app.liveSettings[k]).join("|")
    onReaderLookChanged: if (isReader) web.runJavaScript(Browser.readerApplyScript(), readerWorld)

    // ---- picture-in-picture (Main.qml startPip) ----
    // The page has a video to offer (plugins/pip/), and the page is in the
    // picture-in-picture window.
    property bool hasVideo: false
    readonly property bool inPip: app.pipTab === wt
    // Move the page into `item` (the picture-in-picture window), or back
    // (null).
    function moveViewTo(item) { web.parent = item ? item : viewHolder }

    // ---- unloading inactive tabs ("tabs.unloadInactive") ----
    // An unloaded tab keeps its place, title, icon and history; Chromium
    // drops the page itself and reloads it when the tab is shown again.
    readonly property bool discarded: web.lifecycleState === WebEngineView.LifecycleState.Discarded
    readonly property bool unloaded: discarded || pendingUrl !== ""
    onUnloadedChanged: window.setTabProperty(tabId, "unloaded", unloaded)
    // An unloaded view has no page to take its icon from: it leaves Qt's
    // favicon provider (which would crash reading it) and the tab shows the
    // cached icon until the page is back.
    onDiscardedChanged: {
        if (!discarded) return
        Browser.faviconTrack(web.objectName, false)
        faviconOut = true
        const c = Browser.cachedFavicon(url)
        if (c !== "") window.setTabProperty(tabId, "icon", c)
    }
    // Out of Qt's favicon provider for good, once unloaded: the page it gets when it
    // wakes up has no favicon helper (Qt only makes one with the view), and reading
    // its icon crashes. It doesn't report new icons either, so after each load its
    // tab gets the page's icon by its address.
    property bool faviconOut: false
    function iconFromPage() {
        const page = web.url.toString()
        web.runJavaScript("(document.querySelector('link[rel~=\"icon\"]') || {}).href || location.origin + '/favicon.ico'", 1, href => {
            if (web.url.toString() !== page || isReader) return
            window.setTabProperty(tabId, "icon", /^https?:/.test(page) && /^(https?:|data:image\/)/.test(String(href)) ? href : "")
        })
    }
    // Text typed into the page that wasn't submitted (it would be lost).
    property bool formDirty: false
    // Shown tabs (the current one, both panes in split view) stay loaded.
    // (Whether a page looks like an article matters once it is seen.)
    onVisibleChanged: {
        if (!visible) lastAccessed = Date.now()
        else if (!readerAvailable) readerProbe.restart()
    }
    // Why the tab must stay loaded ("" when it can be unloaded).
    function keepLoadedReason() {
        if (visible || inPip) return "shown"
        if (pendingUrl !== "" || heldUrl !== "" || discarded) return "not loaded"
        if (web.recentlyAudible && !web.audioMuted) return "playing sound"
        if (wakeLockHeld) return "keeping the screen on"
        if (web.loading) return "loading"
        if (formDirty) return "unsent form input"
        if (devTools.active) return "developer tools open"
        if (pinned && wt.app.setting("tabs.unloadKeepPinned")) return "pinned"
        if (url === "" || url.startsWith("kite:") || url === "about:blank" || isReader) return "browser page"
        return ""
    }
    function unload() {
        if (keepLoadedReason() !== "") return false
        web.lifecycleState = WebEngineView.LifecycleState.Discarded
        return discarded
    }

    onActivated: {
        lastAccessed = Date.now()
        wake()
        // (A page's link has no address until its page starts arriving. The
        // bar doesn't show the one asked for before that: the opener can still
        // write into the empty tab, under an address that isn't its own.)
        if ((url === "" && !openedByPage) || url === app.newTabUrl()) focusLocationBar()
        else focusPage()
    }
    property bool openedByPage: false
    // Redirects in a row (extensions could keep sending a page back and forth).
    property int redirectHops: 0
    // Load what it's waiting to load, now that it's being looked at.
    function wake() {
        if (pendingUrl !== "") { const u = pendingUrl; load(u) }
        web.lifecycleState = WebEngineView.LifecycleState.Active
    }

    // A page that finished loading in the shown tab takes the keyboard (so
    // arrow keys and Space scroll it right away), unless something else is
    // being typed into: the address bar, the find bar, a field in a dialog.
    // Clicked buttons (reload, bookmarks, the toolbar) don't keep it.
    function focusPageAfterLoad() {
        if (window.currentTab !== wt || !window.active) return
        // Already focused (e.g. after Enter in the address bar): only bring
        // the input method in line with the loaded page.
        // (Typing in the console while the page reloads.)
        if (devToolsFocused) return
        if (web.activeFocus) { focusPage(); return }
        if (url === "" || url === app.newTabUrl() || app.isStartPage(url)) return
        const f = window.activeFocusItem
        if (f && (f.cursorPosition !== undefined || f.editable === true)) return
        focusPage()
    }

    // Give the page the keyboard. With an input method (IBus, Fcitx), Qt
    // doesn't always tell it that the text field it was serving (e.g. the
    // address bar) lost focus: it keeps turning keys into typed text, which a
    // page without a focused field drops, so key bindings (Vimium's j/k)
    // never see them. Asking Qt to re-check, once the focus has moved,
    // switches the input method off for the page (as a click or Tab would).
    function focusPage() {
        pageFocusAsked = true
        web.forceActiveFocus()
        pageFocusAsked = false
        Qt.callLater(() => { if (web.activeFocus) { Qt.inputMethod.reset(); Qt.inputMethod.update(Qt.ImQueryAll) } })
    }

    function activate() { window.setCurrentIndex(window.indexOfTab(tabId)); window.raise(); window.requestActivate() }
    function close() { window.closeTabById(tabId) }
    function focusLocationBar() {
        Qt.callLater(() => { if (window.currentTab === wt) window.focusLocationBarNow() })
        window.keepFocusOnLocationBar(wt)
    }

    function setPinned(p) {
        // Pinned tabs aren't in groups (leaving one can move the tab).
        if (p) { window.removeFromGroup(tabId, true); window.detachFromTree(tabId) }
        const i = window.indexOfTab(tabId)
        pinned = p
        window.setTabProperty(tabId, "pinned", p)
        // pinned tabs live at the start of the strip
        let target = 0
        for (let j = 0; j < window.count; ++j) if (j !== i && window.tabModel_get(j).pinned) target++
        if (!p) target = Math.max(0, target)
        if (target !== i) window.moveTab(i, Math.min(target, window.count - 1))
        sendUpdate({pinned: p})
    }

    // Per-site zoom and dark mode (Site Settings) for the current host.
    property int siteDarkMode: 0
    function applySiteSettings() {
        const s = JSON.parse(Browser.siteSettings(lastHost))
        siteDarkMode = s.dark_mode || 0
        web.zoomFactor = s.zoom > 0 ? s.zoom / 100 : app.setting("appearance.defaultZoom") / 100
    }
    Connections {
        target: wt.app
        function onSiteSettingsEdited(host) { if (host === wt.lastHost) wt.applySiteSettings() }
    }

    function zoomStep(dir) {
        const steps = [0.3, 0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0]
        let i = steps.findIndex(s => s >= web.zoomFactor - 0.001)
        if (i < 0) i = steps.length - 1
        setZoom(steps[Math.max(0, Math.min(steps.length - 1, i + dir))])
    }
    function setZoom(z) {
        web.zoomFactor = z
        // (Private windows don't store it: it stays with this tab and page.)
        const host = window.privateMode ? "" : Browser.hostOf(url)
        if (host) {
            Browser.setSiteSetting(host, "zoom", Math.round(z * 100))
            // Other tabs of the site follow; the address bar updates.
            app.siteSettingsEdited(host)
        }
    }

    function saveState() {
        return Object.assign({url: url, title: title, pinned: pinned, zoom: web.zoomFactor, history: [], history_index: 0, container: container, pane: pane, group: window.groupOf(tabId)}, window.treeState(tabId))
    }

    function extInfo() {
        return {
            id: tabId, index: window.indexOfTab(tabId), windowId: window.windowId, active: window.currentTab === wt,
            highlighted: window.currentTab === wt, pinned: pinned, url: url, title: title,
            favIconUrl: web.icon.toString().startsWith("image://favicon/") ? web.icon.toString().substring(16) : "",
            status: status, incognito: window.privateMode, audible: web.recentlyAudible, discarded: unloaded,
            mutedInfo: {muted: web.audioMuted}, width: web.width, height: web.height, lastAccessed: lastAccessed,
            openerTabId: openerTabId >= 0 ? openerTabId : undefined, hidden: false, isArticle: readerAvailable, isInReaderMode: isReader,
            cookieStoreId: window.privateMode ? "firefox-private" : container ? "firefox-container-" + container : "firefox-default",
        }
    }

    function sendUpdate(changes) {
        Extensions.browserEvent("tabUpdated", JSON.stringify({tab: extInfo(), changes: changes}))
        if (window.currentTab === wt) window.refreshToolbar()
    }

    function navEvent(ev, extra) {
        Extensions.browserEvent("navigation", JSON.stringify(Object.assign({event: ev, tabId: tabId, url: url, frameId: 0}, extra || {})))
    }

    // ---- developer tools: docked beside the page, or in a window of their own ----
    // One view all along, moved between those, so what's open in it stays.
    property string devToolsPlace: "right"
    property real devToolsWidth: 0
    property real devToolsHeight: 0
    readonly property string devToolsDocked: devTools.active && devToolsPlace !== "window" && !window.contentFullScreen ? devToolsPlace : ""
    // Room for the page and for the tools, however small the window is.
    readonly property real devToolsSpace: {
        const whole = devToolsDocked === "bottom" ? tabBody.height : tabBody.width
        const wanted = devToolsDocked === "bottom" ? devToolsHeight : devToolsWidth
        const least = Kirigami.Units.gridUnit * 12
        return Math.max(Math.min(least, whole / 2), Math.min(wanted, whole - least))
    }
    readonly property bool devToolsFocused: devTools.item !== null && devTools.item.view.activeFocus
    // A page being loaded takes the keyboard by itself, in QtWebEngine, as its view gets
    // replaced, however the load came about and before anything says it started. Typing
    // in the tools while it reloads goes on, so they get it back: when they've only just
    // lost it to the page, without a click there, or Kite asking.
    property real devToolsLostFocusAt: 0
    property bool pageFocusAsked: false
    function devToolsFocusTaken() {
        const now = Date.now()
        return devTools.item !== null && !pageFocusAsked && now - devToolsLostFocusAt < 100 && now - window.lastPointerPress > 500
    }
    function showDevTools() {
        if (!devTools.active) {
            devToolsPlace = app.setting("devtools.place")
            devToolsWidth = app.setting("devtools.width")
            devToolsHeight = app.setting("devtools.height")
            devTools.active = true
        }
        devTools.item.present()
    }
    function closeDevTools() {
        const focused = devToolsFocused
        devTools.active = false
        if (focused) focusPage()
    }
    function toggleDevTools() {
        if (devTools.active) closeDevTools()
        else showDevTools()
    }
    function setDevToolsPlace(place) {
        devToolsPlace = place
        Browser.setSetting("devtools.place", JSON.stringify(place))
        devTools.item.settle()
        devTools.item.present()
    }
    // (What it ended up as, not how far past the limit it was dragged.)
    function rememberDevToolsSize() {
        if (devToolsDocked === "right") {
            devToolsWidth = devToolsSpace
            Browser.setSetting("devtools.width", JSON.stringify(Math.round(devToolsWidth)))
        } else {
            devToolsHeight = devToolsSpace
            Browser.setSetting("devtools.height", JSON.stringify(Math.round(devToolsHeight)))
        }
    }
    // The element the context menu was opened on. (Developer tools first: until
    // they're attached to the page, there's nothing to select it in.)
    function inspectElement() {
        showDevTools()
        web.triggerWebAction(WebEngineView.InspectElement)
    }

    Component.onCompleted: window.registerTab(wt)
    // Its row is gone: it's no longer a tab, and its view goes once this turn is over.
    function retire() { leave(); destroy() }
    property bool retired: false
    function leave() {
        if (retired) return
        retired = true
        if (app.pipTab === wt) app.stopPip(false)
        setWakeLock(false)
        stopMidi()
        window.unregisterTab(wt)
    }
    Component.onDestruction: leave()

    onUrlChanged: window.setTabProperty(tabId, "url", url)
    onTitleChanged: window.setTabProperty(tabId, "title", title)
    onLoadingChanged: window.setTabProperty(tabId, "loading", loading)

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Site permission requests (geolocation, camera, notifications, ...)
        Repeater {
            model: wt.pendingPermissions
            delegate: Kirigami.InlineMessage {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                visible: true
                type: Kirigami.MessageType.Information
                icon.name: "dialog-question"
                text: "<b>" + modelData.origin + "</b> wants to " + wt.permissionText(modelData.permission.permissionType)
                actions: [
                    Kirigami.Action { text: "Allow"; icon.name: "dialog-ok-apply"; onTriggered: { modelData.permission.grant(); wt.dropPermission(index) } },
                    Kirigami.Action { text: "Block"; icon.name: "dialog-cancel"; onTriggered: { modelData.permission.deny(); wt.dropPermission(index) } }
                ]
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: wt.externalPrompt !== ""
            type: Kirigami.MessageType.Warning
            icon.name: "document-open-remote"
            text: "This page wants to open <b>" + wt.externalPrompt.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</b> in another app."
            actions: [
                Kirigami.Action { text: "Open"; icon.name: "document-open"; onTriggered: { const u = wt.externalPrompt; wt.externalPrompt = ""; wt.window.openExternal(u) } },
                Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: wt.externalPrompt = "" }
            ]
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: wt.addressPrompt !== null
            type: Kirigami.MessageType.Information
            icon.name: "user-identity"
            text: !wt.addressPrompt ? "" : (wt.addressPrompt.action === "update" ? "Update the saved address <b>" : "Save the address <b>") + wt.addressPrompt.label.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</b> for filling in forms?"
            actions: [
                Kirigami.Action { text: wt.addressPrompt && wt.addressPrompt.action === "update" ? "Update" : "Save"; icon.name: "document-save"; onTriggered: wt.answerAddressPrompt("save") },
                Kirigami.Action { text: "Not Now"; onTriggered: wt.answerAddressPrompt("dismiss") }
            ]
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: wt.savePrompt !== null
            type: Kirigami.MessageType.Information
            icon.name: "dialog-password"
            text: !wt.savePrompt ? "" : (wt.savePrompt.action === "update"
                ? "Update the saved password for <b>" + wt.savePrompt.username + "</b> on " + wt.savePrompt.host + "?"
                : "Save the login" + (wt.savePrompt.username ? " for <b>" + wt.savePrompt.username + "</b>" : "") + " on " + wt.savePrompt.host + "?")
            actions: [
                Kirigami.Action { text: wt.savePrompt && wt.savePrompt.action === "update" ? "Update" : "Save"; icon.name: "document-save"; onTriggered: wt.answerSavePrompt("save") },
                Kirigami.Action { text: "Never for This Site"; icon.name: "dialog-cancel"; visible: !wt.savePrompt || wt.savePrompt.action === "save"; onTriggered: wt.answerSavePrompt("never") },
                Kirigami.Action { text: "Not Now"; onTriggered: wt.answerSavePrompt("dismiss") }
            ]
        }

        Item {
            id: tabBody
            Layout.fillWidth: true
            Layout.fillHeight: true

        Item {
            id: viewHolder
            width: wt.devToolsDocked === "right" ? parent.width - wt.devToolsSpace : parent.width
            height: wt.devToolsDocked === "bottom" ? parent.height - wt.devToolsSpace : parent.height

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 4
                visible: wt.inPip
                icon.name: "kite-pip"
                text: "Playing in picture-in-picture"
                helpfulAction: Kirigami.Action { text: "Back to Tab"; icon.name: "kite-pip-exit"; onTriggered: wt.app.stopPip(true) }
            }

            ElementPicker {
                id: elementPicker
                tab: wt
            }

            WebEngineView {
                // A local file may not read other local files (Chrome's default; Qt's is on).
                settings.localContentCanAccessFileUrls: false
                id: web
                // (bootstrap.cpp: mouse gestures, on tabs' pages only.)
                readonly property bool kiteTabPage: true
                objectName: "kite-web-" + wt.tabId
                anchors.fill: parent
                // Passive: the page still gets every hover event.
                HoverHandler { id: pointerWatch }
                profile: wt.app.profileFor(wt.window.privateMode, wt.container)
                // Split view: clicking into a pane makes it the active one.
                onActiveFocusChanged: {
                    if (activeFocus && wt.devToolsFocusTaken()) {
                        devTools.item.view.forceActiveFocus()
                        return
                    }
                    if (activeFocus && wt.window.split) wt.window.activatePane(wt.pane)
                }
                webChannel: channel
                webChannelWorld: wt.isExtensionPage ? 0 : 1
                lifecycleState: WebEngineView.LifecycleState.Active
                backgroundColor: Kirigami.Theme.backgroundColor

                settings.javascriptEnabled: wt.app.liveSettings["web.javascript"]
                settings.autoLoadImages: wt.app.liveSettings["web.autoloadImages"]
                settings.localStorageEnabled: wt.app.liveSettings["web.localStorage"]
                settings.pluginsEnabled: wt.app.liveSettings["web.pluginsEnabled"]
                settings.spatialNavigationEnabled: wt.app.liveSettings["web.spatialNavigation"]
                // Animated (smooth) scrolling with the mouse wheel and keys.
                settings.scrollAnimatorEnabled: wt.app.liveSettings["web.smoothScrolling"]
                settings.dnsPrefetchEnabled: wt.app.liveSettings["web.dnsPrefetch"]
                // WebRTC only uses (and so reveals) public network addresses.
                settings.webRTCPublicInterfacesOnly: !!wt.app.liveSettings["privacy.webrtcPublicOnly"]
                settings.printElementBackgrounds: wt.app.liveSettings["web.printBackgrounds"]
                settings.fullScreenSupportEnabled: true
                // (Only the shown tab: one loading behind it mustn't take the keyboard.)
                settings.focusOnNavigationEnabled: wt.window && wt.window.currentTab === wt
                settings.screenCaptureEnabled: true
                settings.javascriptCanOpenWindows: true
                // The site's own choice (Site Settings), else the global one.
                settings.forceDarkMode: wt.siteDarkMode === 1 || (wt.siteDarkMode === 0 && wt.app.liveSettings["appearance.forceDarkMode"])
                settings.pdfViewerEnabled: true
                settings.localContentCanAccessRemoteUrls: wt.isExtensionPage

                onUrlChanged: {
                    wt.formDirty = false
                    wt.hasVideo = false
                    const u = url.toString()
                    const host = Browser.hostOf(wt.sourceOf(u))
                    if (host !== wt.lastHost) {
                        wt.lastHost = host
                        wt.applySiteSettings()
                    }
                    if (!loading) {
                        // same-document navigation (pushState / fragment)
                        wt.navEvent(u.split("#")[0] === wt._lastCommitted.split("#")[0] ? "onReferenceFragmentUpdated" : "onHistoryStateUpdated", {transitionType: "link", transitionQualifiers: []})
                        Browser.recordVisit(u, title, wt.window.privateMode)
                    }
                    wt.sendUpdate({url: wt.sourceOf(u)})
                    if (!loading) readerProbe.restart()
                    // (The icon may have gone before the address changed.)
                    if (u.startsWith("kite:reader?")) wt.showSiteIcon()
                }
                onTitleChanged: {
                    wt.sendUpdate({title: title})
                    if (!loading) Browser.updateTitle(url.toString(), title, wt.window.privateMode)
                }
                onIconChanged: {
                    const s = wt.faviconOut ? icon.toString().replace(/^image:\/\/favicon\//, "") : icon.toString()
                    // (Reader view: a kite: page has none; the site's is kept.)
                    if (wt.isReader) { wt.showSiteIcon(); return }
                    wt.window.setTabProperty(wt.tabId, "icon", s)
                    wt.sendUpdate({favIconUrl: s.startsWith("image://favicon/") ? s.substring(16) : s})
                    // Remember it for the speed dial (not from private windows).
                    if (s.startsWith("image://favicon/") && !wt.window.privateMode && /^https?:/.test(url.toString())) Browser.saveFavicon(url.toString(), s)
                }
                onRecentlyAudibleChanged: () => { wt.window.setTabProperty(wt.tabId, "audible", web.recentlyAudible); wt.sendUpdate({audible: web.recentlyAudible}) }
                onAudioMutedChanged: () => { wt.window.setTabProperty(wt.tabId, "muted", web.audioMuted); wt.sendUpdate({mutedInfo: {muted: web.audioMuted}}) }

                onLoadingChanged: info => {
                    const u = info.url.toString()
                    // (Not on failing: a rejected load counts as that.)
                    if (info.status === WebEngineView.LoadSucceededStatus) wt.redirectHops = 0
                    switch (info.status) {
                    case WebEngineView.LoadStartedStatus:
                        wt.crashed = false
                        wt.readerAvailable = false
                        // (A link it was asked about comes through here too.)
                        if (!Browser.isExternalUrl(u)) wt.externalPrompt = ""
                        // A wake lock prompt belongs to the page being left.
                        if (wt.pendingPermissions.some(p => p.wakeLock)) wt.pendingPermissions = wt.pendingPermissions.filter(p => !p.wakeLock)
                        wt.leftForMidi(u)
                        if (elementPicker.visible) elementPicker.visible = false
                        wt.loginForm = false
                        wt.frameLoginForm = false
                        wt.loginFrames = ({})
                        wt._knownFrames = ({})
                        wt.logins = []
                        wt.navEvent("onBeforeNavigate", {url: u, parentFrameId: -1})
                        wt.navEvent("onCommitted", {url: u, transitionType: "link", transitionQualifiers: []})
                        wt._lastCommitted = u
                        wt.sendUpdate({status: "loading", url: u})
                        break
                    case WebEngineView.LoadSucceededStatus:
                        if (wt.faviconOut) wt.iconFromPage()
                        wt.navEvent("onDOMContentLoaded", {url: u})
                        wt.navEvent("onCompleted", {url: u})
                        Browser.recordVisit(u, title, wt.window.privateMode)
                        wt.sendUpdate({status: "complete"})
                        wt.focusPageAfterLoad()
                        readerProbe.restart()
                        break
                    case WebEngineView.LoadFailedStatus:
                        wt.navEvent("onErrorOccurred", {url: u, error: info.errorString})
                        wt.sendUpdate({status: "complete"})
                        wt.focusPageAfterLoad()
                        // HTTPS-only upgraded this page and the secure version
                        // failed: offer the plain http page instead.
                        if (info.errorDomain !== WebEngineView.CertificateErrorDomain && Browser.httpsUpgradeFailed(u))
                            web.url = "kite:https-only?url=" + encodeURIComponent(u.replace(/^https:/, "http:"))
                        break
                    case WebEngineView.LoadStoppedStatus:
                        wt.sendUpdate({status: "complete"})
                        break
                    }
                }

                onLinkHovered: hoveredUrl => wt.hoveredLink = hoveredUrl.toString()

                // "Continue to the HTTP site" on the HTTPS-only page: allowed
                // for this session, then loaded (Chromium won't redirect from
                // a kite: page to plain http).
                // (Also "Continue anyway" on the dangerous-site warning.)
                onNavigationRequested: request => {
                    const u = request.url.toString()
                    wt.app.prepareAdblock(web.profile, u, wt.window.privateMode)
                    if (Browser.isExternalUrl(u)) {
                        request.reject()
                        if (request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation) wt.externalPrompt = u
                        return
                    }
                    const dangerous = u.startsWith("kite:dangerous-allow?")
                    if (!u.startsWith("kite:https-allow?") && !dangerous) {
                        // Where it should go instead: a redirect has to happen here, as
                        // the interceptor's never finishes on a page a service worker
                        // answers for. (Going back to a page, or posting a form, it
                        // stays the interceptor's: those can't simply be loaded again.)
                        if (!request.isMainFrame || request.hasFormData || request.navigationType === WebEngineNavigationRequest.BackForwardNavigation || wt.redirectHops > 5) return
                        const to = Browser.pageLoadRedirect(u, wt.window.privateMode)
                        if (to === "") return
                        request.reject()
                        wt.redirectHops++
                        Qt.callLater(() => { web.url = to })
                        return
                    }
                    request.reject()
                    // Only the warning page's own link counts: it carries a key
                    // web pages can't know, and the warning is the tab's page.
                    if (!request.isMainFrame || !/^kite:(dangerous|https-only)\?/.test(web.url.toString())) return
                    const target = Browser.allowFromWarning(u)
                    if (target === "") return
                    // Not from inside this handler (that re-enters Chromium).
                    Qt.callLater(() => { web.url = target })
                }

                onFileDialogRequested: request => {
                    request.accepted = true
                    pageFileDialogComponent.createObject(wt, {request: request, parentWindow: wt.window, app: wt.app, site: Browser.hostOf(wt.url)})
                }
                onNewWindowRequested: request => {
                    // (No tab for a link to another program: as above.)
                    if (Browser.isExternalUrl(request.requestedUrl.toString())) {
                        if (request.userInitiated) wt.externalPrompt = request.requestedUrl.toString()
                        return
                    }
                    const dest = request.destination
                    const popup = dest === WebEngineNewWindowRequest.InNewDialog || dest === WebEngineNewWindowRequest.InNewWindow
                    if (popup && !wt.app.setting("tabs.openPopupsInTabs")) {
                        const w = popupComponent.createObject(null, {app: wt.app, profile: web.profile, request: request, opener: wt})
                        return
                    }
                    const bg = dest === WebEngineNewWindowRequest.InNewBackgroundTab
                    wt.window.openTab("", {request: request, background: bg && !wt.app.setting("tabs.switchToNewTabFromLink"), index: wt.window.linkTabIndex(wt), openerTabId: wt.tabId, fromLink: true, container: wt.container})
                }

                onWindowCloseRequested: wt.close()

                onFullScreenRequested: request => {
                    request.accept()
                    if (request.toggleOn) wt.window.enterContentFullScreen(wt)
                    else wt.window.exitContentFullScreen(true)
                }

                onPermissionRequested: permission => {
                    if (permission.permissionType === WebEnginePermission.PermissionType.Notifications && wt.window.privateMode && !wt.app.setting("privacy.privateNotifications")) {
                        permission.deny()
                        return
                    }
                    wt.pendingPermissions = wt.pendingPermissions.concat([{permission: permission, origin: Browser.hostOf(permission.origin.toString()) || permission.origin.toString()}])
                }

                onCertificateError: error => {
                    // Accepted for good earlier (Settings → Privacy → Certificate Exceptions).
                    const fingerprint = Browser.certFingerprint(error)
                    if (error.overridable && Browser.certAccepted(Browser.hostOf(error.url.toString()), fingerprint)) {
                        error.acceptCertificate()
                        return
                    }
                    // Like Firefox and Chrome, only the page itself gets the
                    // interstitial; subresources with bad certificates (e.g. a
                    // favicon from a self-signed router) just fail to load.
                    if (!error.isMainFrame) { error.rejectCertificate(); return }
                    if (!wt.shownInPane) {
                        error.rejectCertificate()
                        wt.certRetryUrl = error.url.toString()
                        return
                    }
                    error.defer()
                    wt.certFingerprint = fingerprint
                    wt.certError = error
                }

                onRegisterProtocolHandlerRequested: request => request.accept()

                onAuthenticationDialogRequested: request => {
                    request.accepted = true
                    authDialog.ask(request)
                }

                onContextMenuRequested: request => {
                    request.accepted = true
                    contextMenu.show(request)
                }

                // (window.print() in a page, or one of its frames: the whole page.)
                onPrintRequested: wt.window.printTab(wt)
                onPrintRequestedByFrame: frame => wt.window.printTab(wt)
                onPdfPrintingFinished: (path, ok) => wt.app.pdfFinished(wt, path, ok)

                onFindTextFinished: result => wt.window.findResult(result.numberOfMatches, result.activeMatch)

                onRenderProcessTerminated: (status, code) => {
                    if (status !== WebEngineView.NormalTerminationStatus) wt.crashed = true
                    wt.setWakeLock(false)
                    wt.stopMidi()
                }

                // Extension content scripts (userscript:kite:<id>:...) count as extension output.
                onJavaScriptConsoleMessage: (level, message, line, source) => {
                    // A change from kite:config. (Browser checks its token: another
                    // page's message could arrive just after going there.)
                    if (web.url.toString().startsWith("kite:config") && message.startsWith("kite-config:")) {
                        const answer = Browser.configCommand(message.substring(12))
                        if (answer !== "null") web.runJavaScript("__kiteConfigDone(" + answer + ")")
                        return
                    }
                    if (message.startsWith(wt.app.frameSecret)) {
                        const text = message.substring(wt.app.frameSecret.length)
                        if (text.startsWith("ab:")) wt.adblockFound(text.substring(3))
                        else if (text.startsWith("plugin:")) wt.window.pluginMessage(wt, JSON.parse(text.substring(7)))
                        else wt.frameMessage(text)
                        return
                    }
                    wt.app.logConsole(wt.isExtensionPage || source.startsWith("userscript:kite:"), "tab " + wt.tabId, level, message, source, line)
                }
            }

            ReaderControls {
                tab: wt
                visible: wt.isReader && !wt.inPip
                z: 1
                anchors { top: parent.top; right: parent.right; margins: Kirigami.Units.largeSpacing * 2 }
            }

            // Certificate error interstitial
            Rectangle {
                anchors.fill: parent
                visible: wt.certError !== null
                color: Kirigami.Theme.backgroundColor
                ColumnLayout {
                    anchors.centerIn: parent
                    width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 36)
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon { source: "security-low"; Layout.preferredWidth: Kirigami.Units.iconSizes.huge; Layout.preferredHeight: Layout.preferredWidth; Layout.alignment: Qt.AlignHCenter }
                    Kirigami.Heading { text: "Warning: Potential Security Risk Ahead"; Layout.fillWidth: true; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter }
                    QQC2.Label {
                        Layout.fillWidth: true; wrapMode: Text.Wrap
                        text: wt.certError ? ("Kite detected a potential security threat and did not continue to <b>" + Browser.hostOf(wt.certError.url.toString()) + "</b>.<br><br>" + wt.certError.description) : ""
                    }
                    QQC2.Label {
                        visible: wt.certFingerprint !== ""
                        Layout.fillWidth: true
                        wrapMode: Text.WrapAnywhere
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                        text: "Certificate fingerprint (SHA-256): " + wt.certFingerprint
                    }
                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        QQC2.Button { text: "Go Back (Recommended)"; icon.name: "go-previous"; onClicked: { wt.certError.rejectCertificate(); wt.certError = null; if (web.canGoBack) web.goBack() } }
                        QQC2.Button {
                            text: "Accept the Risk and Continue"; visible: wt.certError && wt.certError.overridable
                            onClicked: { wt.certError.acceptCertificate(); wt.certError = null }
                        }
                        // For good, for this certificate; another one on this site gets
                        // this warning again. (A private window leaves nothing behind.)
                        QQC2.Button {
                            text: "Always Accept This Certificate"
                            visible: wt.certError && wt.certError.overridable && !wt.window.privateMode && wt.certFingerprint !== ""
                            onClicked: {
                                Browser.addCertException(Browser.hostOf(wt.certError.url.toString()), wt.certFingerprint, wt.certError.description)
                                wt.certError.acceptCertificate()
                                wt.certError = null
                            }
                        }
                    }
                }
            }

            // Crashed wt overlay
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 4
                visible: wt.crashed
                icon.name: "tab-close-other"
                text: "This tab has crashed"
                explanation: "The page's process stopped unexpectedly."
                helpfulAction: Kirigami.Action { text: "Reload"; icon.name: "view-refresh"; onTriggered: { wt.crashed = false; web.reload() } }
            }
        }

        // Developer tools docked beside the page.
        Item {
            id: devToolsSlot
            visible: wt.devToolsDocked !== ""
            x: wt.devToolsDocked === "right" ? parent.width - width : 0
            y: wt.devToolsDocked === "bottom" ? parent.height - height : 0
            width: wt.devToolsDocked === "right" ? wt.devToolsSpace : parent.width
            height: wt.devToolsDocked === "bottom" ? wt.devToolsSpace : parent.height
            Kirigami.Separator {
                width: wt.devToolsDocked === "right" ? implicitWidth : parent.width
                height: wt.devToolsDocked === "right" ? parent.height : implicitHeight
                z: 1
            }
        }
        MouseArea {
            visible: wt.devToolsDocked !== ""
            z: 2
            readonly property bool side: wt.devToolsDocked === "right"
            x: side ? devToolsSlot.x - 3 : 0
            y: side ? 0 : devToolsSlot.y - 3
            width: side ? 7 : parent.width
            height: side ? parent.height : 7
            preventStealing: true
            cursorShape: side ? Qt.SplitHCursor : Qt.SplitVCursor
            onPositionChanged: mouse => {
                if (!pressed) return
                const at = mapToItem(tabBody, mouse.x, mouse.y)
                if (side) wt.devToolsWidth = tabBody.width - at.x
                else wt.devToolsHeight = tabBody.height - at.y
            }
            onReleased: wt.rememberDevToolsSize()
        }
        }
    }

    property string _lastCommitted: ""

    function permissionText(type) {
        if (type === "wakeLock") return "keep your screen on"
        if (type === "midi") return "use your MIDI devices"
        if (type === "midiSysex") return "fully control your MIDI devices, which can change their settings and firmware (SysEx)"
        const names = {}
        try {
            names[WebEnginePermission.PermissionType.Geolocation] = "know your location"
            names[WebEnginePermission.PermissionType.Notifications] = "show notifications"
            names[WebEnginePermission.PermissionType.MediaAudioCapture] = "use your microphone"
            names[WebEnginePermission.PermissionType.MediaVideoCapture] = "use your camera"
            names[WebEnginePermission.PermissionType.MediaAudioVideoCapture] = "use your camera and microphone"
            names[WebEnginePermission.PermissionType.MouseLock] = "hide your mouse pointer"
            names[WebEnginePermission.PermissionType.DesktopVideoCapture] = "share your screen"
            names[WebEnginePermission.PermissionType.DesktopAudioVideoCapture] = "share your screen and audio"
            names[WebEnginePermission.PermissionType.ClipboardReadWrite] = "access the clipboard"
            names[WebEnginePermission.PermissionType.LocalFontsAccess] = "access your local fonts"
        } catch (e) {}
        return names[type] || "use a browser feature"
    }
    function dropPermission(i) { const l = pendingPermissions.slice(); l.splice(i, 1); pendingPermissions = l }

    // Screen Wake Lock (crates/kite/src/js/wakelock.js): prompt in the
    // permission bar, remember the answer per site for the session, keep the
    // screen on while the page holds a lock.
    property bool wakeLockHeld: false
    function setWakeLock(on) {
        if (on === wakeLockHeld) return
        wakeLockHeld = on
        Browser.setWakeLock(tabId, on, Browser.hostOf(web.url.toString()))
    }
    function answerWakeLock(ids, granted) {
        if (granted) setWakeLock(true)
        for (const id of ids) bridge.wakeLockAnswer(id, granted)
    }
    function requestWakeLock(id) {
        const site = Browser.hostOf(web.url.toString()) || web.url.toString()
        const key = (window.privateMode ? "private:" : "") + site
        const decided = app.wakeLockSites[key]
        if (decided !== undefined) { answerWakeLock([id], decided); return }
        const open = pendingPermissions.find(p => p.wakeLock)
        if (open) { open.ids.push(id); return }
        const entry = {origin: site, wakeLock: true, ids: [id]}
        entry.permission = {
            permissionType: "wakeLock",
            grant: () => { app.rememberWakeLock(key, true); answerWakeLock(entry.ids, true) },
            deny: () => { app.rememberWakeLock(key, false); answerWakeLock(entry.ids, false) },
        }
        pendingPermissions = pendingPermissions.concat([entry])
    }

    // ---- Web MIDI (crates/kite/src/js/webmidi.js, kite_core::midi) ----
    property string midiSite: ""       // whose session this tab has
    property bool midiSysex: false
    property var midiPorts: []
    function midiSiteNow() { return Browser.hostOf(web.url.toString()) || web.url.toString() }
    // 0 = ask, 1 = allow, 2 = block; of plain access, or with SysEx too.
    function midiChoice(site, sysex) {
        const s = window.privateMode ? (app.midiPrivateSites[site] || {}) : JSON.parse(Browser.siteSettings(site))
        if (sysex) return s.midi === 2 ? 2 : (s.midi_sysex || 0)
        if (s.midi_sysex === 1) return 1
        return s.midi || 0
    }
    function setMidiChoice(site, sysex, value) {
        if (window.privateMode) app.rememberMidi(site, sysex ? "midi_sysex" : "midi", value)
        else Browser.setSiteSetting(site, sysex ? "midiSysex" : "midi", value)
        app.midiChoiceChanged(site)
    }
    function recheckMidi(site) { if (site === midiSite && midiChoice(site, midiSysex) !== 1) stopMidi() }
    function stopMidi() {
        if (midiSite === "") return
        Browser.midiStop(tabId)
        midiSite = ""
        midiSysex = false
        midiPorts = []
        bridge.midiToPage("kite-midi-event", JSON.stringify({event: "ports", ports: []}))
    }
    // A session is for one site. Moving within it, as in a single-page app, doesn't end it.
    function leftForMidi(url) {
        if (pendingPermissions.some(p => p.midi)) pendingPermissions = pendingPermissions.filter(p => !p.midi)
        if (midiSite !== "" && (Browser.hostOf(url) || url) !== midiSite) stopMidi()
    }
    function answerMidi(requests, granted) {
        if (!granted) {
            for (const r of requests) bridge.midiToPage("kite-midi-answer", JSON.stringify({id: r.id, granted: false}))
            return
        }
        const site = midiSiteNow()
        const sysex = requests.some(r => r.sysex)
        if (midiSite === "") {
            const ports = Browser.midiStart(tabId, site, sysex)
            if (ports === "") {
                for (const r of requests) bridge.midiToPage("kite-midi-answer", JSON.stringify({id: r.id, granted: true, error: "MIDI devices aren't available on this system"}))
                return
            }
            midiPorts = JSON.parse(ports)
            midiSite = site
            midiSysex = sysex
        } else if (sysex && !midiSysex) {
            Browser.midiCommand(tabId, JSON.stringify({op: "allowSysex"}))
            midiSysex = true
        }
        for (const r of requests) bridge.midiToPage("kite-midi-answer", JSON.stringify({id: r.id, granted: true, ports: midiPorts}))
    }
    function requestMidi(json) {
        let r
        try { r = JSON.parse(json) } catch (e) { return }
        if (!Number.isInteger(r.id)) return
        const sysex = r.sysex === true
        const request = {id: r.id, sysex: sysex}
        const site = midiSiteNow()
        const choice = midiChoice(site, sysex)
        if (choice !== 0) { answerMidi([request], choice === 1); return }
        const open = pendingPermissions.find(p => p.midi && p.sysex === sysex)
        if (open) { open.requests.push(request); return }
        const entry = {origin: site, midi: true, sysex: sysex, requests: [request]}
        entry.permission = {
            permissionType: sysex ? "midiSysex" : "midi",
            grant: () => { setMidiChoice(site, sysex, 1); answerMidi(entry.requests, true) },
            deny: () => { setMidiChoice(site, sysex, 2); answerMidi(entry.requests, false) },
        }
        pendingPermissions = pendingPermissions.concat([entry])
    }
    function queryMidi(json) {
        let r
        try { r = JSON.parse(json) } catch (e) { return }
        if (!Number.isInteger(r.id)) return
        bridge.midiToPage("kite-midi-state", JSON.stringify({id: r.id, state: ["prompt", "granted", "denied"][midiChoice(midiSiteNow(), r.sysex === true)]}))
    }
    function midiOp(json) {
        if (midiSite === "") return
        // (It got here from a page of the site the session is for, or it ends.)
        if (midiSiteNow() !== midiSite) { stopMidi(); return }
        let r
        try { r = JSON.parse(json) } catch (e) { return }
        if (r.op === "stop") stopMidi()
        // SysEx permission is only ever given above.
        else if (r.op === "open" || r.op === "close" || r.op === "send") Browser.midiCommand(tabId, json)
    }
    // Taken back while in use: it ends now, not at the next visit, in every tab of the site.
    Connections {
        target: wt.app
        enabled: wt.midiSite !== ""
        function onMidiChoiceChanged(site) { wt.recheckMidi(site) }
        function onSiteSettingsChanged(site) { wt.recheckMidi(site) }
    }
    Connections {
        target: Browser
        enabled: wt.midiSite !== ""
        function onMidiEvent(tab, json) {
            if (tab !== wt.tabId || wt.midiSite === "") return
            const e = JSON.parse(json)
            if (e.event === "ports") wt.midiPorts = e.ports
            bridge.midiToPage("kite-midi-event", json)
        }
    }

    WebChannel { id: channel; registeredObjects: [bridge] }
    ExtBridge {
        id: bridge
        viewKey: "tab:" + wt.tabId; tabId: wt.tabId; windowId: wt.window.windowId; viewKind: "tab"
        inPrivate: wt.window.privateMode
        onPasswordFormSubmitted: json => {
            const r = JSON.parse(Passwords.formSubmitted(web.url.toString(), wt.window.privateMode, json))
            if (r) wt.savePrompt = r
        }
        onAutofillFocused: json => { autofillPopup.bridge = bridge; autofillPopup.show(JSON.parse(json)) }
        onAutofillBlurred: autofillPopup.close()
        onAutofillKeyPressed: key => autofillPopup.key(key)
        onAutofillSubmitted: json => {
            const r = JSON.parse(FormFill.formSubmitted(web.url.toString(), wt.window.privateMode, json))
            if (r) wt.addressPrompt = r
        }
        onWakeLockRequested: id => wt.requestWakeLock(id)
        onWakeLockReleased: wt.setWakeLock(false)
        onMidiRequested: json => wt.requestMidi(json)
        onMidiQueried: json => wt.queryMidi(json)
        onMidiOpRequested: json => wt.midiOp(json)
        onFormEdited: dirty => wt.formDirty = dirty
        onLoginFormDetected: emptyPassword => {
            wt.loginForm = true
            wt.refreshLogins()
            const own = wt.logins.find(l => l.frameId === undefined)
            if (emptyPassword && own && wt.autofillHere()) wt.fillLogin(own.id)
        }
    }
    Connections { target: Passwords; function onChanged() { wt.refreshLogins() } }

    PageContextMenu { id: contextMenu; tab: wt }
    AutofillPopup {
        id: autofillPopup
        tab: wt; view: web; bridge: bridge
        onPickedAddress: id => {
            const data = FormFill.fillData((autofillPopup.field && autofillPopup.field.url) || web.url.toString(), id)
            if (data !== "null") autofillPopup.bridge.fillAddress(data)
        }
        onPickedPassword: password => autofillPopup.bridge.fillPassword(password)
        onManage: wt.app.showPreferences("browsing")
    }
    AuthDialog { id: authDialog; tab: wt }

    Loader {
        id: devTools
        active: false
        onLoaded: item.settle()
        sourceComponent: Item {
            id: tools
            readonly property alias view: toolsView
            anchors.fill: parent
            // Where the tab says, which can't be a binding: the loader sets the parent too.
            function settle() { parent = wt.devToolsPlace === "window" ? toolsWindow.contentItem : devToolsSlot }
            function present() {
                if (wt.devToolsPlace === "window") { toolsWindow.show(); toolsWindow.raise(); toolsWindow.requestActivate() }
                toolsView.forceActiveFocus()
            }
            Window {
                id: toolsWindow
                // (Not kept above the browser, as it otherwise would be.)
                transientParent: null
                visible: wt.devToolsPlace === "window"
                width: 1000; height: 700
                title: "Developer Tools — " + wt.title
                onClosing: close => { close.accepted = false; Qt.callLater(wt.closeDevTools) }
            }
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                QQC2.ToolBar {
                    Layout.fillWidth: true
                    position: QQC2.ToolBar.Header
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        QQC2.Label {
                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.smallSpacing
                            text: "Developer Tools"
                            elide: Text.ElideRight
                        }
                        Repeater {
                            model: [
                                {place: "right", text: "Dock to Right", icon: "view-split-left-right"},
                                {place: "bottom", text: "Dock to Bottom", icon: "view-split-top-bottom"},
                                {place: "window", text: "Open in a Window", icon: "window-new"},
                            ]
                            delegate: QQC2.ToolButton {
                                required property var modelData
                                icon.name: modelData.icon
                                text: modelData.text
                                display: QQC2.AbstractButton.IconOnly
                                down: wt.devToolsPlace === modelData.place
                                onClicked: wt.setDevToolsPlace(modelData.place)
                                QQC2.ToolTip.text: text
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            }
                        }
                        QQC2.ToolButton {
                            icon.name: "window-close"
                            text: "Close Developer Tools"
                            display: QQC2.AbstractButton.IconOnly
                            onClicked: wt.closeDevTools()
                            QQC2.ToolTip.text: text
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }
                }
                WebEngineView {
                    // A local file may not read other local files (Chrome's default; Qt's is on).
                    settings.localContentCanAccessFileUrls: false
                    id: toolsView
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    profile: web.profile
                    inspectedView: web
                    backgroundColor: Kirigami.Theme.backgroundColor
                    // Split view: clicking into a tab's tools makes its pane the active one.
                    onActiveFocusChanged: {
                        if (!activeFocus) wt.devToolsLostFocusAt = Date.now()
                        else if (wt.window.split) wt.window.activatePane(wt.pane)
                    }
                    // (Its own close button, beside its tabs.)
                    onWindowCloseRequested: Qt.callLater(wt.closeDevTools)
                }
            }
        }
    }

    Component {
        id: pageFileDialogComponent
        PageFileDialog {}
    }

    Component {
        id: popupComponent
        PopupWindow {}
    }
}
