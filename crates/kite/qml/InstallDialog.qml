import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami
import org.kde.kite

// "Add <extension>?" confirmation with permission warnings (Firefox-style).
Window {
    id: dlg
    property int request: -1
    property var info: ({})
    property bool answered: false
    width: Kirigami.Units.gridUnit * 26
    height: layout.implicitHeight + Kirigami.Units.gridUnit * 2
    title: info.update ? "Update Extension" : "Add Extension"
    flags: Qt.Dialog
    modality: Qt.ApplicationModal
    color: Kirigami.Theme.backgroundColor

    function show(req, i) { request = req; info = i; answered = false; visible = true; raise(); requestActivate() }
    function answer(ok) { answered = true; Extensions.confirmInstall(request, ok); visible = false }
    onClosing: if (!answered) Extensions.confirmInstall(request, false)

    ColumnLayout {
        id: layout
        anchors { fill: parent; margins: Kirigami.Units.gridUnit }
        spacing: Kirigami.Units.largeSpacing
        RowLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon { source: "kite-extension"; Layout.preferredWidth: Kirigami.Units.iconSizes.large; Layout.preferredHeight: Layout.preferredWidth }
            Kirigami.Heading {
                level: 3
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: dlg.info.update ? ("Update " + dlg.info.name + " to " + dlg.info.version + "?") : ("Add " + dlg.info.name + "?")
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: (dlg.info.warnings || []).length > 0
            text: "It requires your permission to:"
        }
        Repeater {
            model: dlg.info.warnings || []
            delegate: RowLayout {
                required property string modelData
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Kirigami.Icon { source: "emblem-important"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                QQC2.Label { text: modelData; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: "Version " + dlg.info.version + (dlg.info.previousVersion ? " (currently " + dlg.info.previousVersion + ")" : "") + " · ID " + dlg.info.id
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            QQC2.Button { text: "Cancel"; icon.name: "dialog-cancel"; onClicked: dlg.answer(false) }
            QQC2.Button { text: dlg.info.update ? "Update" : "Add"; icon.name: "dialog-ok"; highlighted: true; onClicked: dlg.answer(true) }
        }
    }
}
