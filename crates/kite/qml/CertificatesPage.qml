import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Settings → Privacy → Manage Certificate Exceptions (kite_core::certs).
Kirigami.ScrollablePage {
    id: page
    title: "Certificate Exceptions"
    property var list: JSON.parse(Browser.certExceptions())
    function reload() { list = JSON.parse(Browser.certExceptions()) }

    actions: [
        Kirigami.Action { text: "Add Site…"; icon.name: "list-add"; onTriggered: { siteField.text = ""; addSite.bad = false; addSite.open() } }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: "Sites whose certificate problems you accepted for good, with “Always Accept This Certificate” on the warning page. Removing one takes full effect after restarting Kite, as what was accepted is remembered until then."
        }
        QQC2.Label {
            visible: page.list.length === 0
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.gridUnit
            horizontalAlignment: Text.AlignHCenter
            opacity: 0.7
            text: "No exceptions."
        }
        Repeater {
            model: page.list
            delegate: RowLayout {
                id: row
                required property var modelData
                Layout.fillWidth: true
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    QQC2.Label { Layout.fillWidth: true; elide: Text.ElideRight; text: row.modelData.host }
                    QQC2.Label {
                        Layout.fillWidth: true
                        font: Kirigami.Theme.smallFont
                        elide: Text.ElideRight
                        opacity: 0.7
                        color: row.modelData.fingerprint ? Kirigami.Theme.textColor : Kirigami.Theme.neutralTextColor
                        text: row.modelData.fingerprint ? "This certificate only: " + row.modelData.fingerprint.substring(0, 32) + "…" : "Any certificate"
                    }
                    QQC2.Label {
                        visible: !!row.modelData.description
                        Layout.fillWidth: true
                        font: Kirigami.Theme.smallFont
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        text: row.modelData.description
                    }
                }
                QQC2.ToolButton {
                    icon.name: "edit-delete-remove"
                    display: QQC2.AbstractButton.IconOnly
                    text: "Remove Exception"
                    onClicked: {
                        Browser.removeCertException(row.modelData.host, row.modelData.fingerprint)
                        Qt.callLater(page.reload)
                    }
                    QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }
    }

    Kirigami.Dialog {
        id: addSite
        property bool bad: false
        title: "Accept Any Certificate of a Site"
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: "Add Site"
                icon.name: "list-add"
                enabled: siteField.text.trim() !== ""
                onTriggered: {
                    if (Browser.addCertExceptionForSite(siteField.text)) { addSite.close(); page.reload() }
                    else addSite.bad = true
                }
            },
            Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: addSite.close() }
        ]
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Layout.preferredWidth: Kirigami.Units.gridUnit * 24
            QQC2.TextField {
                id: siteField
                Layout.fillWidth: true
                placeholderText: "router.local"
                onTextEdited: addSite.bad = false
            }
            QQC2.Label {
                visible: addSite.bad
                Layout.fillWidth: true
                color: Kirigami.Theme.negativeTextColor
                text: "That isn't the name of a site."
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: true
                type: Kirigami.MessageType.Warning
                text: "Kite won't warn about this site's certificate at all, so nothing tells you if someone is listening in. Only for your own devices that make themselves new certificates. For anything else, use “Always Accept This Certificate” on the warning page."
            }
        }
    }
}
