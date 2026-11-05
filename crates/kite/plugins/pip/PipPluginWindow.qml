import QtQuick
import org.kde.kite

// Picture-in-picture in a browser window: its action, and what pages tell it. The
// window it opens is one for the whole browser (Main.qml startPip).
Item {
    id: root
    property var win
    property var app
    property var plugin

    function runAction(id) {
        if (id === "pip.toggle" && win.currentTab) app.startPip(win.currentTab)
    }
    Connections {
        target: root.win
        function onPluginMessage(tab, message) {
            if (message.plugin !== root.plugin.id) return
            if (message.m.cmd === "videoFound") tab.hasVideo = true
            else if (message.m.cmd === "pipEnded" && tab.inPip) root.app.stopPip(true)
        }
    }
    // Turned off while in use: the video goes back to its tab.
    Component.onDestruction: {
        const t = app.pipTab
        if (t && t.window === win) app.stopPip(true)
    }
}
