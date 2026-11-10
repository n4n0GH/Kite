import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite
import "keys.js" as Rules

// Settings → Plugins & Extensions → Vim Keys.
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

    // The site in the browser window last used: what "Add Current Site" leaves alone.
    readonly property var browserTab: app.activeWindow ? app.activeWindow.currentTab : null
    readonly property string currentSite: browserTab ? Rules.siteOf(browserTab.url) : ""
    function setExceptions(next) {
        Qt.callLater(() => { root.exceptions = next; root.app.setPluginSetting(root.plugin, "exceptions", next) })
    }

    SettingsSectionHeader { text: "Scrolling and Link Hints" }
    SettingsForm {
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
            Layout.maximumWidth: Kirigami.Units.gridUnit * 12
            text: root.app.pluginSetting(root.plugin, "hintChars")
            onEditingFinished: if (Rules.labels(2, text).length) root.app.setPluginSetting(root.plugin, "hintChars", text)
        }
        SettingsHint { text: "The easiest to reach first. Links get labels made from these. Hold Shift on a label's last letter to open it in a new tab." }
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

    SettingsSectionHeader { text: "Sites Left Alone" }
    SettingsHint {
        Layout.maximumWidth: -1
        text: "Sites with keys of their own, and the sites under them, get no vim keys. Also from the command palette: “Vim keys on this site”."
    }
    QQC2.Label {
        visible: root.exceptions.length === 0
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        opacity: 0.7
        text: "No sites are left alone."
    }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 0
        Repeater {
            model: root.exceptions
            delegate: SettingsListRow {
                id: siteRow
                required property string modelData
                RowLayout {
                    Kirigami.Icon { source: "internet-web-browser"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                    QQC2.Label { Layout.fillWidth: true; text: siteRow.modelData; elide: Text.ElideRight }
                    SettingsRowButton {
                        icon.name: "edit-delete"
                        text: "Remove (vim keys on this site again)"
                        onClicked: root.setExceptions(root.exceptions.filter(s => s !== siteRow.modelData))
                    }
                }
            }
        }
    }
    QQC2.Button {
        readonly property bool covered: root.currentSite !== "" && Rules.isExcepted(root.currentSite, root.exceptions)
        text: "Add Current Site"
        icon.name: "list-add"
        enabled: root.currentSite !== "" && !covered
        QQC2.ToolTip.text: root.currentSite === "" ? "The current tab isn't showing a website"
                         : covered ? root.currentSite + " is left alone already"
                         : "Leave " + root.currentSite + " alone"
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        onClicked: root.setExceptions(root.exceptions.concat([root.currentSite]))
    }

    // ---- keys: as Settings → Shortcuts lists Kite's own ----
    readonly property var defaults: plugin.settings.keys
    readonly property var groups: Rules.COMMANDS.reduce((g, c) => g.includes(c.group) ? g : g.concat([c.group]), [])
    property string filter: ""
    function matches(c) {
        const f = filter.trim().toLowerCase()
        return f === "" || c.name.toLowerCase().includes(f) || c.group.toLowerCase().includes(f) || (keys[c.id] || "").toLowerCase().includes(f)
    }
    readonly property bool anyChanged: Rules.COMMANDS.some(c => (keys[c.id] || "") !== (defaults[c.id] || ""))

    // (No heading of its own: its groups have theirs, as on the Shortcuts page.)
    RowLayout {
        Layout.fillWidth: true
        Layout.topMargin: Kirigami.Units.largeSpacing
        Kirigami.SearchField { Layout.fillWidth: true; placeholderText: "Search actions or keys…"; onTextChanged: root.filter = text }
        QQC2.Button {
            text: "Reset All"
            icon.name: "edit-undo"
            enabled: root.anyChanged
            onClicked: resetAllDlg.open()
        }
    }
    SettingsHint {
        Layout.maximumWidth: -1
        text: "Click a key to change it. One key, or several in a row like gg; capitals count. On Ctrl or Alt, keys stay Kite's and the page's."
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: root.clashing.length > 0
        type: Kirigami.MessageType.Warning
        text: "Some keys clash, by being the same or by one starting another. Changes aren't used until that's solved."
    }
    Repeater {
        model: root.groups
        delegate: ColumnLayout {
            id: grp
            required property string modelData
            readonly property var items: Rules.COMMANDS.filter(c => c.group === grp.modelData && root.matches(c))
            Layout.fillWidth: true
            visible: items.length > 0
            spacing: 0
            SettingsSectionHeader { text: grp.modelData }
            Repeater {
                model: grp.items
                delegate: SettingsListRow {
                    id: row
                    required property var modelData
                    readonly property string key: root.keys[modelData.id] || ""
                    readonly property string original: root.defaults[modelData.id] || ""
                    readonly property bool changed: key !== original
                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        QQC2.Label { text: row.modelData.name; Layout.fillWidth: true; elide: Text.ElideRight; font.italic: row.changed }
                        QQC2.Button {
                            text: row.key !== "" ? row.key : "None"
                            font.family: row.key !== "" ? "monospace" : Kirigami.Theme.defaultFont.family
                            opacity: row.key !== "" ? 1 : 0.6
                            palette.buttonText: root.clashing.includes(row.modelData.id) ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                            onClicked: recorder.record(row.modelData)
                            QQC2.ToolTip.text: "Change, or turn off with the button in the dialog"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        SettingsRowButton {
                            icon.name: "edit-undo"
                            // (Kept in its place when there's nothing to reset, so the keys line up.)
                            enabled: row.changed
                            opacity: row.changed ? 1 : 0
                            text: "Reset to " + (row.original !== "" ? row.original : "none")
                            onClicked: root.setKey(row.modelData.id, row.original)
                        }
                    }
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: resetAllDlg
        parent: QQC2.Overlay.overlay; anchors.centerIn: parent
        title: "Reset All Keys?"
        subtitle: "Every action gets its default key back."
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onAccepted: {
            root.keys = Object.assign({}, root.defaults)
            root.app.setPluginSetting(root.plugin, "keys", root.keys)
        }
    }

    // Records the keys for a command, typed one after the other.
    Kirigami.Dialog {
        id: recorder
        parent: QQC2.Overlay.overlay; anchors.centerIn: parent
        property var command: null
        property string seq: ""
        readonly property var clash: command ? Rules.clash(root.keys, command.id, seq) : ({same: null, prefix: null})
        title: command ? command.name : ""
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.NoButton
        function record(c) { command = c; seq = ""; open(); grab.forceActiveFocus() }
        function assign() {
            if (clash.same) root.setKey(clash.same.id, "")
            root.setKey(command.id, seq)
            close()
        }
        onOpened: grab.forceActiveFocus()
        customFooterActions: [
            Kirigami.Action { text: "Turn Off"; icon.name: "edit-delete-remove"; visible: !!recorder.command && (root.keys[recorder.command.id] || "") !== ""; onTriggered: { root.setKey(recorder.command.id, ""); recorder.close() } },
            Kirigami.Action { text: recorder.clash.same ? "Reassign" : "Assign"; icon.name: "dialog-ok-apply"; enabled: recorder.seq !== "" && !recorder.clash.prefix; onTriggered: recorder.assign() },
            Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: recorder.close() }
        ]
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Layout.preferredWidth: Kirigami.Units.gridUnit * 22
            QQC2.Label { text: "Type the keys for this action:" }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.width: grab.activeFocus ? 2 : 1
                border.color: grab.activeFocus ? Kirigami.Theme.highlightColor : Qt.alpha(Kirigami.Theme.textColor, 0.2)
                QQC2.Label {
                    anchors.centerIn: parent
                    text: recorder.seq !== "" ? recorder.seq : "…"
                    font.family: "monospace"
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.4
                }
                Item {
                    id: grab
                    anchors.fill: parent
                    focus: true
                    // Take every key, also ones that are shortcuts elsewhere.
                    Keys.onShortcutOverride: event => event.accepted = true
                    Keys.onPressed: event => {
                        event.accepted = true
                        if (event.key === Qt.Key_Escape) { recorder.close(); return }
                        if (event.key === Qt.Key_Backspace) { recorder.seq = recorder.seq.slice(0, -1); return }
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { if (recorder.seq !== "" && !recorder.clash.prefix) recorder.assign(); return }
                        // Keys a page gets as text, without Ctrl or Alt (those stay Kite's and the page's).
                        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
                        const t = event.text
                        if (t.length !== 1 || !/\S/.test(t)) return
                        if (recorder.seq.length < 4) recorder.seq += t
                    }
                }
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: text !== ""
                color: recorder.clash.prefix ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.neutralTextColor
                text: recorder.clash.prefix ? "Clashes with “" + recorder.clash.prefix.name + "” (" + root.keys[recorder.clash.prefix.id] + "): one starts the other, so it would never be reached."
                    : recorder.clash.same ? "Already used for “" + recorder.clash.same.name + "”. Reassign moves it here."
                    : ""
            }
            SettingsHint {
                Layout.maximumWidth: -1
                text: "Backspace takes back a key, Enter assigns, Escape cancels. Default: " + (recorder.command && root.defaults[recorder.command.id] ? root.defaults[recorder.command.id] : "none") + "."
            }
        }
    }
}
