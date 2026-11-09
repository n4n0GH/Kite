import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Over a page in reader view (ReaderViewTab.qml): its look, and leaving it.
Rectangle {
    id: controls
    property var tab
    property var reader

    width: buttons.implicitWidth
    height: buttons.implicitHeight
    radius: Kirigami.Units.cornerRadius
    color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.9)
    border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)

    RowLayout {
        id: buttons
        spacing: 0
        QQC2.ToolButton {
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
            onClicked: controls.reader.toggle(controls.tab)
            QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }

    QQC2.Popup {
        id: look
        y: controls.height + Kirigami.Units.smallSpacing
        x: controls.width - width
        // The button toggles it.
        closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
        contentItem: ReaderViewLook { app: controls.reader.app; plugin: controls.reader.plugin }
    }
}
