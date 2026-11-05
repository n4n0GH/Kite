import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite
import "keys.js" as Rules

// Settings → Plugins → Vim Keys.
ColumnLayout {
    id: root
    property var app
    property var plugin

    spacing: Kirigami.Units.largeSpacing

    property var keys: Object.assign({}, plugin.settings.keys, app.pluginSetting(plugin, "keys"))
    property var exceptions: app.pluginSetting(plugin, "exceptions")
    readonly property var clashing: Rules.conflicts(keys)
    function setKey(id, value) {
        const next = Object.assign({}, keys)
        next[id] = value
        keys = next
        // Only what can all work reaches the pages.
        if (!Rules.conflicts(next).length) app.setPluginSetting(plugin, "keys", next)
    }

    property string hintColor: app.pluginSetting(plugin, "hintColor")
    function setHintColor(color) {
        hintColor = color
        app.setPluginSetting(plugin, "hintColor", color)
    }
    readonly property var look: Rules.hintTheme(hintColor)

    Kirigami.FormLayout {
        Layout.fillWidth: true
        QQC2.CheckBox {
            id: smooth
            Kirigami.FormData.label: "Scrolling:"
            text: "Scroll smoothly"
            checked: !!root.app.pluginSetting(root.plugin, "smoothScroll")
            onToggled: root.app.setPluginSetting(root.plugin, "smoothScroll", checked)
        }
        RowLayout {
            Kirigami.FormData.label: "Held key speed:"
            enabled: smooth.checked
            QQC2.Slider {
                id: speed
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                from: Rules.SCROLL_SPEEDS.least
                to: Rules.SCROLL_SPEEDS.most
                stepSize: Rules.SCROLL_SPEEDS.step
                snapMode: QQC2.Slider.SnapAlways
                wheelEnabled: false
                value: Rules.scrollSpeed(root.app.pluginSetting(root.plugin, "scrollSpeed"))
                onMoved: root.app.setPluginSetting(root.plugin, "scrollSpeed", value)
            }
            QQC2.Label { text: Math.round(speed.value) + " px/s" }
        }
        QQC2.TextField {
            Kirigami.FormData.label: "Hint letters:"
            text: root.app.pluginSetting(root.plugin, "hintChars")
            onEditingFinished: if (Rules.labels(2, text).length) root.app.setPluginSetting(root.plugin, "hintChars", text)
        }
        QQC2.Label {
            // (A set width: a form makes room for a text's whole length otherwise.)
            Layout.maximumWidth: Kirigami.Units.gridUnit * 26
            wrapMode: Text.Wrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: "The easiest to reach first. Links get labels made from these. Hold Shift on a label's last letter to open it in a new tab."
        }
        RowLayout {
            Kirigami.FormData.label: "Hint color:"
            // What the labels will look like.
            Rectangle {
                implicitWidth: sample.implicitWidth + Kirigami.Units.smallSpacing * 2
                implicitHeight: sample.implicitHeight + 2
                radius: 3
                border.color: root.look.outline
                gradient: Gradient {
                    GradientStop { position: 0; color: root.look.top }
                    GradientStop { position: 1; color: root.look.bottom }
                }
                QQC2.Label { id: sample; anchors.centerIn: parent; text: "SA"; font.bold: true; color: root.look.text }
            }
            QQC2.Button {
                text: "Choose…"
                icon.name: "color-picker"
                onClicked: { colorDialog.selectedColor = root.hintColor; colorDialog.open() }
            }
            QQC2.Button {
                text: "Default"
                icon.name: "edit-undo"
                enabled: root.hintColor !== Rules.DEFAULT_HINT_COLOR
                onClicked: root.setHintColor(Rules.DEFAULT_HINT_COLOR)
            }
        }
    }
    ColorDialog {
        id: colorDialog
        title: "Hint Color"
        onAccepted: root.setHintColor(selectedColor.toString().substring(0, 7))
    }

    Kirigami.Heading { level: 4; text: "Keys" }
    QQC2.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        opacity: 0.7
        font: Kirigami.Theme.smallFont
        text: "One key, or several in a row like gg; capitals count. Empty turns it off. On Ctrl or Alt, keys stay Kite's and the page's."
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: root.clashing.length > 0
        type: Kirigami.MessageType.Warning
        text: "Some keys clash, by being the same or by one starting another. Changes aren't used until that's solved."
    }
    GridLayout {
        columns: 2
        columnSpacing: Kirigami.Units.largeSpacing
        Repeater {
            model: Rules.COMMANDS.length * 2
            delegate: Item {
                required property int index
                readonly property var command: Rules.COMMANDS[Math.floor(index / 2)]
                readonly property bool isField: index % 2 === 1
                Layout.fillWidth: !isField
                implicitWidth: isField ? Kirigami.Units.gridUnit * 5 : label.implicitWidth
                implicitHeight: isField ? field.implicitHeight : label.implicitHeight
                QQC2.Label { id: label; visible: !parent.isField; anchors.verticalCenter: parent.verticalCenter; text: parent.command.name }
                QQC2.TextField {
                    id: field
                    visible: parent.isField
                    width: parent.width
                    text: root.keys[parent.command.id] || ""
                    color: root.clashing.includes(parent.command.id) ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                    onTextEdited: root.setKey(parent.command.id, text.trim())
                }
            }
        }
    }
    QQC2.Button {
        text: "Reset Keys"
        icon.name: "edit-undo"
        onClicked: {
            root.keys = Object.assign({}, root.plugin.settings.keys)
            root.app.setPluginSetting(root.plugin, "keys", root.keys)
        }
    }

    Kirigami.Heading { level: 4; text: "Sites Left Alone" }
    QQC2.Label {
        visible: root.exceptions.length === 0
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        opacity: 0.7
        text: "None. For sites with keys of their own: go to the site and choose “Vim keys on this site” in the command palette."
    }
    Repeater {
        model: root.exceptions
        delegate: RowLayout {
            id: siteRow
            required property string modelData
            QQC2.Label { Layout.fillWidth: true; text: siteRow.modelData; elide: Text.ElideRight }
            QQC2.ToolButton {
                icon.name: "edit-delete-remove"
                text: "Remove"
                display: QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.text: "Vim keys on this site again"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    const next = root.exceptions.filter(s => s !== siteRow.modelData)
                    Qt.callLater(() => { root.exceptions = next; root.app.setPluginSetting(root.plugin, "exceptions", next) })
                }
            }
        }
    }
}
