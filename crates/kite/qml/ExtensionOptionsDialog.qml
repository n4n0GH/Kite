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
        view.url = url
        visible = true; raise(); requestActivate()
    }
    onClosing: view.url = "about:blank"
    WebEngineView {
        id: view
        anchors.fill: parent
        profile: w.profile
        webChannel: WebChannel { registeredObjects: [bridge] }
        settings.localContentCanAccessRemoteUrls: true
        onWindowCloseRequested: w.close()
        ExtBridge { id: bridge; viewKey: "options"; viewKind: "options" }
    }
}
