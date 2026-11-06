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
        Layout.leftMargin: Kirigami.Units.gridUnit
        text: "Also in private windows"
        enabled: !!root.app.pluginSetting(root.plugin, "mediaControls")
        checked: !!root.app.pluginSetting(root.plugin, "privateWindows")
        onToggled: root.app.setPluginSetting(root.plugin, "privateWindows", checked)
    }
    QQC2.Label {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit * 2
        text: "The desktop, and paired devices, would see the titles of what plays there."
        wrapMode: Text.WordWrap
        font: Kirigami.Theme.smallFont
        opacity: 0.8
    }
    QQC2.CheckBox {
        text: "Send pages and links to devices with KDE Connect"
        checked: !!root.app.pluginSetting(root.plugin, "kdeConnect")
        onToggled: root.app.setPluginSetting(root.plugin, "kdeConnect", checked)
    }
}
