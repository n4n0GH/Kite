import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// The tab list, searchable: the open tabs of all windows (normal ones or
// private ones, not mixed), this window's first. Typing narrows them down by
// title and address; arrow keys and Enter pick one.
QQC2.Popup {
    id: search
    property var win
    property var rows: []
    property int selected: 0

    width: Math.min(Kirigami.Units.gridUnit * 30, win.width - Kirigami.Units.gridUnit * 2)
    margins: Kirigami.Units.smallSpacing
    padding: Kirigami.Units.smallSpacing
    focus: true

    // Under `anchor` (the tab bar's button, which toggles it), or at the top
    // of the page.
    function toggle(anchor) {
        if (opened) { close(); return }
        parent = anchor || win.contentItem
        closePolicy = anchor ? QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
                             : QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutside
        x = anchor ? 0 : (win.contentItem.width - width) / 2
        y = anchor ? anchor.height : Kirigami.Units.gridUnit
        field.text = ""
        refresh()
        selected = currentRow()
        open()
    }
    onOpened: { field.forceActiveFocus(); list.positionViewAtIndex(selected, ListView.Contain) }
    onClosed: if (win.currentTab) win.currentTab.focusPage()

    function currentRow() {
        const i = rows.findIndex(r => r.current)
        return i >= 0 ? i : 0
    }

    function refresh() {
        const words = field.text.toLowerCase().split(/\s+/).filter(w => w !== "")
        const out = []
        const windows = [win].concat(win.app.windows.filter(w => w !== win && w.privateMode === win.privateMode))
        for (const w of windows) {
            for (let i = 0; i < w.tabs.count; ++i) {
                const r = w.tabs.get(i)
                const text = (r.title + " " + r.url).toLowerCase()
                if (!words.every(word => text.includes(word))) continue
                out.push({tabId: r.tabId, title: r.title, url: r.url, icon: r.icon, audible: r.audible, pinned: r.pinned,
                          otherWindow: w !== win, window: w, current: w === win && r.tabId === (win.currentTab ? win.currentTab.tabId : -1)})
            }
        }
        rows = out
        selected = Math.max(0, Math.min(selected, rows.length - 1))
    }

    function pick(row) {
        if (!row) return
        close()
        const t = row.window.tabById(row.tabId)
        if (t) t.activate()
    }

    function closeRow(row) {
        const t = row.window.tabById(row.tabId)
        if (t) t.close()
        refresh()
        field.forceActiveFocus()
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Kirigami.SearchField {
            id: field
            Layout.fillWidth: true
            placeholderText: "Search tabs"
            autoAccept: false
            onTextChanged: { search.refresh(); search.selected = 0 }
            onAccepted: search.pick(search.rows[search.selected])
            Keys.onPressed: event => {
                const n = search.rows.length
                const page = Math.max(1, Math.floor(list.height / (Kirigami.Units.gridUnit * 2)))
                const to = {[Qt.Key_Down]: search.selected + 1, [Qt.Key_Up]: search.selected - 1,
                            [Qt.Key_PageDown]: search.selected + page, [Qt.Key_PageUp]: search.selected - page}[event.key]
                if (event.key === Qt.Key_Escape) { search.close(); event.accepted = true }
                else if (to !== undefined && n > 0) { search.selected = Math.max(0, Math.min(n - 1, to)); list.positionViewAtIndex(search.selected, ListView.Contain); event.accepted = true }
            }
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 22)
            clip: true
            model: search.rows
            currentIndex: search.selected
            highlightFollowsCurrentItem: false
            QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
            delegate: QQC2.ItemDelegate {
                id: row
                required property var modelData
                required property int index
                width: ListView.view.width
                highlighted: index === search.selected
                onHoveredChanged: if (hovered) search.selected = index
                onClicked: search.pick(modelData)
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    TabIcon {
                        source: row.modelData.icon
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                        Layout.preferredHeight: Layout.preferredWidth
                    }
                    ColumnLayout {
                        spacing: 0
                        Layout.fillWidth: true
                        QQC2.Label { text: row.modelData.title; font.bold: row.modelData.current; elide: Text.ElideRight; Layout.fillWidth: true }
                        QQC2.Label { text: row.modelData.url; visible: text !== ""; opacity: 0.7; font: Kirigami.Theme.smallFont; elide: Text.ElideMiddle; Layout.fillWidth: true }
                    }
                    Kirigami.Icon { visible: row.modelData.audible; source: "audio-volume-high"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                    Kirigami.Icon { visible: row.modelData.pinned; source: "window-pin"; opacity: 0.7; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                    QQC2.Label { visible: row.modelData.otherWindow; text: "Other window"; opacity: 0.7; font: Kirigami.Theme.smallFont }
                    QQC2.ToolButton {
                        icon.name: "window-close-symbolic"
                        text: "Close Tab"
                        display: QQC2.AbstractButton.IconOnly
                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                        Layout.preferredHeight: Layout.preferredWidth
                        padding: 0
                        onClicked: search.closeRow(row.modelData)
                    }
                }
            }
        }
        QQC2.Label {
            visible: search.rows.length === 0
            text: "No tabs found"
            opacity: 0.7
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
        }
        Kirigami.Separator { Layout.fillWidth: true }
        QQC2.ItemDelegate {
            Layout.fillWidth: true
            text: "Reopen Closed Tab"
            icon.name: "edit-undo"
            onClicked: { search.close(); search.win.reopenClosedTab(0) }
        }
    }
}
