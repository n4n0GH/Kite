import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtWebEngine
import org.kde.kirigami as Kirigami

// Find in page (Falkon's SearchToolBar).
QQC2.ToolBar {
    id: bar
    property var tab
    property int matches: 0
    property int activeMatch: 0
    visible: false
    position: QQC2.ToolBar.Footer

    function open() { visible = true; field.forceActiveFocus(); field.selectAll(); if (field.text) find(false) }
    function close() { visible = false; returnToPage = false; if (tab) { tab.view.findText(""); tab.focusPage() } }
    // For keys on the page (the Vim keys plugin): Enter goes back to the page, with
    // the bar still there, and next and previous leave the keyboard where it is.
    property bool returnToPage: false
    function openReturningToPage() { returnToPage = true; open() }
    function step(backward) {
        if (visible) find(backward)
        else if (field.text) { visible = true; find(backward) }
        else openReturningToPage()
    }
    function find(backward) {
        if (!tab) return
        if (!visible) { open(); return }
        let flags = 0
        if (caseBox.checked) flags |= WebEngineView.FindCaseSensitively
        if (backward) flags |= WebEngineView.FindBackward
        tab.view.findText(field.text, flags)
    }
    onTabChanged: if (visible) find(false)

    RowLayout {
        anchors.fill: parent
        QQC2.ToolButton { icon.name: "window-close"; onClicked: bar.close() }
        QQC2.TextField {
            id: field
            Layout.preferredWidth: Kirigami.Units.gridUnit * 16
            placeholderText: "Find in page"
            onTextChanged: bar.find(false)
            Keys.onReturnPressed: event => {
                bar.find(event.modifiers & Qt.ShiftModifier)
                if (bar.returnToPage && bar.tab) bar.tab.focusPage()
            }
            Keys.onEscapePressed: bar.close()
            color: text !== "" && bar.matches === 0 ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
        }
        QQC2.ToolButton { icon.name: "go-up"; onClicked: bar.find(true); QQC2.ToolTip.text: "Previous"; QQC2.ToolTip.visible: hovered }
        QQC2.ToolButton { icon.name: "go-down"; onClicked: bar.find(false); QQC2.ToolTip.text: "Next"; QQC2.ToolTip.visible: hovered }
        QQC2.CheckBox { id: caseBox; text: "Match case"; onToggled: bar.find(false) }
        QQC2.Label { text: field.text === "" ? "" : bar.matches === 0 ? "Phrase not found" : bar.activeMatch + " of " + bar.matches + " matches" }
        Item { Layout.fillWidth: true }
    }
}
