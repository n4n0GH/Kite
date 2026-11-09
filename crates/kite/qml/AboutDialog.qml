import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Help → About Kite (BrowserWindow.showAbout): the logo, the version, and what
// it's built on, to copy into a bug report.
Kirigami.Dialog {
    id: about
    title: "About Kite"
    padding: Kirigami.Units.gridUnit
    // (Its text wraps at this width rather than setting the dialog's.)
    readonly property real textWidth: Kirigami.Units.gridUnit * 22
    standardButtons: Kirigami.Dialog.NoButton

    readonly property string version: Qt.application.version
    readonly property string engine: Browser.engineInfo()

    customFooterActions: [
        Kirigami.Action {
            text: "Copy Version Info"
            icon.name: "edit-copy"
            onTriggered: {
                Browser.copyText("Kite " + about.version + "\n" + about.engine)
                copied.visible = true
            }
        },
        Kirigami.Action { text: "Close"; icon.name: "dialog-close"; onTriggered: about.close() }
    ]
    onOpened: copied.visible = false

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            source: "qrc:/qt/qml/org/kde/kite/icons/org.kde.kite.svg"
            Layout.preferredWidth: Kirigami.Units.iconSizes.enormous
            Layout.preferredHeight: Layout.preferredWidth
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
        }

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Layout.alignment: Qt.AlignHCenter
            Kirigami.Heading {
                text: "Kite"
                level: 1
                font.weight: Font.Bold
                Layout.alignment: Qt.AlignHCenter
            }
            // The version, as a badge.
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                implicitWidth: versionLabel.implicitWidth + Kirigami.Units.largeSpacing * 2
                implicitHeight: versionLabel.implicitHeight + Kirigami.Units.smallSpacing * 2
                radius: height / 2
                color: Qt.alpha(Kirigami.Theme.highlightColor, 0.15)
                border.color: Qt.alpha(Kirigami.Theme.highlightColor, 0.5)
                QQC2.Label {
                    id: versionLabel
                    anchors.centerIn: parent
                    text: "Version " + about.version
                    color: Kirigami.Theme.highlightColor
                    font.weight: Font.DemiBold
                }
            }
        }

        Kirigami.Separator {
            Layout.preferredWidth: about.textWidth
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.smallSpacing
        }

        // What it's built on.
        QQC2.Label {
            Layout.fillWidth: true
            Layout.maximumWidth: about.textWidth
            Layout.alignment: Qt.AlignHCenter
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: about.engine
            font: Kirigami.Theme.smallFont
            opacity: 0.75
            lineHeight: 1.2
        }
        QQC2.Label {
            Layout.fillWidth: true
            Layout.maximumWidth: about.textWidth
            Layout.alignment: Qt.AlignHCenter
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: "Licensed under the GNU General Public License, version 3 or later."
            font: Kirigami.Theme.smallFont
            opacity: 0.75
        }
        QQC2.Label {
            id: copied
            visible: false
            Layout.alignment: Qt.AlignHCenter
            text: "Copied to the clipboard"
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.positiveTextColor
        }
    }
}
