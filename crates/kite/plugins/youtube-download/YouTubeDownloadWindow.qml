import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite
import "ytdl.js" as Ytdl

// YouTube Download in a browser window: on a YouTube video's page, a button in
// the address bar (only while yt-dlp is installed), and its popup to save the
// video and follow the download. The downloads themselves run in
// YouTubeDownloadApp.qml.
Item {
    id: root
    property var win
    property var app
    property var plugin

    // The plugin's item for the whole browser (made along with this one).
    property var store: null
    Component.onCompleted: Qt.callLater(() => root.store = root.app.pluginAppItem(root.plugin.id))

    readonly property var tab: win.currentTab
    readonly property string video: tab ? Ytdl.videoUrl(tab.url) : ""
    readonly property bool available: !!store && store.program !== "" && video !== ""
    // This video's download, if one was started this session.
    readonly property var job: { if (!store || !video) return null; store.downloads; return store.find(video) }

    // The address bar's button (BrowserWindow.pluginLocationButtons). The list
    // changes only when the button comes or goes; what it says follows the
    // download through its functions.
    readonly property var locationButtons: available ? [{
        icon: "download",
        text: () => !root.job ? "Download this video"
            : root.job.state === "running" ? "Downloading: " + root.percent(root.job) + "%"
            : root.job.state === "done" ? "Downloaded" : "Download this video",
        highlighted: () => !!root.job && (root.job.state === "running" || root.job.state === "done"),
        run: button => root.openPopup(button)
    }] : []

    function percent(d) { return d.total > 0 ? Math.min(100, Math.floor(d.done * 100 / d.total)) : 0 }

    function runAction(id) {
        if (id !== "youtube-download.download") return
        if (!store || store.program === "") { win.toast("YouTube Download: yt-dlp isn't installed", plugin.icon); return }
        if (!video) { win.toast("YouTube Download: this isn't a YouTube video", plugin.icon); return }
        download(app.pluginSetting(plugin, "quality"))
    }
    function download(quality) {
        if (store.start(video, Ytdl.titleOf(tab.title), quality)) win.toast("Downloading “" + Ytdl.titleOf(tab.title) + "”", plugin.icon)
        else win.toast("YouTube Download: yt-dlp didn't start", plugin.icon)
    }

    function openPopup(button) {
        popup.parent = button
        popup.x = button.width - popup.width
        popup.y = button.height + Kirigami.Units.smallSpacing
        quality.currentIndex = Math.max(0, Ytdl.QUALITIES.findIndex(q => q.k === app.pluginSetting(plugin, "quality")))
        popup.open()
    }

    QQC2.Popup {
        id: popup
        width: Kirigami.Units.gridUnit * 20
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Heading {
                level: 4
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
                text: root.job ? root.job.title : root.tab ? Ytdl.titleOf(root.tab.title) : ""
            }

            // Not started, or over without a file: what to save.
            RowLayout {
                visible: !root.job || root.job.state === "failed" || root.job.state === "stopped"
                QQC2.Label { text: "Quality:" }
                QQC2.ComboBox {
                    id: quality
                    Layout.fillWidth: true
                    model: Ytdl.QUALITIES
                    textRole: "t"
                    valueRole: "k"
                }
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: !!root.job && root.job.state === "stopped"
                wrapMode: Text.Wrap
                text: "Stopped. Trying again goes on from where it was."
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!root.job && root.job.state === "failed"
                type: Kirigami.MessageType.Error
                text: root.job ? root.job.error : ""
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: !!root.store && !root.store.ffmpeg && (!root.job || root.job.state !== "running")
                wrapMode: Text.Wrap
                opacity: 0.7
                font: Kirigami.Theme.smallFont
                text: "Without ffmpeg, the best quality with video and sound in one file (often 720p)."
            }

            // Under way.
            ColumnLayout {
                visible: !!root.job && root.job.state === "running"
                Layout.fillWidth: true
                QQC2.ProgressBar {
                    Layout.fillWidth: true
                    from: 0; to: 100
                    indeterminate: !!root.job && root.job.total <= 0
                    value: root.job ? root.percent(root.job) : 0
                }
                QQC2.Label {
                    opacity: 0.7
                    font: Kirigami.Theme.smallFont
                    text: !root.job ? "" : root.job.total <= 0 ? "Starting…"
                        : (root.job.part > 1 ? "Audio: " : "") + Ytdl.size(root.job.done) + " of " + Ytdl.size(root.job.total) + " (" + root.percent(root.job) + "%)"
                }
            }

            // Saved.
            QQC2.Label {
                Layout.fillWidth: true
                visible: !!root.job && root.job.state === "done"
                wrapMode: Text.Wrap
                text: root.job && root.job.file ? "Saved as “" + root.job.file.substring(root.job.file.lastIndexOf("/") + 1) + "”." : ""
            }

            RowLayout {
                Layout.alignment: Qt.AlignRight
                QQC2.Button {
                    visible: !root.job || root.job.state === "failed" || root.job.state === "stopped"
                    text: root.job ? "Try Again" : "Download"
                    icon.name: "download"
                    onClicked: root.download(quality.currentValue)
                }
                QQC2.Button {
                    visible: !!root.job && root.job.state === "running"
                    text: "Stop"
                    icon.name: "process-stop"
                    onClicked: root.store.cancel(root.video)
                }
                QQC2.Button {
                    visible: !!root.job && root.job.state === "done"
                    text: "Show in Folder"
                    icon.name: "folder-open"
                    onClicked: { root.store.showFolder(root.job.file); popup.close() }
                }
                QQC2.Button {
                    visible: !!root.job && root.job.state === "done"
                    text: "Open"
                    icon.name: "document-open"
                    onClicked: { root.store.showFile(root.job.file); popup.close() }
                }
                QQC2.Button {
                    visible: !!root.job && root.job.state === "done"
                    text: "Download Again"
                    icon.name: "view-refresh"
                    display: QQC2.AbstractButton.IconOnly
                    onClicked: root.store.forget(root.video)
                    QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }
    }
}
