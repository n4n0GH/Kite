import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

// The picture-in-picture window: a small window above others holding a
// tab's page (Main.qml moves the WebEngineView in), in which the page shows
// only the video (bridge.js). Buttons on hover: back to the tab, close.
Window {
    id: pip
    // Main.qml: back to the tab (keeps playing) or close (pauses).
    signal backToTab()
    signal closeRequested()
    property string pageTitle: ""
    readonly property Item area: holder

    title: "Picture-in-Picture"
    flags: Qt.Window | Qt.WindowStaysOnTopHint
    color: "black"
    minimumWidth: 200
    minimumHeight: 112

    // Closing from the window manager counts as closing.
    onClosing: close => { close.accepted = false; pip.closeRequested() }

    Item { id: holder; anchors.fill: parent }

    HoverHandler { id: hover }
    Rectangle {
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: bar.implicitHeight + Kirigami.Units.smallSpacing * 2
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.65) }
            GradientStop { position: 1; color: "transparent" }
        }
        opacity: hover.hovered ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        RowLayout {
            id: bar
            anchors { fill: parent; margins: Kirigami.Units.smallSpacing }
            QQC2.Label {
                Layout.fillWidth: true
                text: pip.pageTitle
                elide: Text.ElideRight
                color: "white"
                leftPadding: Kirigami.Units.smallSpacing
            }
            QQC2.ToolButton {
                icon.name: "kite-pip-exit"
                icon.color: "white"
                onClicked: pip.backToTab()
                QQC2.ToolTip.text: "Back to Tab"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
            QQC2.ToolButton {
                icon.name: "window-close"
                icon.color: "white"
                onClicked: pip.closeRequested()
                QQC2.ToolTip.text: "Close (pauses the video)"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }
}
