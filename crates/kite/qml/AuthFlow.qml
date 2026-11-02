import QtQuick
import QtQuick.Window
import QtWebEngine
import org.kde.kirigami as Kirigami

// identity.launchWebAuthFlow: show the provider's login page and finish when
// it redirects to the extension's redirect URL.
Window {
    id: w
    property WebEngineProfile profile
    property var onDone: null
    property var onFail: null
    property bool finished: false
    width: 520; height: 640
    title: "Sign in"
    color: Kirigami.Theme.backgroundColor
    function start(details, done, fail) {
        onDone = done; onFail = fail; finished = false
        view.url = details.url
        if (details.interactive !== false) { visible = true; raise(); requestActivate() }
    }
    onClosing: if (!finished && onFail) { finished = true; onFail("User cancelled or denied access.") }
    WebEngineView {
        id: view
        anchors.fill: parent
        profile: w.profile
        onUrlChanged: {
            const u = url.toString()
            if (u.indexOf(".extensions.allizom.org/") > 0 || u.indexOf(".chromiumapp.org/") > 0) {
                w.finished = true
                w.visible = false
                if (w.onDone) w.onDone(u)
                view.url = "about:blank"
            }
        }
    }
}
