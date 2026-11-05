import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// Drawn like Breeze's tab close indicator (as in Dolphin): a plain × that
// becomes a white × on a red circle on hover. Drawn here rather than taken from
// the icon theme, as themes differ (e.g. Qogir's close icons are all red circles).
Item {
    id: button
    property string text: "Close"
    signal clicked()
    implicitWidth: Kirigami.Units.iconSizes.small
    implicitHeight: Kirigami.Units.iconSizes.small
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        visible: closeHover.hovered
        color: Kirigami.Theme.negativeTextColor
    }
    Kirigami.Icon {
        anchors.fill: parent
        anchors.margins: closeHover.hovered ? 3 : 2
        source: "window-close-symbolic"
        isMask: true
        color: closeHover.hovered ? "white" : Kirigami.Theme.textColor
    }
    Accessible.role: Accessible.Button
    Accessible.name: button.text
    Accessible.onPressAction: button.clicked()
    HoverHandler { id: closeHover }
    // Exclusive, so this doesn't also act on what it sits on.
    TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: button.clicked() }
    QQC2.ToolTip.text: button.text
    QQC2.ToolTip.visible: closeHover.hovered
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
}
