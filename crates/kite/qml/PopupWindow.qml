import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.kde.kite

// Window for JavaScript popups (window.open with features), like Falkon's
// PopupWindow: read-only location bar and the page.
Window {
    id: w
    property var app
    property var opener
    property WebEngineProfile profile
    property var request
    // An address for another app the page asked to open (as in a tab: only after
    // a click, and only once the user says so).
    property string externalPrompt: ""
    // (As the page asks, but a window: not one too small to see, or to find its address in.)
    width: Math.max(320, request && request.requestedGeometry.width > 0 ? request.requestedGeometry.width : 640)
    height: Math.max(240, request && request.requestedGeometry.height > 0 ? request.requestedGeometry.height + bar.height : 480)
    x: request && request.requestedGeometry.x > 0 ? request.requestedGeometry.x : 100
    y: request && request.requestedGeometry.y > 0 ? request.requestedGeometry.y : 100
    title: view.title
    visible: true
    color: Kirigami.Theme.backgroundColor
    onClosing: Qt.callLater(() => w.destroy())

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        QQC2.TextField {
            id: bar
            Layout.fillWidth: true
            readOnly: true
            text: view.url
            leftPadding: Kirigami.Units.gridUnit * 1.5
            Kirigami.Icon { source: view.url.toString().startsWith("https") ? "security-high" : "security-low"; width: Kirigami.Units.iconSizes.small; height: width; anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter } }
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: w.externalPrompt !== ""
            type: Kirigami.MessageType.Warning
            icon.name: "document-open-remote"
            text: "This page wants to open <b>" + w.externalPrompt.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</b> in another app."
            actions: [
                Kirigami.Action { text: "Open"; icon.name: "document-open"; onTriggered: { const u = w.externalPrompt; w.externalPrompt = ""; if (w.opener && w.opener.window) w.opener.window.openExternal(u); else Qt.openUrlExternally(u) } },
                Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: w.externalPrompt = "" }
            ]
        }
        WebEngineView {
            // A local file may not read other local files (Chrome's default; Qt's is on).
            settings.localContentCanAccessFileUrls: false
            // What is set for pages holds for a page in a window of its own as in a tab
            // (WebTab.qml): a page could else open one to be rid of it.
            settings.javascriptEnabled: w.app.liveSettings["web.javascript"]
            settings.autoLoadImages: w.app.liveSettings["web.autoloadImages"]
            settings.localStorageEnabled: w.app.liveSettings["web.localStorage"]
            settings.dnsPrefetchEnabled: w.app.liveSettings["web.dnsPrefetch"]
            settings.webRTCPublicInterfacesOnly: !!w.app.liveSettings["privacy.webrtcPublicOnly"]
            settings.pdfViewerEnabled: !!w.app.liveSettings["web.pdfViewer"]
            settings.pluginsEnabled: !!w.app.liveSettings["web.pdfViewer"]
            id: view
            // (A name to find it by from C++, Browser.adoptedView.)
            objectName: "kite-popup-web-" + Date.now().toString(36) + Math.floor(Math.random() * 1e6).toString(36)
            // Other apps' addresses (mailto:, smb:, …): asked about, as in a tab,
            // never handed to the desktop straight away.
            onNavigationRequested: request => {
                const u = request.url.toString()
                // (A link of a kind a site was allowed to open: as in a tab, WebTab.qml.)
                const handler = w.opener && w.opener.window && !w.opener.window.privateMode ? Browser.linkHandlerTarget(u) : ""
                if (handler !== "" && (request.isMainFrame || request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation)) {
                    request.reject()
                    Qt.callLater(() => { view.url = handler })
                    return
                }
                if (!Browser.isExternalUrl(u)) return
                request.reject()
                if (request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation) w.externalPrompt = u
            }
            Layout.fillWidth: true
            Layout.fillHeight: true
            profile: w.profile
            onWindowCloseRequested: w.close()
            onNewWindowRequested: request => w.opener.window.openTab("", {request: request, background: false, container: w.opener.container})
            onGeometryChangeRequested: geometry => { w.x = geometry.x; w.y = geometry.y; w.width = Math.max(320, geometry.width); w.height = Math.max(240, geometry.height + bar.height) }
            // (adoptedView: as for a tab a page opened, BrowserWindow.qml.)
            Component.onCompleted: { w.request.openIn(view); Browser.adoptedView(view.objectName) }
        }
    }
}
