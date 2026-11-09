import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Reader View's look: text size, line length, font and colours. Over a page in
// reader view (ReaderViewControls.qml) and in Settings (ReaderViewSettings.qml);
// a change reaches open pages at once (ReaderViewWindow.qml).
GridLayout {
    id: look
    property var app
    property var plugin

    // The settings as they are now, kept up with changes from anywhere.
    property var values: read()
    function read() {
        const out = {}
        for (const key of ["fontSize", "width", "font", "theme"]) out[key] = app.pluginSetting(plugin, key)
        return out
    }
    Connections {
        target: Browser
        function onSettingChanged(key) { if (key.startsWith("plugins." + look.plugin.id + ".")) look.values = look.read() }
    }
    function set(key, value) { app.setPluginSetting(plugin, key, value) }
    function step(key, by, low, high) { set(key, Math.max(low, Math.min(high, values[key] + by))) }

    columns: 2
    columnSpacing: Kirigami.Units.largeSpacing
    rowSpacing: Kirigami.Units.smallSpacing

    component Choice: QQC2.Button {
        required property string key
        required property string value
        Layout.fillWidth: true
        checkable: true
        autoExclusive: false
        checked: look.values[key] === value
        onClicked: { look.set(key, value); checked = Qt.binding(() => look.values[key] === value) }
    }

    QQC2.Label { text: "Text size:" }
    RowLayout {
        QQC2.ToolButton { text: "−"; Accessible.name: "Smaller text"; onClicked: look.step("fontSize", -2, 12, 40) }
        QQC2.Label { text: look.values.fontSize; horizontalAlignment: Text.AlignHCenter; Layout.fillWidth: true }
        QQC2.ToolButton { text: "+"; Accessible.name: "Larger text"; onClicked: look.step("fontSize", 2, 12, 40) }
    }
    QQC2.Label { text: "Line length:" }
    RowLayout {
        QQC2.ToolButton { text: "−"; Accessible.name: "Shorter lines"; onClicked: look.step("width", -4, 24, 70) }
        QQC2.Label { text: look.values.width; horizontalAlignment: Text.AlignHCenter; Layout.fillWidth: true }
        QQC2.ToolButton { text: "+"; Accessible.name: "Longer lines"; onClicked: look.step("width", 4, 24, 70) }
    }
    QQC2.Label { text: "Font:" }
    RowLayout {
        Choice { key: "font"; value: "serif"; text: "Serif"; font.family: "serif" }
        Choice { key: "font"; value: "sans"; text: "Sans"; font.family: "sans-serif" }
    }
    QQC2.Label { text: "Colours:" }
    RowLayout {
        Choice { key: "theme"; value: "auto"; text: "System" }
        Choice { key: "theme"; value: "light"; text: "Light" }
        Choice { key: "theme"; value: "sepia"; text: "Sepia" }
        Choice { key: "theme"; value: "dark"; text: "Dark" }
    }
}
