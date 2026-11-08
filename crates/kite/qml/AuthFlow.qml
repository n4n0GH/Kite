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
        view.active = true
        view.item.url = details.url
        if (details.interactive !== false) { visible = true; raise(); requestActivate() }
    }
    onClosing: {
        view.active = false
        if (!finished && onFail) { finished = true; onFail("User cancelled or denied access.") }
    }
    // Only during a sign-in: a web view holds a renderer process even when it shows nothing.
    Loader {
        id: view
        anchors.fill: parent
        active: false
        sourceComponent: WebEngineView {
            // A local file may not read other local files (Chrome's default; Qt's is on).
            settings.localContentCanAccessFileUrls: false
            profile: w.profile
            onUrlChanged: {
                const u = url.toString()
                if (u.indexOf(".extensions.allizom.org/") > 0 || u.indexOf(".chromiumapp.org/") > 0) {
                    w.finished = true
                    w.visible = false
                    if (w.onDone) w.onDone(u)
                    view.active = false
                }
            }
        }
    }
}
