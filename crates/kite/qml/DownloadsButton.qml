import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// The toolbar's downloads button: a panel with the downloads under way and the
// ones of this window that ended since it was last looked at (what the dot on
// the button is about), and the way to all of them (the Library's Downloads page).
QQC2.ToolButton {
    id: button
    property var win
    icon.name: "download"
    display: QQC2.AbstractButton.IconOnly
    text: "Downloads"
    checkable: true
    checked: panel.opened
    QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered && !panel.opened; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    onClicked: panel.opened ? panel.close() : panel.open()

    // A download began in this window (BrowserWindow.downloadStarted). The dot:
    // the panel has something that wasn't seen yet (BrowserWindow.downloadsNew).
    function pulse() { badgeAnimation.restart() }
    Rectangle {
        visible: button.win.downloadsNew.length > 0 && !panel.opened
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
        // Newest first, {id, request, record}. Under way: the download itself
        // (`request`, of Main.activeDownloads), whose progress the row follows.
        // Ended: its `record` in the list of downloads, as the Library shows it,
        // with what the Library can do with it. The list changes only when a
        // download begins or ends.
        property var rows: []
        // What it has shown since it opened: a download that ends stays, as ended.
        property var shown: []
        function reload() {
            const live = button.win.app.activeDownloads
            const ids = new Set(shown.concat(Object.keys(live).map(Number), button.win.downloadsNew))
            const sorted = Array.from(ids).sort((a, b) => b - a)
            const records = sorted.every(id => live[id]) ? [] : JSON.parse(Browser.downloads())
            const out = []
            for (const id of sorted) {
                if (live[id]) { out.push({id: id, request: live[id], record: null}); continue }
                const r = records.find(r => r.id === id)
                if (r && (r.state === "complete" || r.state === "interrupted")) out.push({id: id, request: null, record: r})
            }
            shown = out.map(r => r.id)
            rows = out
        }
        onAboutToShow: { shown = []; reload() }
        onClosed: {
            // All of it was seen.
            button.win.downloadsNew = []
            if (button.win.currentTab) button.win.currentTab.focusPage()
        }
        Connections {
            target: button.win.app
            enabled: panel.opened
            function onDownloadBegan() { panel.reload() }
            function onDownloadEnded(id) {
                // (Told about by somebody else, as the media saver's many: gone like a cancelled one.)
                if (button.win.app.claimedDownloads[id]) panel.shown = panel.shown.filter(x => x !== id)
                panel.reload()
            }
        }

        contentItem: ColumnLayout {
            spacing: 0
            QQC2.Label {
                visible: panel.rows.length === 0
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
                model: panel.rows
                delegate: RowLayout {
                    id: row
                    required property var modelData
                    // One of the two: under way, or ended.
                    readonly property var request: modelData.request
                    readonly property var record: modelData.record
                    readonly property bool complete: !!record && record.state === "complete"
                    width: ListView.view.width
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon {
                        // (Set once: a download's type doesn't change, and Qt warns about binding to it.)
                        Component.onCompleted: {
                            const type = row.request ? row.request.mimeType : row.record.mime
                            source = type ? type.replace("/", "-") : "unknown"
                        }
                        fallback: "unknown"
                        Layout.leftMargin: Kirigami.Units.smallSpacing
                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                        Layout.preferredHeight: Layout.preferredWidth
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        QQC2.Label {
                            text: row.request ? row.request.downloadFileName : row.record.path.substring(row.record.path.lastIndexOf("/") + 1)
                            textFormat: Text.PlainText; elide: Text.ElideMiddle; Layout.fillWidth: true
                        }
                        QQC2.ProgressBar {
                            visible: !!row.request
                            Layout.fillWidth: true
                            from: 0; to: row.request ? Math.max(1, row.request.totalBytes) : 1; value: row.request ? row.request.receivedBytes : 0
                            indeterminate: !!row.request && row.request.totalBytes <= 0 && !row.request.isPaused
                        }
                        QQC2.Label {
                            font: Kirigami.Theme.smallFont
                            opacity: 0.7
                            text: row.request ? (row.request.isPaused ? "Paused — " : "") + button.win.app.formatBytes(row.request.receivedBytes) + " of " + button.win.app.formatBytes(row.request.totalBytes)
                                : row.complete ? button.win.app.formatBytes(row.record.total) + " — " + Browser.hostOf(row.record.url)
                                : "Failed"
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                    // Under way.
                    QQC2.ToolButton {
                        visible: !!row.request
                        icon.name: row.request && row.request.isPaused ? "media-playback-start" : "media-playback-pause"
                        display: QQC2.AbstractButton.IconOnly
                        text: row.request && row.request.isPaused ? "Resume" : "Pause"
                        onClicked: row.request.isPaused ? row.request.resume() : row.request.pause()
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    QQC2.ToolButton {
                        visible: !!row.request
                        icon.name: "dialog-cancel"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Cancel"
                        onClicked: row.request.cancel()
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    // Ended: what the Library's Downloads page has for it.
                    QQC2.ToolButton {
                        visible: row.complete
                        icon.name: "document-open-symbolic"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Open"
                        onClicked: { Qt.openUrlExternally(button.win.app.fileUrl(row.record.path)); panel.close() }
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    QQC2.ToolButton {
                        visible: row.complete
                        icon.name: "folder-open"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Show in folder"
                        onClicked: { button.win.app.showInFolder(row.record.path); panel.close() }
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    QQC2.ToolButton {
                        visible: !!row.record
                        icon.name: "edit-delete-remove"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Remove from list"
                        onClicked: { Browser.removeDownload(row.record.id); panel.reload() }
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
