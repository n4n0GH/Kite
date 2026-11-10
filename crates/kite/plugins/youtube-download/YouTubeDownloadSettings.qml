import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite
import "ytdl.js" as Ytdl

// Settings → Plugins & Extensions → YouTube Download: whether yt-dlp (and
// ffmpeg) are there, and where and how videos are saved.
ColumnLayout {
    id: root
    property var app
    property var plugin
    spacing: Kirigami.Units.largeSpacing

    // What's installed, looked up each time the page is shown (and by the
    // plugin's own item too, so the address bar follows).
    property string program: ""
    property string ffmpeg: ""
    property string version: ""
    function check() {
        program = finder.findProgram(app.pluginSetting(plugin, "program") || "yt-dlp")
        ffmpeg = finder.findProgram("ffmpeg")
        version = ""
        if (program !== "") versionCheck.start(program, JSON.stringify(["--version"]))
        const store = app.pluginAppItem(plugin.id)
        if (store) store.findPrograms()
    }
    Process { id: finder }
    Process {
        id: versionCheck
        onOutput: line => { if (root.version === "" && /^\d/.test(line)) root.version = line.trim() }
    }
    Component.onCompleted: check()

    SettingsSectionHeader { text: "yt-dlp" }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: root.program === ""
        type: Kirigami.MessageType.Warning
        text: "yt-dlp isn't installed, so there's no download button in the address bar. Install it with your distribution's software manager (the package is called yt-dlp), then check again."
        actions: [Kirigami.Action { text: "Check Again"; icon.name: "view-refresh"; onTriggered: root.check() }]
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: root.program !== "" && root.ffmpeg === ""
        type: Kirigami.MessageType.Information
        text: "ffmpeg isn't installed. Without it, videos are saved in the best quality that comes as one file (often 720p), and audio isn't converted."
        actions: [Kirigami.Action { text: "Check Again"; icon.name: "view-refresh"; onTriggered: root.check() }]
    }
    SettingsForm {
        QQC2.Label {
            Kirigami.FormData.label: "Found:"
            text: root.program === "" ? "No" : root.program + (root.version ? " (version " + root.version + ")" : "")
            elide: Text.ElideMiddle
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
        }
        QQC2.TextField {
            Kirigami.FormData.label: "Program:"
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            placeholderText: "yt-dlp"
            text: root.app.pluginSetting(root.plugin, "program")
            onEditingFinished: if (text.trim() !== root.app.pluginSetting(root.plugin, "program")) { root.app.setPluginSetting(root.plugin, "program", text.trim()); root.check() }
        }
        SettingsHint { text: "Empty uses the yt-dlp on your PATH. A path to another one (a newer one you keep up to date yourself, say) uses that instead." }
    }

    SettingsSectionHeader { text: "Saving" }
    SettingsForm {
        QQC2.ComboBox {
            Kirigami.FormData.label: "Quality:"
            model: Ytdl.QUALITIES
            textRole: "t"
            valueRole: "k"
            wheelEnabled: false
            Component.onCompleted: currentIndex = Math.max(0, indexOfValue(root.app.pluginSetting(root.plugin, "quality")))
            onActivated: root.app.setPluginSetting(root.plugin, "quality", currentValue)
        }
        SettingsHint { text: "What the download button offers first; it can be changed for each video." }
        RowLayout {
            Kirigami.FormData.label: "Folder:"
            property string chosen: root.app.pluginSetting(root.plugin, "folder")
            id: folderRow
            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 14
                elide: Text.ElideMiddle
                text: folderRow.chosen || root.app.setting("downloads.directory")
            }
            QQC2.Button { text: "Choose…"; icon.name: "folder-open"; onClicked: folderDialog.open() }
            QQC2.Button {
                text: "Default"
                icon.name: "edit-undo"
                enabled: folderRow.chosen !== ""
                onClicked: { root.app.setPluginSetting(root.plugin, "folder", ""); folderRow.chosen = "" }
            }
        }
        SettingsHint { text: "Kite's download folder unless you choose another." }
    }
    FolderDialog {
        id: folderDialog
        title: "Folder for Videos"
        currentFolder: "file://" + (folderRow.chosen || root.app.setting("downloads.directory"))
        onAccepted: {
            const path = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, ""))
            root.app.setPluginSetting(root.plugin, "folder", path)
            folderRow.chosen = path
        }
    }
}
