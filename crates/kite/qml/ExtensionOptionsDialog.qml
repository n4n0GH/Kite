import QtQuick
import QtQuick.Window
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami

// Extension options page shown in its own window (options_ui without open_in_tab).
Window {
    id: w
    property WebEngineProfile profile
    width: Kirigami.Units.gridUnit * 40
    height: Kirigami.Units.gridUnit * 30
    color: Kirigami.Theme.backgroundColor
    function open(url, name) {
        title = name + " — Options"
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
            webChannel: WebChannel { registeredObjects: [bridge] }
            settings.localContentCanAccessRemoteUrls: true
            onWindowCloseRequested: w.close()
            ExtBridge { id: bridge; viewKey: "options"; viewKind: "options" }
        }
    }
}
