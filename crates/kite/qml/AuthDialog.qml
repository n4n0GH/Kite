import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.kde.kite

// HTTP / proxy authentication, with saved credentials (Falkon's
// HTTP authorization autofill).
Kirigami.Dialog {
    id: dlg
    property var tab
    property var request: null
    property string site: ""
    parent: tab ? tab.window.contentItem : null
    title: "Authentication Required"
    standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
    padding: Kirigami.Units.largeSpacing
    preferredWidth: Kirigami.Units.gridUnit * 24

    function ask(r) {
        request = r
        site = r.url.toString()
        const saved = JSON.parse(Passwords.httpAuth(site))
        user.text = saved ? saved.username : ""
        pass.text = saved ? saved.password : ""
        remember.checked = !!saved
        open()
        user.forceActiveFocus()
    }
    onAccepted: {
        if (!request) return
        if (remember.checked) Passwords.saveHttpAuth(site, tab.window.privateMode, user.text, pass.text)
        request.dialogAccept(user.text, pass.text)
        request = null
    }
    onRejected: { if (request) request.dialogReject(); request = null }

    ColumnLayout {
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            // The realm is the site's own text: never markup.
            textFormat: Text.PlainText
            text: dlg.request ? ((dlg.request.type === AuthenticationDialogRequest.AuthenticationTypeProxy ? "The proxy " + dlg.request.proxyHost : Browser.hostOf(dlg.site))
                                 + " requires a username and password." + (dlg.request.realm ? "\nThe site says: “" + dlg.request.realm + "”" : "")) : ""
        }
        Kirigami.FormLayout {
            Layout.fillWidth: true
            QQC2.TextField { id: user; Kirigami.FormData.label: "Username:"; Layout.fillWidth: true; onAccepted: pass.forceActiveFocus() }
            Kirigami.PasswordField { id: pass; Kirigami.FormData.label: "Password:"; Layout.fillWidth: true; onAccepted: dlg.accept() }
            QQC2.CheckBox { id: remember; text: "Remember this login"; visible: !dlg.tab || !dlg.tab.window.privateMode }
        }
    }
}
