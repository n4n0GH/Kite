import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// The toolbar's downloads button: a panel with the downloads under way, and
// the way to all of them (the Library's Downloads page).
QQC2.ToolButton {
    id: button
    property var win
    icon.name: "download"
    display: QQC2.AbstractButton.IconOnly
    text: "Downloads"
    checkable: true
    checked: panel.opened
    QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered && !panel.opened; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    onClicked: { win.downloadsActive = 0; panel.opened ? panel.close() : panel.open() }

    // A download began in this window (BrowserWindow.downloadStarted).
    function pulse() { badgeAnimation.restart() }
    Rectangle {
        visible: button.win.downloadsActive > 0
        width: Kirigami.Units.smallSpacing * 2; height: width; radius: width / 2
        color: Kirigami.Theme.highlightColor
        anchors { right: parent.right; top: parent.top; margins: Kirigami.Units.smallSpacing }
        SequentialAnimation on scale { id: badgeAnimation; running: false; NumberAnimation { to: 2; duration: 150 } NumberAnimation { to: 1; duration: 150 } }
    }

    QQC2.Popup {
        id: panel
        // Right-aligned with the button, like the extensions panel, and kept in the window
        // wherever the button was put on the toolbar.
        x: button.width - width
        y: button.height
        margins: 0
        width: Kirigami.Units.gridUnit * 20
        padding: Kirigami.Units.smallSpacing
        // Presses on the button toggle the panel (its own click).
        closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
        // It takes the keyboard, for Esc; the page has it back afterwards.
        focus: true
        // The downloads themselves (Main.activeDownloads), newest first: the rows follow
        // their progress, and the list changes only when one begins or ends.
        property var requests: []
        function reload() {
            const live = button.win.app.activeDownloads
            requests = Object.keys(live).map(Number).sort((a, b) => b - a).map(id => live[id])
        }
        onAboutToShow: reload()
        onClosed: if (button.win.currentTab) button.win.currentTab.focusPage()
        Connections {
            target: button.win.app
            enabled: panel.opened
            function onDownloadBegan() { panel.reload() }
            function onDownloadEnded() { panel.reload() }
        }

        contentItem: ColumnLayout {
            spacing: 0
            QQC2.Label {
                visible: panel.requests.length === 0
                text: "No active downloads"
                opacity: 0.7
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing
            }
            ListView {
                visible: count > 0
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.smallSpacing
                Layout.preferredHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 20)
                clip: true
                spacing: Kirigami.Units.smallSpacing
                model: panel.requests
                delegate: RowLayout {
                    id: row
                    required property var modelData
                    width: ListView.view.width
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon {
                        // (Set once: a download's type doesn't change, and Qt warns about binding to it.)
                        Component.onCompleted: source = row.modelData.mimeType ? row.modelData.mimeType.replace("/", "-") : "unknown"
                        fallback: "unknown"
                        Layout.leftMargin: Kirigami.Units.smallSpacing
                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                        Layout.preferredHeight: Layout.preferredWidth
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        QQC2.Label { text: row.modelData.downloadFileName; textFormat: Text.PlainText; elide: Text.ElideMiddle; Layout.fillWidth: true }
                        QQC2.ProgressBar {
                            Layout.fillWidth: true
                            from: 0; to: Math.max(1, row.modelData.totalBytes); value: row.modelData.receivedBytes
                            indeterminate: row.modelData.totalBytes <= 0 && !row.modelData.isPaused
                        }
                        QQC2.Label {
                            font: Kirigami.Theme.smallFont
                            opacity: 0.7
                            text: (row.modelData.isPaused ? "Paused — " : "") + button.win.app.formatBytes(row.modelData.receivedBytes) + " of " + button.win.app.formatBytes(row.modelData.totalBytes)
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                    QQC2.ToolButton {
                        icon.name: row.modelData.isPaused ? "media-playback-start" : "media-playback-pause"
                        display: QQC2.AbstractButton.IconOnly
                        text: row.modelData.isPaused ? "Resume" : "Pause"
                        onClicked: row.modelData.isPaused ? row.modelData.resume() : row.modelData.pause()
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    QQC2.ToolButton {
                        icon.name: "dialog-cancel"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Cancel"
                        onClicked: row.modelData.cancel()
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
            }
            QQC2.Button {
                text: "All Downloads"
                icon.name: "download"
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.smallSpacing
                onClicked: { panel.close(); button.win.app.showLibrary("downloads") }
            }
        }
    }
}
