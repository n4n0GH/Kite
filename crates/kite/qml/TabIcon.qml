import QtQuick
import QtQuick.Window
import org.kde.kirigami as Kirigami

// A tab's icon: the page's own, or, for pages not loaded and reader view, the
// cached one (a data: address, which Kirigami.Icon can't draw).
Item {
    id: tabIcon
    property string source: ""
    readonly property bool cached: source.startsWith("data:")

    Image {
        anchors.fill: parent
        visible: tabIcon.cached
        source: tabIcon.cached ? tabIcon.source : ""
        sourceSize: Qt.size(width * Screen.devicePixelRatio, height * Screen.devicePixelRatio)
        fillMode: Image.PreserveAspectFit
        smooth: true
    }
    Kirigami.Icon {
        anchors.fill: parent
        visible: !tabIcon.cached
        source: tabIcon.source !== "" && !tabIcon.cached ? tabIcon.source : "text-html"
        fallback: "text-html"
    }
}
