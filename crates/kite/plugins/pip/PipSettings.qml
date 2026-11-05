import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Settings → Plugins & Extensions → Picture-in-Picture.
ColumnLayout {
    id: root
    property var app
    property var plugin

    QQC2.CheckBox {
        text: "Show on all virtual desktops"
        checked: !!root.app.pluginSetting(root.plugin, "allDesktops")
        onToggled: root.app.setPluginSetting(root.plugin, "allDesktops", checked)
    }
    QQC2.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        opacity: 0.7
        font: Kirigami.Theme.smallFont
        text: "On KDE Plasma. Takes effect the next time a video goes into its window."
    }
}
