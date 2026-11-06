import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.purpose as Purpose

// KDE's list of ways to share. Loaded when needed, so that a system without Purpose
// only loses this.
Window {
    id: dialog
    property var inputData
    // error: "" when it's done, or what to tell the page.
    signal done(string error, string message)
    property bool answered: false
    function finish(error, message) {
        if (answered) return
        answered = true
        done(error, message)
        close()
    }
    title: "Share"
    flags: Qt.Dialog
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 24
    height: Kirigami.Units.gridUnit * 20
    color: Kirigami.Theme.backgroundColor
    onClosing: finish("AbortError", "")
    Shortcut { sequence: "Escape"; onActivated: dialog.finish("AbortError", "") }
    Purpose.AlternativesView {
        id: view
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        pluginType: "ShareUrl"
        inputData: dialog.inputData
        delegate: QQC2.ItemDelegate {
            required property int index
            required property var model
            width: ListView.view.width
            text: model.display
            icon.name: model.iconName || ""
            onClicked: view.createJob(index)
        }
        onFinished: (output, error, message) => dialog.finish(error === 0 ? "" : "AbortError", error === 0 ? "" : message)
    }
}
