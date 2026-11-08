import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import QtWebEngine
import org.kde.kirigami as Kirigami

// Window for JavaScript popups (window.open with features), like Falkon's
// PopupWindow: read-only location bar and the page.
Window {
    id: w
    property var app
    property var opener
    property WebEngineProfile profile
    property var request
    width: request && request.requestedGeometry.width > 0 ? request.requestedGeometry.width : 640
    height: request && request.requestedGeometry.height > 0 ? request.requestedGeometry.height + bar.height : 480
    x: request && request.requestedGeometry.x > 0 ? request.requestedGeometry.x : 100
    y: request && request.requestedGeometry.y > 0 ? request.requestedGeometry.y : 100
    title: view.title
    visible: true
    color: Kirigami.Theme.backgroundColor
    onClosing: Qt.callLater(() => w.destroy())

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        QQC2.TextField {
            id: bar
            Layout.fillWidth: true
            readOnly: true
            text: view.url
            leftPadding: Kirigami.Units.gridUnit * 1.5
            Kirigami.Icon { source: view.url.toString().startsWith("https") ? "security-high" : "security-low"; width: Kirigami.Units.iconSizes.small; height: width; anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter } }
        }
        WebEngineView {
            // A local file may not read other local files (Chrome's default; Qt's is on).
            settings.localContentCanAccessFileUrls: false
            id: view
            Layout.fillWidth: true
            Layout.fillHeight: true
            profile: w.profile
            onWindowCloseRequested: w.close()
            onNewWindowRequested: request => w.opener.window.openTab("", {request: request, background: false, container: w.opener.container})
            onGeometryChangeRequested: geometry => { w.x = geometry.x; w.y = geometry.y; w.width = geometry.width; w.height = geometry.height + bar.height }
            Component.onCompleted: w.request.openIn(view)
        }
    }
}
