import QtQuick
import org.kde.kite

// Plasma Integration in a browser window. The desktop sees one player for the whole
// browser, kept in kite_core::plasma; here is what this window's tabs tell it, and
// what it asks of them.
Item {
    id: root
    property var win
    property var app
    property var plugin

    function setting(key) { return !!app.pluginSetting(plugin, key) }
    // This window's tabs the desktop has been told about.
    property var told: ({})

    function reportsAllowed() { return setting("mediaControls") && (!win.privateMode || setting("privateWindows")) }
    function forgetAll() {
        for (const id of Object.keys(told)) Browser.plasmaForgetTab(Number(id))
        told = {}
    }
    Connections {
        target: root.win
        function onPluginMessage(tab, message) {
            if (message.plugin !== root.plugin.id || message.m.cmd !== "media" || !root.reportsAllowed()) return
            const state = message.m.state
            if (state) {
                state.title = state.title || tab.title
                state.url = tab.url
                root.told[tab.tabId] = true
            }
            Browser.plasmaMedia(tab.tabId, String(message.m.frame), JSON.stringify(state))
        }
        function onTabClosed(tabId) {
            if (!root.told[tabId]) return
            delete root.told[tabId]
            Browser.plasmaForgetTab(tabId)
        }
    }
    Connections {
        target: Browser
        function onPlasmaCommand(json) {
            const c = JSON.parse(json)
            const tab = root.win.tabById(c.tab)
            if (!tab) return
            // Its page was put away: nothing there to control any more.
            if (tab.unloaded) {
                delete root.told[c.tab]
                Browser.plasmaForgetTab(c.tab)
            } else if (c.cmd === "raise") {
                root.win.show()
                root.win.raise()
                root.win.requestActivate()
                root.win.setCurrentIndex(root.win.indexOfTab(c.tab))
            } else {
                tab.runInFrames("globalThis.__kitePlasma && __kitePlasma.command(" + JSON.stringify(c.frame) + ", " + JSON.stringify(c.cmd) + ", " + Number(c.value) + ")")
            }
        }
        function onSettingChanged(key) {
            if ((key.endsWith(".mediaControls") || key.endsWith(".privateWindows")) && key.startsWith("plugins." + root.plugin.id + ".") && !root.reportsAllowed()) root.forgetAll()
        }
    }

    // Devices come and go. (The daemon isn't asked while this is off, or started.)
    Timer {
        interval: 30000
        repeat: true
        running: root.setting("kdeConnect")
        triggeredOnStart: true
        onTriggered: Browser.plasmaRefreshDevices()
    }
    // For the page's context menu: what it could send, and where to.
    function contextItems(info) {
        if (!setting("kdeConnect")) return []
        const devices = JSON.parse(Browser.plasmaDevices())
        Browser.plasmaRefreshDevices()
        if (devices.length === 0) return []
        const sends = []
        if (info.linkUrl) sends.push({title: info.linkUrl.startsWith("tel:") ? "Call on Device" : "Send Link to Device", url: info.linkUrl})
        if (info.srcUrl && info.mediaType) sends.push({title: "Send " + {image: "Image", video: "Video", audio: "Audio"}[info.mediaType] + " to Device", url: info.srcUrl})
        if (!info.linkUrl && !info.srcUrl && !info.selectionText && !info.editable) sends.push({title: "Send Page to Device", url: info.pageUrl})
        return sends.filter(s => Browser.plasmaCanSend(s.url)).map(s => ({
            title: s.title,
            iconName: "kdeconnect",
            children: devices.map(d => ({title: d.name, iconName: d.icon, run: () => {
                Browser.plasmaShare(d.id, s.url)
                root.win.toast("Sent to " + d.name, "kdeconnect")
            }})),
        }))
    }
    Component.onDestruction: forgetAll()
}
