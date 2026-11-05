import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's BookmarksToolbar: the "Bookmarks Toolbar" folder as buttons,
// sub-folders as menus.
QQC2.ToolBar {
    id: bar
    property var win
    property var items: []
    function reload() { items = JSON.parse(Browser.bookmarkTree(JSON.parse(Browser.bookmarkRoots()).toolbar)) }
    Component.onCompleted: reload()
    Connections { target: Browser; function onBookmarksChanged() { bar.reload() } }
    implicitHeight: visible ? flow.implicitHeight + Kirigami.Units.smallSpacing : 0

    function open(url, mods) {
        if (mods & Qt.ControlModifier) win.openTab(url, {background: true, afterCurrent: true})
        else if (mods & Qt.ShiftModifier) win.app.newWindow({urls: [url], privateMode: win.privateMode})
        else if (win.currentTab) win.currentTab.load(url)
    }

    RowLayout {
        id: flow
        anchors.fill: parent
        spacing: 0
        Repeater {
            model: bar.items
            delegate: Loader {
                required property var modelData
                sourceComponent: modelData.type === "separator" ? sepComp : modelData.type === "folder" ? folderComp : urlComp
                property var node: modelData
            }
        }
        QQC2.Label {
            visible: bar.items.length === 0
            text: "Bookmark pages into the “Bookmarks Toolbar” folder to see them here."
            opacity: 0.6
            font: Kirigami.Theme.smallFont
            Layout.leftMargin: Kirigami.Units.largeSpacing
        }
        Item { Layout.fillWidth: true }
        QQC2.ToolButton {
            icon.name: "bookmarks-organize"
            display: QQC2.AbstractButton.IconOnly
            text: "Show All Bookmarks"
            onClicked: bar.win.app.showLibrary("bookmarks")
            QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered
        }
    }

    Component { id: sepComp; Kirigami.Separator { height: Kirigami.Units.gridUnit; width: 1 } }
    Component {
        id: urlComp
        QQC2.ToolButton {
            text: parent.node.title || parent.node.url
            icon.name: "text-html"
            display: QQC2.AbstractButton.TextBesideIcon
            width: Math.min(implicitWidth, Kirigami.Units.gridUnit * 10)
            onClicked: bar.open(parent.node.url, 0)
            TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: bar.win.openTab(parent.parent.node.url, {background: true}) }
            QQC2.ToolTip.text: parent.node.title + "\n" + parent.node.url
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
    Component {
        id: folderComp
        QQC2.ToolButton {
            id: fb
            text: parent.node.title
            icon.name: "folder-bookmark"
            display: QQC2.AbstractButton.TextBesideIcon
            onClicked: folderMenu.popup(fb, 0, fb.height)
            BookmarkFolderMenu { id: folderMenu; folderId: fb.parent.node.id; bar: bar }
        }
    }
}
