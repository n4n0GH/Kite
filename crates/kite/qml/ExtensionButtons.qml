import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Extensions on the toolbar (like Firefox's unified extensions button):
// buttons of the extensions pinned to the toolbar (browser actions, with
// badge), and the "Extensions" puzzle button opening a panel with all
// add-ons. Unpinned extensions ("extensions.unpinned") live in that panel;
// their action runs from there, and their popup opens under the puzzle
// button, placed like a toolbar button's.
RowLayout {
    id: root
    property var win
    property var actions: []
    spacing: 0

    readonly property var unpinned: win.app.liveSettings["extensions.unpinned"] || []
    function isPinned(extId) { return !unpinned.includes(extId) }
    function setPinned(extId, pinned) {
        const list = unpinned.filter(e => e !== extId)
        if (!pinned) list.push(extId)
        Browser.setSetting("extensions.unpinned", JSON.stringify(list))
    }
    function actionFor(extId) { return actions.find(a => a.ext === extId) || null }

    // Where an extension's popup opens: its toolbar button, or the puzzle
    // button for extensions kept in the panel.
    function buttonFor(extId) {
        for (let i = 0; i < rep.count; ++i) if (rep.itemAt(i).modelData.ext === extId) return rep.itemAt(i)
        return puzzle
    }
    // Clicking an extension's button (or its row in the panel).
    function trigger(action, anchor) {
        const tabId = win.currentTab ? win.currentTab.tabId : -1
        if (action.popup) win.openPageActionPopup(action, anchor)
        else Extensions.actionClicked(action.ext, "browserAction", tabId)
    }

    Repeater {
        id: rep
        model: root.actions.filter(a => root.isPinned(a.ext))
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
            onClicked: root.trigger(modelData, btn)
            TapHandler { acceptedButtons: Qt.RightButton; onTapped: { panel.close(); root.openMenu(btn.modelData.ext, true, true, btn, 0, btn.height) } }
            Badge { action: btn.modelData }
        }
    }

    QQC2.ToolButton {
        id: puzzle
        icon.name: "preferences-plugin"
        display: QQC2.AbstractButton.IconOnly
        text: "Extensions"
        checkable: true
        checked: panel.opened
        QQC2.ToolTip.text: "Extensions"; QQC2.ToolTip.visible: hovered && !panel.opened; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        onClicked: {
            extMenu.close()
            // An extension's popup opened from the panel sits on this button:
            // a click swaps it for the panel.
            if (root.win.closeExtensionPopupAt(puzzle)) panel.open()
            else panel.opened ? panel.close() : panel.open()
        }
    }

    // ---- the extensions panel --------------------------------------------
    QQC2.Popup {
        id: panel
        parent: puzzle
        // Right-aligned with the puzzle button, like extension popups.
        x: puzzle.width - width
        y: puzzle.height
        width: Kirigami.Units.gridUnit * 20
        padding: Kirigami.Units.smallSpacing
        // Presses on the puzzle button toggle the panel (its own click).
        closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
        property var exts: []
        onClosed: extMenu.close()
        onAboutToShow: exts = JSON.parse(Extensions.list()).filter(e => !e.broken)

        contentItem: ColumnLayout {
            spacing: 0
            Kirigami.Heading { level: 4; text: "Extensions"; Layout.margins: Kirigami.Units.smallSpacing }
            QQC2.Label {
                visible: panel.exts.length === 0
                text: "No extensions installed"
                opacity: 0.7
                Layout.margins: Kirigami.Units.smallSpacing
            }
            ListView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 20)
                clip: true
                model: panel.exts
                delegate: QQC2.ItemDelegate {
                    id: row
                    required property var modelData
                    readonly property var act: root.actionFor(modelData.id)
                    readonly property bool pinned: root.isPinned(modelData.id)
                    width: ListView.view.width
                    enabled: modelData.enabled
                    onClicked: {
                        panel.close()
                        if (act) root.trigger(act, root.buttonFor(modelData.id))
                        else if (modelData.optionsUrl) root.win.openTab(modelData.optionsUrl, {background: false})
                        else root.win.app.showAddons("installed")
                    }
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Item {
                            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                            Layout.preferredHeight: Layout.preferredWidth
                            Kirigami.Icon {
                                anchors.fill: parent
                                source: row.act && row.act.icon ? row.act.icon : row.modelData.icon || "preferences-plugin"
                            }
                            Badge { action: row.act }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            QQC2.Label { text: row.modelData.name; Layout.fillWidth: true; elide: Text.ElideRight }
                            QQC2.Label {
                                visible: text !== ""
                                text: row.act && row.act.title && row.act.title !== row.modelData.name ? row.act.title : ""
                                font: Kirigami.Theme.smallFont
                                opacity: 0.7
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }
                        }
                        QQC2.ToolButton {
                            visible: !!row.act
                            icon.name: row.pinned ? "window-unpin" : "window-pin"
                            display: QQC2.AbstractButton.IconOnly
                            text: row.pinned ? "Unpin from Toolbar" : "Pin to Toolbar"
                            onClicked: root.setPinned(row.modelData.id, !row.pinned)
                            QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        QQC2.ToolButton {
                            icon.name: "overflow-menu"
                            display: QQC2.AbstractButton.IconOnly
                            text: "More"
                            onClicked: root.openMenu(row.modelData.id, row.pinned, !!row.act, this, 0, height)
                        }
                    }
                }
            }
            Kirigami.Separator { Layout.fillWidth: true; Layout.topMargin: Kirigami.Units.smallSpacing; Layout.bottomMargin: Kirigami.Units.smallSpacing }
            QQC2.ItemDelegate { Layout.fillWidth: true; icon.name: "get-hot-new-stuff"; text: "Find More Extensions…"; onClicked: { panel.close(); root.win.app.showAddons("browse") } }
            QQC2.ItemDelegate { Layout.fillWidth: true; icon.name: "configure"; text: "Manage Extensions"; onClicked: { panel.close(); root.win.app.showAddons("installed") } }
        }
    }

    // An extension's context menu (toolbar button, or "⋯" in the panel):
    // one shared, modal menu, so only one is ever open and a click elsewhere
    // just closes it (like a native context menu).
    // Always given the item it opens at: popup() reparents the menu, and a
    // parent that went away (a panel row) would leave it unusable.
    function openMenu(extId, pinned, canPin, item, x, y) {
        extMenu.close()
        extMenu.extId = extId
        extMenu.pinned = pinned
        extMenu.canPin = canPin
        extMenu.popup(item, x, y)
    }
    QQC2.Menu {
        id: extMenu
        property string extId
        property bool pinned
        property bool canPin: true
        modal: true
        dim: false
        QQC2.MenuItem {
            visible: extMenu.canPin
            height: visible ? implicitHeight : 0
            text: extMenu.pinned ? "Unpin from Toolbar" : "Pin to Toolbar"
            icon.name: extMenu.pinned ? "window-unpin" : "window-pin"
            onTriggered: root.setPinned(extMenu.extId, !extMenu.pinned)
        }
        QQC2.MenuItem { text: "Manage Extension"; icon.name: "configure"; onTriggered: root.win.app.showAddons("installed") }
        QQC2.MenuItem {
            text: "Options"; icon.name: "settings-configure"
            enabled: Extensions.optionsUrl(extMenu.extId) !== ""
            onTriggered: root.win.openTab(Extensions.optionsUrl(extMenu.extId), {background: false})
        }
        QQC2.MenuItem { text: "Remove Extension"; icon.name: "edit-delete"; onTriggered: root.win.app.showAddons("installed") }
    }

    // A browser action's badge (e.g. uBlock Origin's blocked-request count).
    component Badge: Rectangle {
        property var action
        visible: !!action && action.badgeText !== undefined && action.badgeText !== null && action.badgeText !== ""
        anchors { right: parent.right; bottom: parent.bottom; rightMargin: 1; bottomMargin: 1 }
        height: badgeLabel.implicitHeight + 1
        width: Math.max(height, badgeLabel.implicitWidth + 4)
        radius: 3
        color: action && action.badgeBg || "#d92626"
        QQC2.Label {
            id: badgeLabel
            anchors.centerIn: parent
            text: parent.action && parent.action.badgeText || ""
            color: parent.action && parent.action.badgeFg || "white"
            font.pixelSize: Kirigami.Units.gridUnit * 0.55
            font.bold: true
        }
    }
}
