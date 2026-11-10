import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// A list row's icon button on a Settings page: its name as a tooltip.
QQC2.ToolButton {
    display: QQC2.AbstractButton.IconOnly
    QQC2.ToolTip.text: text
    QQC2.ToolTip.visible: hovered
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
}
