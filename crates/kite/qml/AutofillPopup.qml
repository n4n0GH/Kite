import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Suggestions under a focused form field: saved addresses, or a generated
// password for sign-up forms. The page keeps keyboard focus; its bridge
// world relays the arrow keys, Enter and Escape while this is open.
QQC2.Popup {
    id: pop
    required property var tab
    required property Item view
    required property QtObject bridge
    property var field: null
    property var items: []
    property int current: -1
    // The generated password stays the same while the field keeps focus.
    property string password: ""

    signal pickedAddress(int id)
    signal pickedPassword(string password)
    signal manage()

    parent: view
    focus: false
    modal: false
    dim: false
    closePolicy: QQC2.Popup.NoAutoClose
    padding: 0
    readonly property real zoom: view.zoomFactor || 1
    readonly property real below: field ? (field.y + field.h) * zoom + 2 : 0
    readonly property bool flip: field && below + implicitHeight > view.height && field.y * zoom - implicitHeight - 2 > 0
    x: field ? Math.max(0, Math.min(field.x * zoom, view.width - width)) : 0
    y: field ? (flip ? field.y * zoom - implicitHeight - 2 : below) : 0
    width: Math.min(view.width, Math.max(field ? field.w * zoom : 0, Kirigami.Units.gridUnit * 18))

    onClosed: { current = -1; bridge.autofillMenu(false) }
    onOpened: bridge.autofillMenu(true)

    Connections {
        target: pop.view
        function onVisibleChanged() { if (!pop.view.visible) pop.close() }
        function onUrlChanged() { pop.close() }
    }

    function show(f) {
        if (!f || (field && f.kind !== field.kind) || !visible) password = ""
        field = f
        let list = []
        if (f.kind === "new-password") {
            if (!tab.app.setting("passwords.generator") || f.value !== "") { close(); return }
            if (password === "") password = FormFill.generatePassword()
            list = [{generate: true}]
        } else {
            if (!FormFill.enabled()) { close(); return }
            if (!FormFill.unlocked) { if (FormFill.count > 0) FormFill.unlock(); close(); return }
            list = JSON.parse(FormFill.suggestions(view.url.toString(), f.kind, f.value))
            // Nothing new to offer once the field holds a suggestion.
            if (list.length === 1 && list[0].label.toLowerCase() === f.value.trim().toLowerCase()) list = []
        }
        if (list.length === 0) { close(); return }
        if (JSON.stringify(list) !== JSON.stringify(items)) current = -1
        items = list
        if (!visible) open()
    }

    function key(k) {
        const n = items.length
        if (k === "Escape") close()
        else if (k === "ArrowDown" || k === "PageDown") current = (current + 1) % n
        else if (k === "ArrowUp" || k === "PageUp") current = current <= 0 ? n - 1 : current - 1
        else if (k === "Enter") { if (current >= 0) pick(current); else close() }
    }

    function pick(i) {
        const it = items[i]
        close()
        if (!it) return
        if (it.generate) pickedPassword(password)
        else pickedAddress(it.id)
    }

    background: Kirigami.ShadowedRectangle {
        color: Kirigami.Theme.backgroundColor
        radius: Kirigami.Units.cornerRadius
        border.width: 1
        border.color: Qt.alpha(Kirigami.Theme.textColor, 0.2)
        shadow.size: Kirigami.Units.largeSpacing
        shadow.color: Qt.rgba(0, 0, 0, 0.25)
        shadow.yOffset: 2
    }

    contentItem: ColumnLayout {
        spacing: 0
        Repeater {
            model: pop.items
            delegate: QQC2.ItemDelegate {
                id: row
                required property var modelData
                required property int index
                Layout.fillWidth: true
                focusPolicy: Qt.NoFocus
                highlighted: index === pop.current
                hoverEnabled: true
                onHoveredChanged: if (hovered) pop.current = index
                onClicked: pop.pick(index)
                contentItem: RowLayout {
                    spacing: Kirigami.Units.mediumSpacing
                    Kirigami.Icon {
                        source: row.modelData.generate ? "password-generate" : row.modelData.detail !== undefined && pop.field && pop.field.kind === "email" ? "mail-message" : "user-identity"
                        fallback: "dialog-password"
                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                        Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                    }
                    ColumnLayout {
                        spacing: 0
                        Layout.fillWidth: true
                        QQC2.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            text: row.modelData.generate ? "Use a strong password" : row.modelData.label
                            font.weight: Font.DemiBold
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            visible: text !== ""
                            text: row.modelData.generate ? pop.password : row.modelData.detail
                            font.family: row.modelData.generate ? "monospace" : Kirigami.Theme.defaultFont.family
                            opacity: 0.7
                        }
                    }
                }
            }
        }
        Kirigami.Separator { Layout.fillWidth: true }
        QQC2.ItemDelegate {
            Layout.fillWidth: true
            focusPolicy: Qt.NoFocus
            text: pop.field && pop.field.kind === "new-password" ? "Kite offers to save it once you sign up" : "Manage Addresses…"
            enabled: !(pop.field && pop.field.kind === "new-password")
            icon.name: pop.field && pop.field.kind === "new-password" ? "dialog-information" : "configure"
            font: Kirigami.Theme.smallFont
            onClicked: { pop.close(); pop.manage() }
        }
    }
}
