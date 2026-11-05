import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Over a page in reader view: its look (text size, line length, font and
// colours; "reader.*", applied to open pages at once), and leaving it.
Rectangle {
    id: controls
    property var tab
    readonly property var live: tab.app.liveSettings

    function set(key, value) { Browser.setSetting(key, JSON.stringify(value)) }
    function step(key, by, low, high) { set(key, Math.max(low, Math.min(high, live[key] + by))) }

    width: buttons.implicitWidth
    height: buttons.implicitHeight
    radius: Kirigami.Units.cornerRadius
    color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.9)
    border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)

    RowLayout {
        id: buttons
        spacing: 0
        QQC2.ToolButton {
            id: lookButton
            icon.name: "format-font-size-more"
            text: "Text and Colours"
            display: QQC2.AbstractButton.IconOnly
            onClicked: look.opened ? look.close() : look.open()
            QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
        QQC2.ToolButton {
            icon.name: "window-close"
            text: "Close Reader View"
            display: QQC2.AbstractButton.IconOnly
            onClicked: controls.tab.toggleReader()
            QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }

    component Choice: QQC2.Button {
        required property string key
        required property string value
        Layout.fillWidth: true
        checkable: true
        autoExclusive: false
        checked: controls.live[key] === value
        onClicked: { controls.set(key, value); checked = Qt.binding(() => controls.live[key] === value) }
    }

    QQC2.Popup {
        id: look
        y: controls.height + Kirigami.Units.smallSpacing
        x: controls.width - width
        // The button toggles it.
        closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
        contentItem: GridLayout {
            columns: 2
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.smallSpacing

            QQC2.Label { text: "Text size:" }
            RowLayout {
                QQC2.ToolButton { text: "−"; Accessible.name: "Smaller text"; onClicked: controls.step("reader.fontSize", -2, 12, 40) }
                QQC2.Label { text: controls.live["reader.fontSize"]; horizontalAlignment: Text.AlignHCenter; Layout.fillWidth: true }
                QQC2.ToolButton { text: "+"; Accessible.name: "Larger text"; onClicked: controls.step("reader.fontSize", 2, 12, 40) }
            }
            QQC2.Label { text: "Line length:" }
            RowLayout {
                QQC2.ToolButton { text: "−"; Accessible.name: "Shorter lines"; onClicked: controls.step("reader.width", -4, 24, 70) }
                QQC2.Label { text: controls.live["reader.width"]; horizontalAlignment: Text.AlignHCenter; Layout.fillWidth: true }
                QQC2.ToolButton { text: "+"; Accessible.name: "Longer lines"; onClicked: controls.step("reader.width", 4, 24, 70) }
            }
            QQC2.Label { text: "Font:" }
            RowLayout {
                Choice { key: "reader.font"; value: "serif"; text: "Serif"; font.family: "serif" }
                Choice { key: "reader.font"; value: "sans"; text: "Sans"; font.family: "sans-serif" }
            }
            QQC2.Label { text: "Colours:" }
            RowLayout {
                Choice { key: "reader.theme"; value: "auto"; text: "System" }
                Choice { key: "reader.theme"; value: "light"; text: "Light" }
                Choice { key: "reader.theme"; value: "sepia"; text: "Sepia" }
                Choice { key: "reader.theme"; value: "dark"; text: "Dark" }
            }
        }
    }
}
