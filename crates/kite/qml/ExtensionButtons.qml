import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Toolbar buttons of extensions' browser actions (with badge), plus the
// "Extensions" puzzle menu listing all add-ons.
RowLayout {
    id: root
    property var win
    property var actions: []
    spacing: 0

    function buttonFor(extId) {
        for (let i = 0; i < rep.count; ++i) if (rep.itemAt(i).modelData.ext === extId) return rep.itemAt(i)
        return puzzle
    }

    Repeater {
        id: rep
        model: root.actions
        delegate: QQC2.ToolButton {
            id: btn
            required property var modelData
            enabled: modelData.enabled !== false
            icon.source: modelData.icon || ""
            icon.name: modelData.icon ? "" : "preferences-plugin"
            icon.color: "transparent"
            display: QQC2.AbstractButton.IconOnly
            text: modelData.title
            QQC2.ToolTip.text: modelData.title
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: {
                const tabId = root.win.currentTab ? root.win.currentTab.tabId : -1
                if (modelData.popup) root.win.openPageActionPopup(modelData, btn)
                else Extensions.actionClicked(modelData.ext, "browserAction", tabId)
            }
            TapHandler { acceptedButtons: Qt.RightButton; onTapped: actionMenu.popup() }
            QQC2.Menu {
                id: actionMenu
                QQC2.MenuItem { text: "Manage Extension"; icon.name: "configure"; onTriggered: root.win.app.showAddons("installed") }
                QQC2.MenuItem {
                    text: "Options"; icon.name: "settings-configure"
                    enabled: Extensions.optionsUrl(btn.modelData.ext) !== ""
                    onTriggered: root.win.openTab(Extensions.optionsUrl(btn.modelData.ext), {background: false})
                }
                QQC2.MenuItem { text: "Remove Extension"; icon.name: "edit-delete"; onTriggered: root.win.app.showAddons("installed") }
            }
            Rectangle {
                visible: btn.modelData.badgeText !== undefined && btn.modelData.badgeText !== null && btn.modelData.badgeText !== ""
                anchors { right: parent.right; bottom: parent.bottom; rightMargin: 1; bottomMargin: 1 }
                height: badge.implicitHeight + 1
                width: Math.max(height, badge.implicitWidth + 4)
                radius: 3
                color: btn.modelData.badgeBg || "#d92626"
                QQC2.Label {
                    id: badge
                    anchors.centerIn: parent
                    text: btn.modelData.badgeText || ""
                    color: btn.modelData.badgeFg || "white"
                    font.pixelSize: Kirigami.Units.gridUnit * 0.55
                    font.bold: true
                }
            }
        }
    }

    QQC2.ToolButton {
        id: puzzle
        icon.name: "preferences-plugin"
        display: QQC2.AbstractButton.IconOnly
        text: "Extensions"
        QQC2.ToolTip.text: "Extensions"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        onClicked: { menu.exts = JSON.parse(Extensions.list()); menu.popup(puzzle, 0, puzzle.height) }
        QQC2.Menu {
            id: menu
            property var exts: []
            Instantiator {
                model: menu.exts.filter(e => !e.broken)
                delegate: QQC2.MenuItem {
                    required property var modelData
                    text: modelData.name
                    icon.source: modelData.icon || ""
                    icon.name: modelData.icon ? "" : "preferences-plugin"
                    icon.color: "transparent"
                    enabled: modelData.enabled
                    onTriggered: {
                        if (modelData.optionsUrl) root.win.openTab(modelData.optionsUrl, {background: false})
                        else root.win.app.showAddons("installed")
                    }
                }
                onObjectAdded: (i, o) => menu.insertItem(i, o)
                onObjectRemoved: (i, o) => menu.removeItem(o)
            }
            QQC2.MenuSeparator {}
            QQC2.MenuItem { text: "Find More Extensions…"; icon.name: "get-hot-new-stuff"; onTriggered: root.win.app.showAddons("browse") }
            QQC2.MenuItem { text: "Manage Extensions"; icon.name: "configure"; onTriggered: root.win.app.showAddons("installed") }
        }
    }
}
