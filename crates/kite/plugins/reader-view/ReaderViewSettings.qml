import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Settings → Plugins & Extensions → Reader View: how articles look. (The same
// as the button over a page in reader view.)
ColumnLayout {
    id: root
    property var app
    property var plugin
    spacing: Kirigami.Units.largeSpacing

    SettingsSectionHeader { text: "Look" }
    ReaderViewLook {
        app: root.app
        plugin: root.plugin
        Layout.fillWidth: true
        hint: "System colours follow your desktop's. Pages already in reader view change at once."
    }
}
