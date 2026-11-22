import QtQuick
import QtQuick.Window
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite
import "extcall.js" as ExtCall

// Extension options page shown in its own window (options_ui without open_in_tab).
Window {
    id: w
    property WebEngineProfile profile
    width: Kirigami.Units.gridUnit * 40
    height: Kirigami.Units.gridUnit * 30
    color: Kirigami.Theme.backgroundColor
    property string pageUrl: ""
    // A link in the page that leads out of the extension, for a tab.
    signal linkOpened(string url)
    function open(url, name) {
        title = name + " — Options"
        pageUrl = url
        view.active = true
        view.item.url = url
        visible = true; raise(); requestActivate()
    }
    onClosing: view.active = false
    // Only while open: a web view holds a renderer process even when it shows nothing.
    Loader {
        id: view
        anchors.fill: parent
        active: false
        sourceComponent: WebEngineView {
            // A local file may not read other local files (Chrome's default; Qt's is on).
            settings.localContentCanAccessFileUrls: false
            profile: w.profile
            webChannel: WebChannel { registeredObjects: [bridge.channel] }
            settings.localContentCanAccessRemoteUrls: true
            // (As in ExtensionPopup.)
            settings.javascriptCanAccessClipboard: true
            settings.javascriptCanPaste: Extensions.urlHasPermission(w.pageUrl, "clipboardRead")
            onPermissionRequested: permission => ExtCall.refuseClipboard(permission, WebEnginePermission.PermissionType.ClipboardReadWrite)
            onWindowCloseRequested: w.close()
            // (As in ExtensionPopup: this view stays the extension's.)
            // Its links to the web open in a tab.
            onNavigationRequested: request => {
                const to = request.url.toString()
                if (request.isMainFrame && !ExtCall.staysHome(to, w.pageUrl)) {
                    request.reject()
                    // (Not from inside this handler: that re-enters Chromium.)
                    if (ExtCall.mayOpenFrom(to, w.pageUrl)) Qt.callLater(() => w.linkOpened(to))
                }
            }
            onNewWindowRequested: request => { if (ExtCall.mayOpenFrom(request.requestedUrl.toString(), w.pageUrl)) w.linkOpened(request.requestedUrl.toString()) }
            id: optionsView
            ExtBridge {
                id: bridge; viewKey: "options"; viewKind: "options"
                onCalled: (token, ctx, method, args, callId) => Extensions.call(viewKey, tabId, windowId, ExtCall.documentOf(optionsView), token, ctx, method, args, callId)
            }
        }
    }
}
