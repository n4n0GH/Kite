import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Customize Toolbar (like Firefox's): reorder the navigation toolbar's items,
// remove them, add ones from the catalog, toggle the menu bar and the
// bookmarks toolbar. Changes apply to the toolbar right away
// ("toolbar.items"). A window of its own: an in-window dialog has to fit the
// browser window, and its size and position then depend on each other.
QQC2.ApplicationWindow {
    id: dialog
    property var win
    property ToolbarItems catalog
    title: "Customize Toolbar — Kite"
    width: Kirigami.Units.gridUnit * 36
    height: Kirigami.Units.gridUnit * 32
    minimumWidth: Kirigami.Units.gridUnit * 26
    minimumHeight: Kirigami.Units.gridUnit * 20
    transientParent: win
    function open() { show(); raise(); requestActivate() }

    ListModel { id: current }
    onVisibleChanged: if (visible) load(catalog.sanitize(win.app.liveSettings["toolbar.items"]))
    function load(list) {
        current.clear()
        for (const id of list) current.append({itemId: id})
    }
    function save() {
        const list = []
        for (let i = 0; i < current.count; ++i) list.push(current.get(i).itemId)
        Browser.setSetting("toolbar.items", JSON.stringify(list))
    }
    function has(id) { for (let i = 0; i < current.count; ++i) if (current.get(i).itemId === id) return true; return false }
    function add(id) {
        // New items go before the menu button if it is last, like Firefox.
        const last = current.count - 1
        const at = last >= 0 && current.get(last).itemId === "menu" ? last : current.count
        current.insert(at, {itemId: id})
        save()
    }

    footer: QQC2.DialogButtonBox {
        QQC2.Button { text: "Restore Defaults"; icon.name: "edit-undo"; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ResetRole; onClicked: { dialog.load(dialog.catalog.defaults); dialog.save() } }
        QQC2.Button { text: "Done"; icon.name: "dialog-ok"; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole; onClicked: dialog.close() }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.largeSpacing
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: "Drag items to change their order on the toolbar (top is leftmost), remove them, or add more from the right."
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Kirigami.Units.largeSpacing
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Kirigami.Heading { level: 4; text: "Toolbar" }
                ListView {
                    id: currentList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: current
                    reuseItems: false
                    moveDisplaced: Transition { YAnimator { duration: Kirigami.Units.shortDuration } }
                    // Kirigami's reorder pattern: the drag handle moves the
                    // row inside a wrapper delegate (moving the delegate
                    // itself fights the ListView's own positioning).
                    delegate: Item {
                        id: slot
                        required property string itemId
                        required property int index
                        readonly property var info: dialog.catalog.info(itemId)
                        width: ListView.view.width
                        height: row.implicitHeight
                        QQC2.ItemDelegate {
                            id: row
                            width: slot.width
                            contentItem: RowLayout {
                                Kirigami.ListItemDragHandle {
                                    listItem: row
                                    listView: currentList
                                    onMoveRequested: (oldIndex, newIndex) => current.move(oldIndex, newIndex, 1)
                                    onDropped: dialog.save()
                                }
                                Kirigami.Icon { source: slot.info ? slot.info.icon : ""; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                                QQC2.Label { text: slot.info ? slot.info.name : slot.itemId; Layout.fillWidth: true; elide: Text.ElideRight }
                                QQC2.ToolButton {
                                    visible: !(slot.info && slot.info.required)
                                    icon.name: "list-remove"
                                    display: QQC2.AbstractButton.IconOnly
                                    text: "Remove from toolbar"
                                    onClicked: { current.remove(slot.index); dialog.save() }
                                    QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                                }
                            }
                        }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Kirigami.Heading { level: 4; text: "Available Items" }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    // Recomputed when the toolbar's items change.
                    model: { current.count; return dialog.catalog.catalog.filter(c => c.repeatable || !dialog.has(c.id)) }
                    delegate: QQC2.ItemDelegate {
                        required property var modelData
                        width: ListView.view.width
                        icon.name: modelData.icon
                        text: modelData.name
                        onClicked: dialog.add(modelData.id)
                        QQC2.ToolTip.text: "Add to toolbar"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
            }
        }
        QQC2.CheckBox { text: "Show menu bar (or tap Alt to show it briefly)"; checked: !!dialog.win.app.liveSettings["appearance.showMenuBar"]; onToggled: Browser.setSetting("appearance.showMenuBar", JSON.stringify(checked)) }
        QQC2.CheckBox { text: "Show bookmarks toolbar"; checked: dialog.win.showBookmarksBar; onToggled: Browser.setSetting("appearance.showBookmarksToolbar", JSON.stringify(checked)) }
    }
}
