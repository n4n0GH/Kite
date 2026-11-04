import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// A submenu listing the containers ("privacy.containers"), e.g. "New
// Container Tab". It puts itself into `parentMenu` after the item `after`
// while container tabs are on, and takes itself out when they're turned off.
// Declare it as a property of the parent menu, not inside it (a Menu declared
// inside a Menu is always added as a submenu).
QQC2.Menu {
    id: cm
    property var app
    property QQC2.Menu parentMenu
    property var after
    signal chosen(string container)

    Instantiator {
        model: cm.app ? cm.app.containers : []
        delegate: QQC2.MenuItem {
            required property var modelData
            text: modelData.name
            icon.name: "tag"
            icon.color: modelData.color
            onTriggered: cm.chosen(modelData.id)
        }
        onObjectAdded: (i, o) => cm.insertItem(i, o)
        onObjectRemoved: (i, o) => cm.removeItem(o)
    }

    readonly property bool wanted: !!app && app.containersEnabled
    property bool placed: false
    function place() {
        if (!parentMenu || wanted === placed) return
        if (wanted) {
            let at = parentMenu.count
            for (let i = 0; i < parentMenu.count; ++i) if (parentMenu.itemAt(i) === after) { at = i + 1; break }
            parentMenu.insertMenu(at, cm)
        } else {
            parentMenu.removeMenu(cm)
        }
        placed = wanted
    }
    onWantedChanged: place()
    Component.onCompleted: Qt.callLater(place)
}
