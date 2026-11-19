import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami
import org.kde.kite

// Asks for the parent password (parental controls), for one thing a child's profile
// can't do without it.
Window {
    id: dlg
    property string text: ""
    property string error: ""
    // What takes the password: gives an error to show, or "" if that was it.
    property var attempt: null
    property var done: null
    width: Kirigami.Units.gridUnit * 24
    height: layout.implicitHeight + Kirigami.Units.gridUnit * 2
    title: "Parent Password"
    flags: Qt.Dialog
    modality: Qt.ApplicationModal
    color: Kirigami.Theme.backgroundColor

    function ask(what, attemptWith, whenDone) {
        text = what; attempt = attemptWith; done = whenDone || null
        error = ""; field.text = ""
        visible = true; raise(); requestActivate(); field.forceActiveFocus()
    }
    function tryIt() {
        if (field.text === "" || !attempt) return
        error = attempt(field.text) || ""
        if (error !== "") { field.selectAll(); field.forceActiveFocus(); return }
        const then = done
        close()
        if (then) then()
    }
    onVisibleChanged: if (!visible) { field.text = ""; attempt = null; done = null }

    ColumnLayout {
        id: layout
        anchors { fill: parent; margins: Kirigami.Units.gridUnit }
        RowLayout {
            Kirigami.Icon { source: "dialog-password"; Layout.preferredWidth: Kirigami.Units.iconSizes.large; Layout.preferredHeight: Layout.preferredWidth; Layout.alignment: Qt.AlignTop }
            QQC2.Label { text: dlg.text; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
        Kirigami.PasswordField { id: field; Layout.fillWidth: true; placeholderText: "Parent password"; onAccepted: dlg.tryIt() }
        QQC2.Label { visible: dlg.error !== ""; text: dlg.error; color: Kirigami.Theme.negativeTextColor; wrapMode: Text.Wrap; Layout.fillWidth: true }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            QQC2.Button { text: "Cancel"; icon.name: "dialog-cancel"; onClicked: dlg.close() }
            QQC2.Button { text: "OK"; icon.name: "dialog-ok"; highlighted: true; enabled: field.text !== ""; onClicked: dlg.tryIt() }
        }
    }
}
