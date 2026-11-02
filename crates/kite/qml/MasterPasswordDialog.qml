import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami
import org.kde.kite

// Unlocks the encrypted password store.
Window {
    id: dlg
    property var callbacks: []
    property string error: ""
    width: Kirigami.Units.gridUnit * 22
    height: layout.implicitHeight + Kirigami.Units.gridUnit * 2
    title: "Unlock Passwords"
    flags: Qt.Dialog
    modality: Qt.ApplicationModal
    color: Kirigami.Theme.backgroundColor

    function ask(cb) {
        if (!Passwords.locked) { cb(true); return }
        callbacks = callbacks.concat([cb])
        if (Passwords.backend === "kwallet") {
            // KWallet shows its own prompt if the wallet is closed.
            Passwords.openWallet()
            return
        }
        if (visible) return
        error = ""; field.text = ""
        visible = true; raise(); requestActivate(); field.forceActiveFocus()
    }
    function finish(ok) {
        const cbs = callbacks; callbacks = []; visible = false; field.text = ""
        for (const cb of cbs) cb(ok)
    }
    function tryUnlock() {
        busy.running = true
        Qt.callLater(() => {
            error = Passwords.unlock(field.text)
            busy.running = false
            if (error === "") finish(true); else { field.selectAll(); field.forceActiveFocus() }
        })
    }
    onClosing: if (callbacks.length) finish(false)
    Connections {
        target: Passwords
        function onWalletOpened(error) {
            if (Passwords.backend !== "kwallet" || dlg.callbacks.length === 0) return
            const cbs = dlg.callbacks; dlg.callbacks = []
            for (const cb of cbs) cb(error === "")
        }
    }

    ColumnLayout {
        id: layout
        anchors { fill: parent; margins: Kirigami.Units.gridUnit }
        RowLayout {
            Kirigami.Icon { source: "dialog-password"; Layout.preferredWidth: Kirigami.Units.iconSizes.large; Layout.preferredHeight: Layout.preferredWidth }
            QQC2.Label { text: "Enter your master password to use saved logins."; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
        Kirigami.PasswordField { id: field; Layout.fillWidth: true; onAccepted: dlg.tryUnlock() }
        QQC2.Label { visible: dlg.error !== ""; text: dlg.error; color: Kirigami.Theme.negativeTextColor }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            QQC2.BusyIndicator { id: busy; running: false; visible: running; Layout.preferredHeight: Kirigami.Units.iconSizes.medium }
            QQC2.Button { text: "Cancel"; onClicked: dlg.finish(false) }
            QQC2.Button { text: "Unlock"; highlighted: true; enabled: field.text !== ""; onClicked: dlg.tryUnlock() }
        }
    }
}
