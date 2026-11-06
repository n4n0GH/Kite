import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite
import "fuzzy.js" as Fuzzy

// A search field over a list to pick from, looking like KDE's own command bar
// (Ctrl+Alt+I in Dolphin, KCommandBar): at the top of the window, compact rows
// with what was typed picked out in them, and shortcuts as keys. For the tab
// search (TabSearch.qml) and the command palette (CommandPalette.qml).
//
// `source()` gives the rows: {title, prefix (dim, before it), icon, run(),
// keys (shown as keycaps), detail, badges (icons), checked (shows On/Off),
// current (where the list starts), first (stays on top while searching), extra
// (more to search in, not highlighted), weight (of its score, if not 1),
// close() (adds an ×, which keeps the bar open)}.
QQC2.Popup {
    id: bar
    property var win
    property var source: () => []
    property string placeholder: "Search"
    property string emptyText: "Nothing found"
    readonly property alias query: field.text
    property var rows: []
    property int selected: 0
    property real _closedAt: 0
    property var _picked: null
    property bool pickedWithShift: false

    parent: win.contentItem
    width: Math.min(win.width - Kirigami.Units.gridUnit * 2, Math.max(Kirigami.Units.gridUnit * 28, win.width / 2.4))
    x: (win.contentItem.width - width) / 2
    y: Kirigami.Units.smallSpacing
    padding: 1
    focus: true
    closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutside
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    background: Kirigami.ShadowedRectangle {
        color: Kirigami.Theme.backgroundColor
        radius: Kirigami.Units.cornerRadius
        border.width: 1
        border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
        shadow.size: Kirigami.Units.gridUnit
        shadow.color: Qt.rgba(0, 0, 0, 0.35)
        shadow.yOffset: 2
    }

    // Pressing the button that opened it closes it first: that isn't reopening.
    function toggle() {
        if (opened) { close(); return }
        pickedWithShift = false
        if (Date.now() - _closedAt < 250) return
        field.text = ""
        refresh()
        open()
    }
    onOpened: { field.forceActiveFocus(); list.positionViewAtIndex(selected, ListView.Contain) }
    // The page gets the keyboard back first: what was picked may want it elsewhere.
    onClosed: {
        _closedAt = Date.now()
        if (win.currentTab) win.currentTab.focusPage()
        const run = _picked
        _picked = null
        if (run) Qt.callLater(run)
    }

    function refresh() {
        const query = field.text
        const words = query.trim().toLowerCase().split(/\s+/).filter(w => w)
        const all = bar.source()
        if (!words.length) {
            rows = all.map(r => Object.assign({}, r, {label: labelOf(r), positions: []}))
            selected = Math.max(0, rows.findIndex(r => r.current))
            return
        }
        const out = []
        for (const r of all) {
            const label = labelOf(r)
            const m = Fuzzy.match(query, label)
            // (`extra`, like a tab's address, only by plain words: letters
            // scattered over a long address match nearly anything.)
            if (m) out.push(Object.assign({}, r, {label, positions: m.positions, score: m.score * (r.weight === undefined ? 1 : r.weight)}))
            else if (r.extra && words.every(w => (label + " " + r.extra).toLowerCase().includes(w))) out.push(Object.assign({}, r, {label, positions: [], score: -1}))
        }
        out.sort((a, b) => (b.first ? 1 : 0) - (a.first ? 1 : 0) || b.score - a.score)
        // Without the letters scattered over much longer texts, once there are
        // real matches.
        const best = out.reduce((top, r) => Math.max(top, r.score), 0)
        rows = out.filter(r => r.first || r.score < 0 || r.score >= best / 3)
        selected = 0
    }
    function labelOf(r) { return (r.prefix ? r.prefix + ": " : "") + r.title }
    function pick(i) {
        const r = rows[i]
        if (!r) return
        _picked = r.run
        close()
    }
    function closeRow(i) {
        const r = rows[i]
        if (r && r.close) r.close()
        const keep = selected
        refresh()
        selected = Math.max(0, Math.min(keep, rows.length - 1))
        field.forceActiveFocus()
    }
    function move(by) {
        const n = rows.length
        if (!n) return
        selected = (selected + by + n) % n
        list.positionViewAtIndex(selected, ListView.Contain)
    }

    contentItem: ColumnLayout {
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            Layout.leftMargin: Kirigami.Units.smallSpacing * 2
            spacing: Kirigami.Units.smallSpacing * 2
            Kirigami.Icon { source: "search"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
            QQC2.TextField {
                id: field
                Layout.fillWidth: true
                placeholderText: bar.placeholder
                background: Item {}
                leftPadding: 0
                onTextChanged: bar.refresh()
                onAccepted: bar.pick(bar.selected)
                Keys.onPressed: event => {
                    const page = Math.max(1, Math.floor(list.height / list.rowHeight))
                    if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ShiftModifier)) {
                        bar.pickedWithShift = true
                        bar.pick(bar.selected)
                    }
                    else if (event.key === Qt.Key_Escape) bar.close()
                    else if (event.key === Qt.Key_Down) bar.move(1)
                    else if (event.key === Qt.Key_Up) bar.move(-1)
                    else if (event.key === Qt.Key_PageDown) bar.move(Math.min(page, bar.rows.length - 1 - bar.selected))
                    else if (event.key === Qt.Key_PageUp) bar.move(-Math.min(page, bar.selected))
                    else return
                    event.accepted = true
                }
            }
            Kirigami.Icon {
                visible: field.text !== ""
                source: "edit-clear-locationbar-rtl"
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Layout.preferredWidth
                TapHandler { onTapped: { field.clear(); field.forceActiveFocus() } }
            }
        }
        Kirigami.Separator { Layout.fillWidth: true }
        ListView {
            id: list
            readonly property real rowHeight: Math.round(Kirigami.Units.gridUnit * 1.45)
            Layout.fillWidth: true
            Layout.margins: 2
            Layout.preferredHeight: Math.min(contentHeight, Math.max(rowHeight * 8, bar.win.height * 0.42))
            clip: true
            model: bar.rows
            boundsBehavior: Flickable.StopAtBounds
            QQC2.ScrollBar.vertical: QQC2.ScrollBar { id: scroll }
            // The selection follows the mouse only when it moves: rows changing
            // under a resting pointer (typing) must not choose what Enter does.
            HoverHandler {
                property point last
                onPointChanged: {
                    const at = point.scenePosition
                    if (at.x === last.x && at.y === last.y) return
                    const fresh = last.x === 0 && last.y === 0
                    last = at
                    if (fresh) return
                    const i = list.indexAt(point.position.x + list.contentX, point.position.y + list.contentY)
                    if (i >= 0) bar.selected = i
                }
            }
            delegate: Item {
                id: row
                required property var modelData
                required property int index
                readonly property bool isSelected: index === bar.selected
                width: ListView.view.width - (scroll.visible ? scroll.width : 0)
                height: list.rowHeight
                Rectangle {
                    anchors.fill: parent
                    radius: Kirigami.Units.cornerRadius
                    color: row.isSelected ? Qt.alpha(Kirigami.Theme.highlightColor, 0.3) : "transparent"
                }
                HoverHandler { id: rowHover }
                TapHandler { onTapped: bar.pick(row.index) }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Kirigami.Units.smallSpacing
                    anchors.rightMargin: Kirigami.Units.smallSpacing
                    spacing: Kirigami.Units.smallSpacing
                    TabIcon {
                        source: row.modelData.icon || ""
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                        Layout.preferredHeight: Layout.preferredWidth
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        textFormat: Text.StyledText
                        text: Fuzzy.markup(row.modelData.label, row.modelData.positions, Kirigami.Theme.linkColor.toString(), Kirigami.Theme.disabledTextColor.toString(), row.modelData.prefix ? row.modelData.prefix.length + 2 : 0)
                    }
                    QQC2.Label {
                        visible: !!row.modelData.detail
                        text: row.modelData.detail || ""
                        elide: Text.ElideMiddle
                        opacity: 0.6
                        Layout.maximumWidth: bar.width * 0.35
                    }
                    Repeater {
                        model: row.modelData.badges || []
                        Kirigami.Icon {
                            required property string modelData
                            source: modelData
                            opacity: 0.7
                            Layout.preferredWidth: Kirigami.Units.iconSizes.small
                            Layout.preferredHeight: Layout.preferredWidth
                        }
                    }
                    // A setting's state.
                    QQC2.Label {
                        visible: row.modelData.checked !== undefined
                        text: row.modelData.checked ? "On" : "Off"
                        color: row.modelData.checked ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.disabledTextColor
                    }
                    // A shortcut, key by key.
                    Repeater {
                        model: row.modelData.keys ? row.modelData.keys.length * 2 - 1 : 0
                        delegate: Item {
                            required property int index
                            readonly property bool isKey: index % 2 === 0
                            implicitWidth: capText.implicitWidth + (isKey ? Kirigami.Units.smallSpacing * 2 : 0)
                            implicitHeight: list.rowHeight - 4
                            Rectangle {
                                visible: parent.isKey
                                anchors.fill: parent
                                radius: Kirigami.Units.cornerRadius
                                color: Qt.alpha(Kirigami.Theme.textColor, 0.1)
                            }
                            QQC2.Label {
                                id: capText
                                anchors.centerIn: parent
                                text: parent.isKey ? row.modelData.keys[parent.index / 2] : "+"
                                font: Kirigami.Theme.smallFont
                            }
                        }
                    }
                    CloseButton {
                        visible: !!row.modelData.close
                        opacity: rowHover.hovered || row.isSelected ? 1 : 0
                        text: "Close Tab"
                        Layout.preferredWidth: implicitWidth
                        Layout.preferredHeight: implicitHeight
                        onClicked: bar.closeRow(row.index)
                    }
                }
            }
        }
        QQC2.Label {
            visible: bar.rows.length === 0 && text !== ""
            text: bar.emptyText
            opacity: 0.7
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
        }
    }
}
