import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// "Block Element": a small panel over the page, to choose how much around the element goes,
// and adjust the rule. The rule goes into the user's own filters (Settings → Content Blocking).
Rectangle {
    id: panel
    property var tab
    property var candidates: []
    property string host: ""
    property string address: ""
    property string state_: "invalid"
    property int matches: 0

    visible: false
    width: Kirigami.Units.gridUnit * 22
    height: column.implicitHeight + Kirigami.Units.largeSpacing * 2
    color: Kirigami.Theme.backgroundColor
    border.color: Qt.darker(Kirigami.Theme.backgroundColor, 1.4)
    radius: Kirigami.Units.cornerRadius
    z: 20
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    Kirigami.Theme.inherit: false

    function page(code, then) { tab.view.runJavaScript(code, 1, then || (() => {})) }
    function ruleFor(index) {
        const c = candidates[index]
        return c ? host + "##" + c.selector : ""
    }
    // At a point in the view, as the context menu reports it.
    function start(x, y) {
        const zoom = tab.view.zoomFactor || 1
        page(Browser.elementPickerScript() + ";__kitePicker.start(" + (x / zoom) + "," + (y / zoom) + ")", json => {
            const r = JSON.parse(json || "{}")
            if (!r.candidates || !r.candidates.length) { tab.window.toast("Nothing to block there", "dialog-information"); finish(false); return }
            candidates = r.candidates
            host = r.host
            address = r.address
            nesting.to = candidates.length - 1
            nesting.value = 0
            preview.checked = false
            ruleField.text = ruleFor(0)
            test()
            // Out of the element's way.
            panel.x = r.right ? Kirigami.Units.largeSpacing : parent.width - width - Kirigami.Units.largeSpacing
            // (Bound: its height settles after this, with the rule's lines. Dragging ends that.)
            panel.y = Qt.binding(() => parent.height - height - Kirigami.Units.largeSpacing)
            visible = true
            takeKeys.restart()
        })
    }
    function test() {
        page("__kitePicker && __kitePicker.test(" + JSON.stringify(ruleField.text) + ")", json => {
            const r = JSON.parse(json || "{}")
            state_ = r.state || "invalid"
            matches = r.count || 0
        })
    }
    function finish(backToPage) {
        if (visible || backToPage) page("globalThis.__kitePicker && __kitePicker.stop(), 0")
        visible = false
        if (backToPage) tab.focusPage()
    }
    function block() {
        const rule = ruleField.text.trim()
        tab.app.addCustomFilter(rule, tab.window.privateMode ? "" : address)
        page("__kitePicker.block(" + JSON.stringify(rule) + ")")
        visible = false
        tab.focusPage()
        tab.window.toast("Blocked. Your rules are in Settings → Content Blocking.", "dialog-ok-apply")
    }

    // The page gets the keyboard back as the menu closes, a moment after this was asked for.
    Timer { id: takeKeys; interval: 120; onTriggered: nesting.forceActiveFocus() }
    Keys.onEscapePressed: finish(true)
    DragHandler { target: panel; yAxis.minimum: 0; yAxis.maximum: panel.parent ? panel.parent.height - panel.height : 0; xAxis.minimum: 0; xAxis.maximum: panel.parent ? panel.parent.width - panel.width : 0 }

    ColumnLayout {
        id: column
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Kirigami.Units.largeSpacing }
        spacing: Kirigami.Units.smallSpacing
        Keys.onEscapePressed: panel.finish(true)

        Kirigami.Heading { level: 4; text: "Block Element" }
        RowLayout {
            Layout.fillWidth: true
            QQC2.Label { text: "Just this"; opacity: 0.7; font: Kirigami.Theme.smallFont }
            QQC2.Slider {
                id: nesting
                Layout.fillWidth: true
                from: 0
                stepSize: 1
                snapMode: QQC2.Slider.SnapAlways
                enabled: panel.candidates.length > 1
                Accessible.name: "How much around the element to block"
                onMoved: { ruleField.text = panel.ruleFor(value); panel.test() }
                Keys.onEscapePressed: panel.finish(true)
            }
            QQC2.Label { text: "More around it"; opacity: 0.7; font: Kirigami.Theme.smallFont }
        }
        QQC2.TextArea {
            id: ruleField
            Layout.fillWidth: true
            font.family: "monospace"
            wrapMode: TextEdit.WrapAnywhere
            Accessible.name: "The rule"
            onTextChanged: if (activeFocus) testSoon.restart()
            Keys.onEscapePressed: panel.finish(true)
            Timer { id: testSoon; interval: 200; onTriggered: panel.test() }
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            opacity: 0.8
            text: panel.state_ === "invalid" ? "This isn't a rule Kite can read."
                : panel.state_ === "other" ? "Can't be tried here, but it's a rule."
                : panel.matches === 1 ? "Matches 1 element on this page."
                : "Matches " + panel.matches + " elements on this page."
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
            opacity: 0.8
            visible: panel.tab && panel.tab.window.privateMode
            text: "The rule stays after this private window closes, and names this site."
        }
        RowLayout {
            Layout.fillWidth: true
            QQC2.Button {
                id: preview
                text: "Preview"
                icon.name: "view-preview"
                checkable: true
                onToggled: panel.page("__kitePicker.preview(" + checked + ")")
            }
            Item { Layout.fillWidth: true }
            QQC2.Button { text: "Cancel"; icon.name: "dialog-cancel"; onClicked: panel.finish(true) }
            QQC2.Button {
                text: "Block"
                icon.name: "edit-delete-remove"
                enabled: panel.state_ !== "invalid"
                onClicked: panel.block()
            }
        }
    }
}
