import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Settings → Plugins & Extensions → Plasma Integration.
ColumnLayout {
    id: root
    property var app
    property var plugin
    spacing: Kirigami.Units.largeSpacing

    component Setting: QQC2.CheckBox {
        property string key
        checked: !!root.app.pluginSetting(root.plugin, key)
        onToggled: root.app.setPluginSetting(root.plugin, key, checked)
    }

    SettingsSectionHeader { text: "Desktop" }
    SettingsForm {
        Setting { Kirigami.FormData.label: "Media:"; text: "Control what's playing from the desktop"; key: "mediaControls" }
        SettingsHint { text: "With Plasma's media controls, media keys, and paired devices. Pages already open need a reload." }
        Setting { Kirigami.FormData.label: "Downloads:"; text: "Show downloads in the notification area"; key: "downloads" }
        Setting { Kirigami.FormData.label: "Sharing:"; text: "Send pages and links to devices with KDE Connect"; key: "kdeConnect" }
        Setting { text: "Let pages share links with KDE's share menu"; key: "share" }
        SettingsHint { text: "The Web Share API, when you click a page's share button. Pages already open need a reload." }
    }

    SettingsSectionHeader { text: "Search" }
    SettingsForm {
        Setting { Kirigami.FormData.label: "Find in KRunner:"; text: "Open tabs"; key: "searchTabs" }
        Setting { text: "Bookmarks"; key: "searchBookmarks" }
        Setting { text: "History"; key: "searchHistory" }
        SettingsHint { text: "In KRunner and the application launcher, while Kite is running. The first time, KRunner has to be restarted to know of Kite: systemctl --user restart plasma-krunner. The launcher needs you to log in again." }
    }

    SettingsSectionHeader { text: "Private Windows" }
    SettingsForm {
        Setting { Kirigami.FormData.label: "Private windows:"; text: "Include them in all of the above"; key: "privateWindows" }
        SettingsHint { text: "The desktop, and paired devices, would see what plays there and the titles of its tabs. Its downloads show here too if Settings → Privacy allows notifications from private windows." }
    }
}
