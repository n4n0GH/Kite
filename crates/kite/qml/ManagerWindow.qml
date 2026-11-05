import QtQuick
import org.kde.kirigami as Kirigami

// A window besides the browser's (add-ons, library, downloads, settings).
//
// Kirigami gives each of its windows an application-wide shortcut for the
// standard Quit key that closes that window. With one open, the key would have
// two meanings anywhere in Kite, and Qt runs only one of them, so Ctrl+Q would
// close that window and leave the browser running. Here the key means what it
// means everywhere else: quit Kite ("quit" in Settings → Shortcuts).
Kirigami.ApplicationWindow {
    id: managerWindow
    property var app

    quitAction.enabled: false
    Shortcut {
        sequences: managerWindow.app ? managerWindow.app.shortcutKeys("quit") : []
        onActivated: managerWindow.app.quit()
    }
}
