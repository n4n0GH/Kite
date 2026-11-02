import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's DownloadManager.
Kirigami.ApplicationWindow {
    id: w
    property var app
    title: "Downloads — Kite"
    width: Kirigami.Units.gridUnit * 34
    height: Kirigami.Units.gridUnit * 26
    pageStack.initialPage: Kirigami.ScrollablePage {
        id: page
        title: "Downloads"
        property var items: JSON.parse(Browser.downloads())
        Connections { target: Browser; function onDownloadsChanged() { page.items = JSON.parse(Browser.downloads()) } }
        Timer { interval: 1000; repeat: true; running: page.items.some(i => i.state === "in_progress"); onTriggered: page.items = JSON.parse(Browser.downloads()) }
        actions: [
            Kirigami.Action { text: "Open Folder"; icon.name: "folder-download"; onTriggered: Qt.openUrlExternally("file://" + w.app.setting("downloads.directory")) },
            Kirigami.Action { text: "Clear List"; icon.name: "edit-clear-list"; onTriggered: Browser.clearDownloads() }
        ]
        function human(n) {
            if (n <= 0) return "?"
            const u = ["B", "KB", "MB", "GB", "TB"]; let i = 0
            while (n >= 1024 && i < u.length - 1) { n /= 1024; i++ }
            return n.toFixed(i ? 1 : 0) + " " + u[i]
        }
        ListView {
            model: page.items
            Kirigami.PlaceholderMessage { anchors.centerIn: parent; visible: page.items.length === 0; icon.name: "download"; text: "No downloads" }
            delegate: QQC2.ItemDelegate {
                required property var modelData
                width: ListView.view.width
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon { source: modelData.mime ? modelData.mime.replace("/", "-") : "unknown"; fallback: "unknown"; Layout.preferredWidth: Kirigami.Units.iconSizes.medium; Layout.preferredHeight: Layout.preferredWidth }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        QQC2.Label { text: modelData.path.substring(modelData.path.lastIndexOf("/") + 1); elide: Text.ElideMiddle; Layout.fillWidth: true; font.bold: true }
                        QQC2.ProgressBar { visible: modelData.state === "in_progress"; Layout.fillWidth: true; from: 0; to: Math.max(1, modelData.total); value: modelData.received; indeterminate: modelData.total <= 0 }
                        QQC2.Label {
                            font: Kirigami.Theme.smallFont; opacity: 0.75
                            text: modelData.state === "in_progress" ? page.human(modelData.received) + " of " + page.human(modelData.total)
                                : modelData.state === "complete" ? page.human(modelData.total) + " — " + Browser.hostOf(modelData.url)
                                : modelData.state === "cancelled" ? "Cancelled" : "Failed"
                        }
                    }
                    QQC2.ToolButton { icon.name: "document-open"; visible: modelData.state === "complete"; onClicked: Qt.openUrlExternally("file://" + modelData.path); QQC2.ToolTip.text: "Open"; QQC2.ToolTip.visible: hovered }
                    QQC2.ToolButton { icon.name: "folder-open"; visible: modelData.state === "complete"; onClicked: Qt.openUrlExternally("file://" + modelData.path.substring(0, modelData.path.lastIndexOf("/"))); QQC2.ToolTip.text: "Show in folder"; QQC2.ToolTip.visible: hovered }
                    QQC2.ToolButton { icon.name: "edit-delete-remove"; visible: modelData.state !== "in_progress"; onClicked: Browser.removeDownload(modelData.id); QQC2.ToolTip.text: "Remove from list"; QQC2.ToolTip.visible: hovered }
                }
                onDoubleClicked: if (modelData.state === "complete") Qt.openUrlExternally("file://" + modelData.path)
            }
        }
    }
}
