import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kite
import "plaintext.js" as PlainText

// A submenu listing the saved sessions, to send tabs to one of them or to a new
// one (BrowserWindow.sendTabsToSession): "Send Tab to Session". It puts itself
// into `parentMenu` after the item `after`, except in a private window, whose
// tabs aren't kept anywhere. Declare it as a property of the parent menu, not
// inside it (a Menu declared inside a Menu is always added as a submenu).
QQC2.Menu {
    id: sm
    property var win
    property QQC2.Menu parentMenu
    property var after
    // The ids of the tabs to send: asked for when a session is chosen.
    property var tabs: () => []
    title: "Send to Session"
    icon.name: "document-send"
    property var sessions: []
    onAboutToShow: sessions = JSON.parse(Browser.namedSessions())

    // (Once the menus are closed: the tabs go, and with them the tab this menu may be of.)
    function send(name) {
        const win = sm.win, ids = sm.tabs()
        if (parentMenu) parentMenu.close()
        Qt.callLater(() => { if (name === "") win.sendTabsToNewSession(ids); else win.sendTabsToSession(ids, name) })
    }

    Instantiator {
        model: sm.sessions
        delegate: QQC2.MenuItem {
            required property var modelData
            text: PlainText.menuText(modelData.name)
            icon.name: "document-multiple"
            onTriggered: sm.send(modelData.name)
        }
        onObjectAdded: (i, o) => sm.insertItem(i, o)
        onObjectRemoved: (i, o) => sm.removeItem(o)
    }
    QQC2.MenuSeparator { visible: sm.sessions.length > 0; height: visible ? implicitHeight : 0 }
    QQC2.MenuItem { text: "New Session…"; icon.name: "document-new"; onTriggered: sm.send("") }

    readonly property bool wanted: !!win && !win.privateMode
    property bool placed: false
    function place() {
        if (!parentMenu || wanted === placed) return
        if (wanted) {
            let at = parentMenu.count
            for (let i = 0; i < parentMenu.count; ++i) if (parentMenu.itemAt(i) === after) { at = i + 1; break }
            parentMenu.insertMenu(at, sm)
        } else {
            parentMenu.removeMenu(sm)
        }
        placed = wanted
    }
    onWantedChanged: place()
    Component.onCompleted: Qt.callLater(place)
}
