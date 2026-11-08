import QtQuick
import QtQuick.Controls as QQC2
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite

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
    }

    onClosed: { active = false; view.active = false; popupUrl = "" }

    // Only while open: a web view holds a renderer process even when it shows nothing,
    // and each window has one of these popups.
    contentItem: Loader {
        id: view
        active: false
        sourceComponent: WebEngineView {
            // A local file may not read other local files (Chrome's default; Qt's is on).
            settings.localContentCanAccessFileUrls: false
            profile: popup.profile
            webChannel: WebChannel { registeredObjects: [bridge] }
            settings.localContentCanAccessRemoteUrls: true
            settings.javascriptCanAccessClipboard: true
            settings.scrollAnimatorEnabled: popup.win.app.liveSettings["web.smoothScrolling"]
            backgroundColor: "white"
            onWindowCloseRequested: popup.close()
            onContentsSizeChanged: popup.takeSize()
            onLoadingChanged: info => { if (info.status === WebEngineView.LoadSucceededStatus) popup.takeSize() }
            onNewWindowRequested: request => { popup.win.openTab(request.requestedUrl.toString(), {background: false}); popup.close() }
            onNavigationRequested: request => {
                // Links that leave the extension open in a tab.
                if (request.isMainFrame && !request.url.toString().startsWith("moz-extension:") && request.url.toString() !== "about:blank") {
                    request.reject()
                    popup.win.openTab(request.url.toString(), {background: false})
                    popup.close()
                }
            }
            onContextMenuRequested: request => request.accepted = false
            onJavaScriptConsoleMessage: (level, message, line, source) => popup.win.app.logConsole(true, "popup", level, message, source, line)
        }
    }
    ExtBridge {
        id: bridge
        viewKey: "popup:" + (popup.action ? popup.action.ext : "")
        viewKind: "popup"
        tabId: popup.win.currentTab ? popup.win.currentTab.tabId : -1
        windowId: popup.win.windowId
    }
}
