import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kite

// The profiles: this one, others to open in a Kite of their own, and the picker
// to add or delete them. Ones already open can't be opened twice from here.
QQC2.Menu {
    id: menu
    title: "Profiles"
    icon.name: "user-identity"
    property var profiles: []
    readonly property string current: Browser.currentProfile()
    onAboutToShow: profiles = JSON.parse(Browser.profiles())

    Instantiator {
        model: menu.profiles
        delegate: QQC2.MenuItem {
            required property var modelData
            readonly property bool here: modelData.name === menu.current
            text: modelData.name + (modelData.inUse && !here ? " (open)" : "")
            checkable: here
            checked: here
            enabled: !here && !modelData.inUse
            onTriggered: Browser.openProfile(modelData.name)
        }
        onObjectAdded: (i, o) => menu.insertItem(i, o)
        onObjectRemoved: (i, o) => menu.removeItem(o)
    }
    QQC2.MenuSeparator {}
    QQC2.MenuItem { text: "Manage Profiles…"; icon.name: "configure"; onTriggered: Browser.manageProfiles() }
}
