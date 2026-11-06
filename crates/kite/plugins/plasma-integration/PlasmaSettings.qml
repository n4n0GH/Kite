import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Settings → Plugins & Extensions → Plasma Integration.
ColumnLayout {
    id: root
    property var app
    property var plugin

    QQC2.CheckBox {
        text: "Control what's playing from the desktop"
        checked: !!root.app.pluginSetting(root.plugin, "mediaControls")
        onToggled: root.app.setPluginSetting(root.plugin, "mediaControls", checked)
    }
    QQC2.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit
        text: "With Plasma's media controls, media keys, and paired devices. Pages already open need a reload."
        wrapMode: Text.WordWrap
        font: Kirigami.Theme.smallFont
        opacity: 0.8
    }
    QQC2.CheckBox {
        text: "Send pages and links to devices with KDE Connect"
        checked: !!root.app.pluginSetting(root.plugin, "kdeConnect")
        onToggled: root.app.setPluginSetting(root.plugin, "kdeConnect", checked)
    }
    QQC2.CheckBox {
        text: "Show downloads in the notification area"
        checked: !!root.app.pluginSetting(root.plugin, "downloads")
        onToggled: root.app.setPluginSetting(root.plugin, "downloads", checked)
    }
    QQC2.CheckBox {
        text: "Let pages share links with KDE's share menu"
        checked: !!root.app.pluginSetting(root.plugin, "share")
        onToggled: root.app.setPluginSetting(root.plugin, "share", checked)
    }
    QQC2.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit
        text: "The Web Share API, when you click a page's share button. Pages already open need a reload."
        wrapMode: Text.WordWrap
        font: Kirigami.Theme.smallFont
        opacity: 0.8
    }
    QQC2.Label {
        Layout.topMargin: Kirigami.Units.smallSpacing
        text: "Find in KRunner and the application launcher, while Kite is running:"
    }
    Repeater {
        model: [{key: "searchTabs", text: "Open tabs"}, {key: "searchBookmarks", text: "Bookmarks"}, {key: "searchHistory", text: "History"}]
        QQC2.CheckBox {
            required property var modelData
            Layout.leftMargin: Kirigami.Units.gridUnit
            text: modelData.text
            checked: !!root.app.pluginSetting(root.plugin, modelData.key)
            onToggled: root.app.setPluginSetting(root.plugin, modelData.key, checked)
        }
    }
    QQC2.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit
        text: "The first time, KRunner has to be restarted to know of Kite: systemctl --user restart plasma-krunner. The launcher needs you to log in again."
        wrapMode: Text.WordWrap
        font: Kirigami.Theme.smallFont
        opacity: 0.8
    }
    QQC2.CheckBox {
        Layout.topMargin: Kirigami.Units.smallSpacing
        text: "Include private windows"
        checked: !!root.app.pluginSetting(root.plugin, "privateWindows")
        onToggled: root.app.setPluginSetting(root.plugin, "privateWindows", checked)
    }
    QQC2.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit
        text: "The desktop, and paired devices, would see what plays there, the titles of its tabs, and what it downloads."
        wrapMode: Text.WordWrap
        font: Kirigami.Theme.smallFont
        opacity: 0.8
    }
}
