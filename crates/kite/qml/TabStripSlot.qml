import QtQuick
import org.kde.kirigami as Kirigami

// Place for a horizontal tab strip (BrowserWindow): loaded while the tab bar
// is at `position`, and sliding in or out when it is shown or hidden (e.g.
// "hide when only one tab is open"): down from above for the strips above
// the page, up from below for the one under it.
Item {
    id: slot
    property var browser
    property string position
    property bool inUse: false
    property bool shown: false

    readonly property real fullHeight: loader.item ? loader.item.implicitHeight : 0
    implicitHeight: inUse && shown ? fullHeight : 0
    Behavior on implicitHeight { NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic } }
    visible: height > 0
    clip: true

    Loader {
        id: loader
        active: slot.inUse
        width: slot.width
        height: slot.fullHeight
        y: slot.position === "bottom" ? 0 : slot.height - slot.fullHeight
        sourceComponent: TabStrip { browser: slot.browser; position: slot.position }
    }
}
