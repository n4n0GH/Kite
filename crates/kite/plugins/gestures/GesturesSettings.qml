import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite
import "strokes.js" as Strokes

// Settings → Plugins & Extensions → Mouse Gestures.
ColumnLayout {
    id: root
    property var app
    property var plugin

    spacing: Kirigami.Units.largeSpacing

    property var map: app.pluginSetting(plugin, "map")
    // The rows: only rebuilt when gestures come or go, not while a row's own
    // dropdown is telling us its choice.
    property var strokes: Strokes.sorted(map)
    function setMap(next) {
        map = next
        const list = Strokes.sorted(next)
        if (list.join() !== strokes.join()) strokes = list
        app.setPluginSetting(plugin, "map", next)
    }
    readonly property var actions: app.shortcutActions.filter(a => !/^tab(\d|Last)/.test(a.id)).map(a => ({id: a.id, name: a.group + ": " + a.name}))
    function actionName(id) { const a = actions.find(x => x.id === id); return a ? a.name : id }

    SettingsSectionHeader { text: "Drawing" }
    SettingsForm {
        QQC2.ComboBox {
            Kirigami.FormData.label: "Button:"
            model: [{k: "right", t: "Right button"}, {k: "middle", t: "Middle button"}]
            textRole: "t"
            valueRole: "k"
            wheelEnabled: false
            Component.onCompleted: currentIndex = indexOfValue(root.app.pluginSetting(root.plugin, "button"))
            onActivated: root.app.setPluginSetting(root.plugin, "button", currentValue)
        }
        QQC2.CheckBox {
            Kirigami.FormData.label: "Trail:"
            text: "Show the stroke while drawing"
            checked: !!root.app.pluginSetting(root.plugin, "trail")
            onToggled: root.app.setPluginSetting(root.plugin, "trail", checked)
        }
    }

    SettingsSectionHeader { text: "Gestures" }
    QQC2.Label {
        visible: root.strokes.length === 0
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        opacity: 0.7
        text: "No gestures."
    }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 0
        Repeater {
            model: root.strokes
            delegate: SettingsListRow {
                id: row
                required property string modelData
                RowLayout {
                    QQC2.Label {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                        horizontalAlignment: Text.AlignRight
                        text: Strokes.arrows(row.modelData)
                    }
                    QQC2.ComboBox {
                        Layout.fillWidth: true
                        model: root.actions
                        textRole: "name"
                        valueRole: "id"
                        // Scrolling down this list must not change what it passes over.
                        wheelEnabled: false
                        currentIndex: root.actions.findIndex(a => a.id === root.map[row.modelData])
                        onActivated: root.setMap(Object.assign({}, root.map, {[row.modelData]: currentValue}))
                    }
                    SettingsRowButton {
                        icon.name: "edit-delete"
                        text: "Remove this gesture"
                        onClicked: {
                            const next = Object.assign({}, root.map)
                            delete next[row.modelData]
                            Qt.callLater(root.setMap, next)
                        }
                    }
                }
            }
        }
    }
    RowLayout {
        QQC2.Button {
            text: "Record Gesture…"
            icon.name: "media-record"
            onClicked: recorder.start()
        }
        QQC2.Button {
            text: "Reset Gestures…"
            icon.name: "edit-undo"
            onClicked: resetPrompt.open()
        }
    }

    Kirigami.PromptDialog {
        id: resetPrompt
        title: "Reset Gestures?"
        subtitle: "The default gestures come back. Those you recorded or changed are lost."
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onAccepted: root.setMap(root.plugin.settings.map)
    }

    Kirigami.Dialog {
        id: recorder
        title: "Record Gesture"
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.NoButton

        property var tracker: null
        property var points: []
        property string stroke: ""
        property bool tooShort: false
        readonly property bool usable: Strokes.assignable(stroke)
        readonly property string taken: usable && root.map[stroke] ? root.actionName(root.map[stroke]) : ""

        function clear() {
            tracker = null
            points = []
            stroke = ""
            tooShort = false
            pad.requestPaint()
        }
        function start() {
            clear()
            actionBox.currentIndex = -1
            open()
        }

        customFooterActions: [
            Kirigami.Action {
                text: "Reset Shape"
                icon.name: "edit-clear"
                enabled: recorder.points.length > 0
                onTriggered: recorder.clear()
            },
            Kirigami.Action {
                text: "Save"
                icon.name: "dialog-ok-apply"
                enabled: recorder.usable && actionBox.currentIndex >= 0
                onTriggered: {
                    root.setMap(Object.assign({}, root.map, {[recorder.stroke]: actionBox.currentValue}))
                    recorder.close()
                }
            },
            Kirigami.Action {
                text: "Cancel"
                icon.name: "dialog-cancel"
                onTriggered: recorder.close()
            }
        ]

        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Layout.preferredWidth: Kirigami.Units.gridUnit * 26

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 14
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Kirigami.Theme.textColor, 0.2)
                clip: true

                QQC2.Label {
                    anchors.centerIn: parent
                    visible: recorder.points.length === 0
                    opacity: 0.6
                    text: "Draw the gesture here, with any mouse button"
                }
                Canvas {
                    id: pad
                    anchors.fill: parent
                    onPaint: {
                        const c = getContext("2d")
                        c.clearRect(0, 0, width, height)
                        if (recorder.points.length < 2) return
                        c.strokeStyle = Kirigami.Theme.highlightColor
                        c.lineWidth = 3
                        c.lineCap = "round"
                        c.lineJoin = "round"
                        c.beginPath()
                        recorder.points.forEach((p, i) => i ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y))
                        c.stroke()
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                    cursorShape: Qt.CrossCursor
                    onPressed: mouse => {
                        recorder.tracker = Strokes.tracker(mouse.x, mouse.y)
                        recorder.points = [Qt.point(mouse.x, mouse.y)]
                        recorder.stroke = ""
                        recorder.tooShort = false
                    }
                    onPositionChanged: mouse => {
                        if (!recorder.tracker) return
                        recorder.points.push(Qt.point(mouse.x, mouse.y))
                        recorder.stroke = Strokes.track(recorder.tracker, mouse.x, mouse.y)
                        pad.requestPaint()
                    }
                    onReleased: {
                        recorder.tracker = null
                        recorder.tooShort = recorder.stroke === ""
                    }
                }
            }
            // Always this high: the dialog centres itself, and if it changed size
            // during a stroke the pad would move under the pointer.
            QQC2.Label {
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                verticalAlignment: Text.AlignTop
                wrapMode: Text.Wrap
                color: recorder.taken !== "" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
                text: recorder.tooShort ? "Too short to tell a direction. Draw it larger."
                    : recorder.stroke === "" ? ""
                    : !recorder.usable ? Strokes.arrows(recorder.stroke) + "\nToo many turns, try a simpler one."
                    : recorder.taken !== "" ? Strokes.arrows(recorder.stroke) + "\nAlready set to “" + recorder.taken + "”. Saving replaces that."
                    : Strokes.arrows(recorder.stroke)
            }
            RowLayout {
                QQC2.Label { text: "Action:" }
                QQC2.ComboBox {
                    id: actionBox
                    Layout.fillWidth: true
                    model: root.actions
                    textRole: "name"
                    valueRole: "id"
                    displayText: currentIndex < 0 ? "Choose an action…" : currentText
                }
            }
        }
    }
}
