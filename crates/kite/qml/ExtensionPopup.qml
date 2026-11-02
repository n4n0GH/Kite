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
    width: Math.max(Kirigami.Units.gridUnit * 4, Math.min(800, view.contentsSize.width || 320))
    height: Math.max(Kirigami.Units.gridUnit * 2, Math.min(600, view.contentsSize.height || 200))
    closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutside

    function show(action, anchor) {
        popup.action = action
        parent = anchor
        const p = anchor.mapToItem(win.contentItem, 0, anchor.height)
        x = 0; y = anchor.height
        popupUrl = action.popup
        view.url = action.popup
        open()
    }

    onClosed: { view.url = "about:blank"; popupUrl = "" }

    contentItem: WebEngineView {
        id: view
        profile: popup.profile
        webChannel: WebChannel { registeredObjects: [bridge] }
        settings.localContentCanAccessRemoteUrls: true
        settings.javascriptCanAccessClipboard: true
        backgroundColor: "white"
        onWindowCloseRequested: popup.close()
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
        onJavaScriptConsoleMessage: (level, message, line, source) => { if (level >= WebEngineView.WarningMessageLevel) console.warn("[popup] " + message + " (" + source + ":" + line + ")") }
    }
    ExtBridge {
        id: bridge
        viewKey: "popup:" + (popup.action ? popup.action.ext : "")
        viewKind: "popup"
        tabId: popup.win.currentTab ? popup.win.currentTab.tabId : -1
        windowId: popup.win.windowId
    }
}
