import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kite

// The saved sessions: one opens beside what is open already (nothing is lost),
// and saving the current windows under a name. Replacing what is open is in
// Manage Sessions.
QQC2.Menu {
    id: menu
    property var win
    title: "Sessions"
    icon.name: "document-multiple"
    property var sessions: []
    onAboutToShow: sessions = JSON.parse(Browser.namedSessions())

    Instantiator {
        model: menu.sessions
        delegate: QQC2.MenuItem {
            required property var modelData
            text: modelData.name
            icon.name: "document-open"
            onTriggered: menu.win.app.openNamedSession(modelData.name, false)
        }
        onObjectAdded: (i, o) => menu.insertItem(i, o)
        onObjectRemoved: (i, o) => menu.removeItem(o)
    }
    QQC2.MenuSeparator { visible: menu.sessions.length > 0; height: visible ? implicitHeight : 0 }
    QQC2.MenuItem { text: "Save Current Session…"; icon.name: "document-save"; onTriggered: menu.win.saveSessionAs() }
    QQC2.MenuItem { text: "Manage Sessions…"; icon.name: "configure"; onTriggered: menu.win.manageSessions() }
}
