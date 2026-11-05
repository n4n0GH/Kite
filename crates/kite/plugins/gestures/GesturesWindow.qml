import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kite
import "strokes.js" as Strokes

// Mouse gestures in one browser window. The window's input filter (bootstrap.cpp)
// holds back the button set in win.gestureButton and reports what it does.
Item {
    id: root
    property var win
    property var app
    property var plugin

    property var tracker: null
    property var points: []
    property string stroke: ""
    property bool trail: true
    property var map: ({})

    function buttonCode() {
        // Qt::MiddleButton and Qt::RightButton; none in a page's fullscreen, where
        // the page has the mouse (games, players).
        if (win.contentFullScreen) return 0
        return app.pluginSetting(plugin, "button") === "middle" ? Qt.MiddleButton : Qt.RightButton
    }
    function reload() {
        win.gestureButton = buttonCode()
        trail = !!app.pluginSetting(plugin, "trail")
        map = app.pluginSetting(plugin, "map")
    }
    Component.onCompleted: reload()
    Component.onDestruction: win.gestureButton = 0

    Connections {
        target: Browser
        function onSettingChanged(key) { if (key.startsWith("plugins." + root.plugin.id + ".")) root.reload() }
    }

    function actionName(id) {
        const a = app.shortcutActions.find(x => x.id === id)
        return a ? a.name : ""
    }
    function finish() {
        tracker = null
        points = []
        stroke = ""
        canvas.requestPaint()
    }

    Connections {
        target: root.win
        function onContentFullScreenChanged() { root.reload() }
        function onGestureStarted(x, y) {
            root.tracker = Strokes.tracker(x, y)
            root.points = [Qt.point(x, y)]
            root.stroke = ""
        }
        function onGestureMoved(x, y) {
            if (!root.tracker) return
            root.points.push(Qt.point(x, y))
            root.stroke = Strokes.track(root.tracker, x, y)
            if (root.trail) canvas.requestPaint()
        }
        function onGestureFinished() {
            const action = root.map[Strokes.match(root.map, root.stroke)]
            root.finish()
            if (action) root.win.runAction(action)
        }
        function onGestureCancelled() { root.finish() }
    }

    // Only there during a gesture: the input filter looks for pages under the
    // pointer, and an item over them all would hide them.
    Canvas {
        id: canvas
        parent: root.win.contentItem
        anchors.fill: parent
        z: 1000000
        visible: root.tracker !== null
        onPaint: {
            const c = getContext("2d")
            c.clearRect(0, 0, width, height)
            if (!root.trail || root.points.length < 2) return
            c.strokeStyle = Kirigami.Theme.highlightColor
            c.lineWidth = 3
            c.lineCap = "round"
            c.lineJoin = "round"
            c.beginPath()
            root.points.forEach((p, i) => { const at = mapFromItem(null, p.x, p.y); i ? c.lineTo(at.x, at.y) : c.moveTo(at.x, at.y) })
            c.stroke()
        }

        Rectangle {
            anchors.centerIn: parent
            visible: root.stroke !== ""
            width: hint.implicitWidth + Kirigami.Units.gridUnit * 2
            height: hint.implicitHeight + Kirigami.Units.gridUnit
            radius: Kirigami.Units.cornerRadius
            color: Kirigami.Theme.backgroundColor
            border.color: Qt.alpha(Kirigami.Theme.textColor, 0.2)
            QQC2.Label {
                id: hint
                anchors.centerIn: parent
                readonly property string action: root.actionName(root.map[Strokes.match(root.map, root.stroke)])
                text: Strokes.arrows(root.stroke) + "  " + (action || "No action")
                opacity: action ? 1 : 0.6
            }
        }
    }
}
