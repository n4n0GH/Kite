import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Saving a session under a name, renaming one, and the list of them.
Item {
    id: dialogs
    property var win
    readonly property var app: win.app

    function save() { nameDialog.ask("", "Save Current Session", "Save") }
    function manage() { list.reload(); manager.open() }
    function describe(s) {
        const count = (n, one) => n + " " + one + (n === 1 ? "" : "s")
        return count(s.windows, "window") + ", " + count(s.tabs, "tab") + " · " + new Date(s.saved).toLocaleString(Qt.locale(), Locale.ShortFormat)
    }

    // A name for a new session, or a new name for one (`renaming`).
    Kirigami.PromptDialog {
        id: nameDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        property string renaming: ""
        property var taken: []
        // (Saving under a name in use replaces that session; renaming can't.)
        readonly property bool exists: taken.includes(name.text.trim().toLowerCase())
        function ask(from, heading, button) {
            renaming = from
            title = heading
            name.text = from
            taken = JSON.parse(Browser.namedSessions()).map(s => s.name.toLowerCase()).filter(n => n !== from.toLowerCase())
            open()
        }
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onOpened: { name.selectAll(); name.forceActiveFocus() }
        function finish() {
            const text = name.text.trim()
            if (text === "" || (renaming !== "" && exists)) return
            const error = renaming !== "" ? Browser.renameNamedSession(renaming, text) : dialogs.app.saveNamedSession(text)
            if (error) dialogs.win.toast(error, "dialog-error")
            else if (renaming === "") dialogs.win.toast("Saved session “" + text + "”", "document-save")
            close()
            list.reload()
        }
        onAccepted: finish()
        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.TextField {
                id: name
                Layout.fillWidth: true
                Layout.minimumWidth: Kirigami.Units.gridUnit * 18
                maximumLength: 80
                placeholderText: "Session name"
                onAccepted: nameDialog.finish()
            }
            QQC2.Label {
                visible: nameDialog.exists
                text: nameDialog.renaming !== "" ? "There is a session with this name." : "This replaces the session with this name."
                color: Kirigami.Theme.neutralTextColor
                font: Kirigami.Theme.smallFont
            }
        }
    }

    Kirigami.Dialog {
        id: manager
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        title: "Sessions"
        preferredWidth: Kirigami.Units.gridUnit * 34
        standardButtons: Kirigami.Dialog.Close
        customFooterActions: [
            Kirigami.Action { text: "Save Current Session…"; icon.name: "document-save"; onTriggered: dialogs.save() }
        ]
        ColumnLayout {
            spacing: 0
            QQC2.Label {
                visible: list.count === 0
                Layout.margins: Kirigami.Units.gridUnit
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "No saved sessions yet. A session keeps your open windows and tabs under a name, to come back to later."
            }
            ListView {
                id: list
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 18)
                clip: true
                function reload() { model = JSON.parse(Browser.namedSessions()) }
                delegate: Item {
                    id: row
                    required property var modelData
                    width: ListView.view.width
                    implicitHeight: rowLayout.implicitHeight + Kirigami.Units.smallSpacing * 2
                    component RowButton: QQC2.ToolButton {
                        display: QQC2.AbstractButton.IconOnly
                        QQC2.ToolTip.text: text
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    RowLayout {
                        id: rowLayout
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: Kirigami.Units.largeSpacing; rightMargin: Kirigami.Units.smallSpacing }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            QQC2.Label { text: row.modelData.name; font.weight: Font.DemiBold; elide: Text.ElideRight; Layout.fillWidth: true }
                            QQC2.Label { text: dialogs.describe(row.modelData); opacity: 0.7; font: Kirigami.Theme.smallFont; elide: Text.ElideRight; Layout.fillWidth: true }
                        }
                        QQC2.Button {
                            text: "Open"
                            icon.name: "document-open"
                            QQC2.ToolTip.text: "Open beside what is open now"
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            onClicked: { manager.close(); dialogs.app.openNamedSession(row.modelData.name, false) }
                        }
                        RowButton {
                            text: "Switch to it: close the windows open now"
                            icon.name: "exchange-positions"
                            onClicked: { manager.close(); dialogs.app.openNamedSession(row.modelData.name, true) }
                        }
                        RowButton {
                            text: "Update with the windows open now"
                            icon.name: "document-save"
                            onClicked: { confirm.ask(row.modelData.name, "update"); }
                        }
                        RowButton {
                            text: "Rename"
                            icon.name: "edit-rename"
                            onClicked: nameDialog.ask(row.modelData.name, "Rename Session", "Rename")
                        }
                        RowButton {
                            text: "Delete"
                            icon.name: "edit-delete"
                            onClicked: confirm.ask(row.modelData.name, "delete")
                        }
                    }
                }
            }
        }
    }

    // These lose what a session held: Enter or Esc never do it, only the button.
    QQC2.Dialog {
        id: confirm
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        property string session: ""
        property string action: ""
        function ask(name, what) { session = name; action = what; open() }
        title: action === "delete" ? "Delete Session" : "Update Session"
        onOpened: cancelButton.forceActiveFocus()
        contentItem: QQC2.Label {
            wrapMode: Text.Wrap
            text: (confirm.action === "delete" ? "Delete the session “" + confirm.session + "”?" : "Replace what the session “" + confirm.session + "” holds with the windows open now?") + " This can't be undone."
        }
        footer: QQC2.DialogButtonBox {
            QQC2.Button {
                text: confirm.action === "delete" ? "Delete Session" : "Update Session"
                icon.name: confirm.action === "delete" ? "edit-delete" : "document-save"
                onClicked: {
                    const error = confirm.action === "delete" ? Browser.deleteNamedSession(confirm.session) : dialogs.app.saveNamedSession(confirm.session)
                    if (error) dialogs.win.toast(error, "dialog-error")
                    confirm.close()
                    list.reload()
                }
                // (Not in the box's roles: it would close without doing this.)
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
            }
            QQC2.Button {
                id: cancelButton
                text: "Cancel"
                icon.name: "dialog-cancel"
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole
                onClicked: confirm.close()
            }
        }
    }
}
