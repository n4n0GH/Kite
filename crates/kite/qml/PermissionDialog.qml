import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

// permissions.request() prompt.
Window {
    id: dlg
    property var req: ({})
    property var callback: null
    width: Kirigami.Units.gridUnit * 24
    height: layout.implicitHeight + Kirigami.Units.gridUnit * 2
    title: "Extension Permission Request"
    flags: Qt.Dialog
    modality: Qt.ApplicationModal
    color: Kirigami.Theme.backgroundColor
    function ask(r, cb) { req = r; callback = cb; visible = true; raise(); requestActivate() }
    function answer(ok) { const cb = callback; callback = null; visible = false; if (cb) cb(ok) }
    onClosing: answer(false)
    ColumnLayout {
        id: layout
        anchors { fill: parent; margins: Kirigami.Units.gridUnit }
        Kirigami.Heading { level: 3; Layout.fillWidth: true; wrapMode: Text.Wrap; text: (dlg.req.name || "An extension") + " requests additional permissions." }
        QQC2.Label { text: "It wants to:" }
        Repeater {
            model: (dlg.req.warnings && dlg.req.warnings.length) ? dlg.req.warnings : (dlg.req.permissions || [])
            delegate: QQC2.Label { required property string modelData; text: "• " + modelData; Layout.fillWidth: true; wrapMode: Text.Wrap }
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            QQC2.Button { text: "Deny"; onClicked: dlg.answer(false) }
            QQC2.Button { text: "Allow"; highlighted: true; onClicked: dlg.answer(true) }
        }
    }
}
