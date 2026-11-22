import QtQuick
import QtQuick.Controls as QQC2
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite
import "extcall.js" as ExtCall

// Browser/page action popup: an extension page shown in a panel under its
// toolbar button, sized to its content (like Firefox).
QQC2.Popup {
    id: popup
    property var win
    property WebEngineProfile profile
    property var action: null
    property string popupUrl: ""
    padding: 1
    // Keep the popup inside the browser window (Qt only does this when
    // margins are set) and limit it to the space below the toolbar.
    margins: Kirigami.Units.smallSpacing
    property Item anchorItem: null
    readonly property real maxPopupWidth: win ? win.width - 2 * margins : 800
    readonly property real maxPopupHeight: {
        if (!win || !anchorItem) return 600
        const bottom = anchorItem.mapToItem(null, 0, anchorItem.height).y
        return Math.max(Kirigami.Units.gridUnit * 4, win.height - bottom - 2 * margins)
    }
    // Firefox caps extension popups at 800×600.
    // Size of the extension page's content. A page never reports less than
    // the space it is given, so this is reset on every open (otherwise a
    // popup can only grow and keeps the previous, larger size).
    property real pageWidth: 0
    property real pageHeight: 0
    // From show() until closed. (Popup.opened only turns true after the
    // open transition, by which time the page may already have laid out.)
    property bool active: false
    function takeSize() {
        if (!active || !view.item) return
        pageWidth = view.item.contentsSize.width
        pageHeight = view.item.contentsSize.height
    }
    width: Math.min(maxPopupWidth, Math.max(Kirigami.Units.gridUnit * 4, Math.min(800, pageWidth || 320)) + 2 * padding)
    height: Math.min(maxPopupHeight, Math.max(Kirigami.Units.gridUnit * 2, Math.min(600, pageHeight || 200)) + 2 * padding)
    // Right-aligned with the button, so it grows towards the window's inside.
    x: anchorItem ? anchorItem.width - width : 0
    y: anchorItem ? anchorItem.height : 0
    // Presses on the popup's own button don't close it here; the button's
    // click toggles it instead (otherwise press closes and release reopens).
    closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
    focus: true

    function show(action, anchor) {
        if (opened && anchorItem === anchor) {
            close()
            return
        }
        popup.action = action
        pageWidth = 0
        pageHeight = 0
        active = true
        parent = anchor
        anchorItem = anchor
        popupUrl = action.popup
        view.active = true
        view.item.url = action.popup
        open()
        // (Typing goes to the popup at once, as in Firefox: it may have been opened with a shortcut.)
        view.item.forceActiveFocus()
    }

    // (The runtime is told the page is gone: the bridge outlives the view, and a page that still
    // counted would be waited for by every message of its extension that nobody answers.)
    onClosed: { active = false; view.active = false; popupUrl = ""; Extensions.viewClosed(bridge.viewKey) }

    // Only while open: a web view holds a renderer process even when it shows nothing,
    // and each window has one of these popups.
    contentItem: Loader {
        id: view
        active: false
        sourceComponent: WebEngineView {
            // A local file may not read other local files (Chrome's default; Qt's is on).
            settings.localContentCanAccessFileUrls: false
            profile: popup.profile
            webChannel: WebChannel { registeredObjects: [bridge.channel] }
            settings.localContentCanAccessRemoteUrls: true
            // The clipboard: writing as any extension page may (Firefox lets them, from a click),
            // reading only with the manifest's clipboardRead.
            settings.javascriptCanAccessClipboard: true
            settings.javascriptCanPaste: Extensions.urlHasPermission(popup.popupUrl, "clipboardRead")
            onPermissionRequested: permission => ExtCall.refuseClipboard(permission, WebEnginePermission.PermissionType.ClipboardReadWrite)
            settings.scrollAnimatorEnabled: popup.win.app.liveSettings["web.smoothScrolling"]
            backgroundColor: "white"
            onWindowCloseRequested: popup.close()
            onContentsSizeChanged: popup.takeSize()
            onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) popup.takeSize() }
            onNewWindowRequested: request => {
                const to = request.requestedUrl.toString()
                if (ExtCall.mayOpenFrom(to, popup.popupUrl)) popup.win.openTab(to, {background: false})
                popup.close()
            }
            onNavigationRequested: request => {
                // Links that leave the extension open in a tab (those to the web, or nowhere).
                const to = request.url.toString()
                if (request.isMainFrame && !ExtCall.staysHome(to, popup.popupUrl)) {
                    request.reject()
                    if (ExtCall.mayOpenFrom(to, popup.popupUrl)) popup.win.openTab(to, {background: false})
                    popup.close()
                }
            }
            onContextMenuRequested: request => request.accepted = false
            onJavaScriptConsoleMessage: (level, message, line, source) => popup.win.app.logConsole(true, "popup", level, message, source, line, popup.win.privateMode)
        }
    }
    ExtBridge {
        id: bridge
        viewKey: "popup:" + (popup.action ? popup.action.ext : "")
        viewKind: "popup"
        onCalled: (token, ctx, method, args, callId) => Extensions.call(viewKey, tabId, windowId, ExtCall.documentOf(view.item), token, ctx, method, args, callId)
        tabId: popup.win.currentTab ? popup.win.currentTab.tabId : -1
        windowId: popup.win.windowId
    }
}
