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
    property string url: heldUrl !== "" ? heldUrl : pendingUrl !== "" ? pendingUrl : web.url.toString()
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
    // Password manager
    property var logins: []
    property bool loginForm: false
    property var savePrompt: null

    function refreshLogins() { logins = loginForm ? JSON.parse(Passwords.logins(web.url.toString())) : [] }
    function fillLogin(id) {
        const data = Passwords.fillData(web.url.toString(), id)
        if (data !== "null") bridge.fillLogin(data)
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
        pendingUrl = ""
        if (!app.networkReady && /^(https?|wss?):/i.test(u)) { heldUrl = u; return }
        heldUrl = ""
        crashed = false
        certError = null
        web.url = u
    }

    onActivated: {
        lastAccessed = Date.now()
        if (pendingUrl !== "") { const u = pendingUrl; load(u) }
        web.lifecycleState = WebEngineView.LifecycleState.Active
        if (url === "" || url === app.newTabUrl()) focusLocationBar()
        else web.forceActiveFocus()
    }

    function activate() { window.setCurrentIndex(window.indexOfTab(tabId)); window.raise(); window.requestActivate() }
    function close() { window.closeTabById(tabId) }
    function focusLocationBar() { Qt.callLater(() => { if (window.currentTab === wt) window.focusLocationBarNow() }) }

    function setPinned(p) {
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

    function zoomStep(dir) {
        const steps = [0.3, 0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0]
        let i = steps.findIndex(s => s >= web.zoomFactor - 0.001)
        if (i < 0) i = steps.length - 1
        setZoom(steps[Math.max(0, Math.min(steps.length - 1, i + dir))])
    }
    function setZoom(z) {
        web.zoomFactor = z
        const host = Browser.hostOf(url)
        if (host) Browser.setSiteSetting(host, "zoom", Math.round(z * 100))
    }

    function saveState() {
        return {url: url, title: title, pinned: pinned, zoom: web.zoomFactor, history: [], history_index: 0}
    }

    function extInfo() {
        return {
            id: tabId, index: window.indexOfTab(tabId), windowId: window.windowId, active: window.currentTab === wt,
            highlighted: window.currentTab === wt, pinned: pinned, url: url, title: title,
            favIconUrl: web.icon.toString().startsWith("image://favicon/") ? web.icon.toString().substring(16) : "",
            status: status, incognito: window.privateMode, audible: web.recentlyAudible, discarded: pendingUrl !== "",
            mutedInfo: {muted: web.audioMuted}, width: web.width, height: web.height, lastAccessed: lastAccessed,
            openerTabId: openerTabId >= 0 ? openerTabId : undefined, hidden: false, isArticle: false, isInReaderMode: false,
        }
    }

    function sendUpdate(changes) {
        Extensions.browserEvent("tabUpdated", JSON.stringify({tab: extInfo(), changes: changes}))
        if (window.currentTab === wt) window.refreshToolbar()
    }

    function navEvent(ev, extra) {
        Extensions.browserEvent("navigation", JSON.stringify(Object.assign({event: ev, tabId: tabId, url: url, frameId: 0}, extra || {})))
    }

    function showDevTools() {
        if (!devTools.item) devTools.active = true
        devTools.item.show()
        devTools.item.raise()
    }

    Component.onCompleted: window.registerTab(wt)
    Component.onDestruction: { setWakeLock(false); window.unregisterTab(wt) }

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
            Layout.fillWidth: true
            Layout.fillHeight: true

            WebEngineView {
                id: web
                anchors.fill: parent
                profile: wt.window.privateMode ? wt.app.privateProfile : wt.app.profile
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
                settings.printElementBackgrounds: wt.app.liveSettings["web.printBackgrounds"]
                settings.fullScreenSupportEnabled: true
                settings.focusOnNavigationEnabled: true
                settings.screenCaptureEnabled: true
                settings.javascriptCanOpenWindows: true
                settings.forceDarkMode: wt.app.liveSettings["appearance.forceDarkMode"]
                settings.pdfViewerEnabled: true
                settings.localContentCanAccessRemoteUrls: wt.isExtensionPage

                onUrlChanged: {
                    const u = url.toString()
                    const host = Browser.hostOf(u)
                    if (host !== wt.lastHost) {
                        wt.lastHost = host
                        const s = JSON.parse(Browser.siteSettings(host))
                        zoomFactor = s.zoom > 0 ? s.zoom / 100 : wt.app.setting("appearance.defaultZoom") / 100
                    }
                    if (!loading) {
                        // same-document navigation (pushState / fragment)
                        wt.navEvent(u.split("#")[0] === wt._lastCommitted.split("#")[0] ? "onReferenceFragmentUpdated" : "onHistoryStateUpdated", {transitionType: "link", transitionQualifiers: []})
                        Browser.recordVisit(u, title)
                    }
                    wt.sendUpdate({url: u})
                }
                onTitleChanged: {
                    wt.sendUpdate({title: title})
                    if (!loading) Browser.updateTitle(url.toString(), title)
                }
                onIconChanged: {
                    const s = icon.toString()
                    wt.window.setTabProperty(wt.tabId, "icon", s)
                    wt.sendUpdate({favIconUrl: s.startsWith("image://favicon/") ? s.substring(16) : s})
                    // Remember it for the speed dial (not from private windows).
                    if (s.startsWith("image://favicon/") && !wt.window.privateMode && /^https?:/.test(url.toString())) Browser.saveFavicon(url.toString(), s)
                }
                onRecentlyAudibleChanged: () => { wt.window.setTabProperty(wt.tabId, "audible", web.recentlyAudible); wt.sendUpdate({audible: web.recentlyAudible}) }
                onAudioMutedChanged: () => { wt.window.setTabProperty(wt.tabId, "muted", web.audioMuted); wt.sendUpdate({mutedInfo: {muted: web.audioMuted}}) }

                onLoadingChanged: info => {
                    const u = info.url.toString()
                    switch (info.status) {
                    case WebEngineView.LoadStartedStatus:
                        wt.crashed = false
                        // A wake lock prompt belongs to the page being left.
                        if (wt.pendingPermissions.some(p => p.wakeLock)) wt.pendingPermissions = wt.pendingPermissions.filter(p => !p.wakeLock)
                        wt.loginForm = false
                        wt.logins = []
                        wt.navEvent("onBeforeNavigate", {url: u, parentFrameId: -1})
                        wt.navEvent("onCommitted", {url: u, transitionType: "link", transitionQualifiers: []})
                        wt._lastCommitted = u
                        wt.sendUpdate({status: "loading", url: u})
                        break
                    case WebEngineView.LoadSucceededStatus:
                        wt.navEvent("onDOMContentLoaded", {url: u})
                        wt.navEvent("onCompleted", {url: u})
                        Browser.recordVisit(u, title)
                        wt.sendUpdate({status: "complete"})
                        break
                    case WebEngineView.LoadFailedStatus:
                        wt.navEvent("onErrorOccurred", {url: u, error: info.errorString})
                        wt.sendUpdate({status: "complete"})
                        break
                    case WebEngineView.LoadStoppedStatus:
                        wt.sendUpdate({status: "complete"})
                        break
                    }
                }

                onLinkHovered: hoveredUrl => wt.hoveredLink = hoveredUrl.toString()

                onNewWindowRequested: request => {
                    const dest = request.destination
                    const popup = dest === WebEngineNewWindowRequest.InNewDialog || dest === WebEngineNewWindowRequest.InNewWindow
                    if (popup && !wt.app.setting("tabs.openPopupsInTabs")) {
                        const w = popupComponent.createObject(null, {app: wt.app, profile: web.profile, request: request, opener: wt})
                        return
                    }
                    const bg = dest === WebEngineNewWindowRequest.InNewBackgroundTab
                    wt.window.openTab("", {request: request, background: bg && !wt.app.setting("tabs.switchToNewTabFromLink"), index: wt.window.indexOfTab(wt.tabId) + 1, openerTabId: wt.tabId})
                }

                onWindowCloseRequested: wt.close()

                onFullScreenRequested: request => {
                    request.accept()
                    wt.window.contentFullScreen = request.toggleOn
                    if (request.toggleOn) wt.window.showFullScreen(); else wt.window.showNormal()
                }

                onPermissionRequested: permission => {
                    wt.pendingPermissions = wt.pendingPermissions.concat([{permission: permission, origin: Browser.hostOf(permission.origin.toString()) || permission.origin.toString()}])
                }

                onCertificateError: error => {
                    // Like Firefox and Chrome, only the page itself gets the
                    // interstitial; subresources with bad certificates (e.g. a
                    // favicon from a self-signed router) just fail to load.
                    if (!error.isMainFrame) { error.rejectCertificate(); return }
                    error.defer()
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

                onFindTextFinished: result => wt.window.findResult(result.numberOfMatches, result.activeMatch)

                onRenderProcessTerminated: (status, code) => {
                    if (status !== WebEngineView.NormalTerminationStatus) wt.crashed = true
                    wt.setWakeLock(false)
                }

                // Extension content scripts (userscript:kite:<id>:...) count as extension output.
                onJavaScriptConsoleMessage: (level, message, line, source) => wt.app.logConsole(wt.isExtensionPage || source.startsWith("userscript:kite:"), "tab " + wt.tabId, level, message, source, line)
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
                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        QQC2.Button { text: "Go Back (Recommended)"; icon.name: "go-previous"; onClicked: { wt.certError.rejectCertificate(); wt.certError = null; if (web.canGoBack) web.goBack() } }
                        QQC2.Button {
                            text: "Accept the Risk and Continue"; visible: wt.certError && wt.certError.overridable
                            onClicked: { wt.certError.acceptCertificate(); wt.certError = null }
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
                text: "This wt has crashed"
                explanation: "The page's process stopped unexpectedly."
                helpfulAction: Kirigami.Action { text: "Reload"; icon.name: "view-refresh"; onTriggered: { wt.crashed = false; web.reload() } }
            }
        }
    }

    property string _lastCommitted: ""

    function permissionText(type) {
        if (type === "wakeLock") return "keep your screen on"
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

    WebChannel { id: channel; registeredObjects: [bridge] }
    ExtBridge {
        id: bridge
        viewKey: "tab:" + wt.tabId; tabId: wt.tabId; windowId: wt.window.windowId; viewKind: "tab"
        onPasswordFormSubmitted: json => {
            const r = JSON.parse(Passwords.formSubmitted(web.url.toString(), wt.window.privateMode, json))
            if (r) wt.savePrompt = r
        }
        onWakeLockRequested: id => wt.requestWakeLock(id)
        onWakeLockReleased: wt.setWakeLock(false)
        onLoginFormDetected: emptyPassword => {
            wt.loginForm = true
            wt.refreshLogins()
            if (emptyPassword && wt.logins.length > 0 && Passwords.autofillEnabled()) wt.fillLogin(wt.logins[0].id)
        }
    }
    Connections { target: Passwords; function onChanged() { wt.refreshLogins() } }

    PageContextMenu { id: contextMenu; tab: wt }
    AuthDialog { id: authDialog; tab: wt }

    Loader {
        id: devTools
        active: false
        sourceComponent: Window {
            width: 1000; height: 700
            title: "Developer Tools — " + wt.title
            WebEngineView {
                anchors.fill: parent
                profile: web.profile
                inspectedView: web
            }
        }
    }

    Component {
        id: popupComponent
        PopupWindow {}
    }
}
