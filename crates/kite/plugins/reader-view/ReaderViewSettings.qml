import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Settings → Plugins & Extensions → Reader View: how articles look. (The same
// as the button over a page in reader view.)
ColumnLayout {
    id: root
    property var app
    property var plugin

    ReaderViewLook { app: root.app; plugin: root.plugin; Layout.fillWidth: true; Layout.maximumWidth: Kirigami.Units.gridUnit * 26 }
    QQC2.Label {
        Layout.fillWidth: true
        Layout.maximumWidth: Kirigami.Units.gridUnit * 26
        Layout.topMargin: Kirigami.Units.largeSpacing
        wrapMode: Text.Wrap
        opacity: 0.7
        font: Kirigami.Theme.smallFont
        text: "System colours follow your desktop's. Pages already in reader view change at once."
    }
}
