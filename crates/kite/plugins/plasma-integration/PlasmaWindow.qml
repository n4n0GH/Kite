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
            if (message.plugin !== root.plugin.id) return
            if (message.m.cmd === "share") {
                const frame = JSON.stringify(String(message.m.frame))
                const id = tab.tabId
                root.share(message.m.data, error => {
                    const still = root.win.tabById(id)
                    if (still) still.runInFrames("globalThis.__kitePlasmaShare && __kitePlasmaShare.done(" + frame + ", " + JSON.stringify(error) + ")")
                })
                return
            }
            if (message.m.cmd !== "media" || !root.reportsAllowed()) return
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
    // ---- sharing, with KDE's Purpose --------------------------------------------------
    // `done` gets what to tell whoever asked: "", or the name of the error.
    property var shareDialog: null
    function share(data, done) {
        if (!setting("share")) return done("NotAllowedError")
        if (shareDialog) return done("InvalidStateError")
        const component = Qt.createComponent(app.pluginUrl(plugin, "plugins/plasma-integration/ShareDialog.qml"))
        if (component.status !== Component.Ready) {
            console.warn("Sharing isn't available: " + component.errorString())
            win.toast("Sharing isn't available: KDE's Purpose isn't installed", "dialog-warning")
            return done("NotSupportedError")
        }
        // Purpose only knows addresses, and takes a title as well. Text goes as one.
        const urls = (data.url ? [data.url] : []).concat(data.text ? [data.text] : [])
        const dialog = component.createObject(root, {
            transientParent: win,
            inputData: {title: data.title || data.url || data.text, urls: urls, mimeType: data.text ? "text/plain" : "*/*"},
        })
        shareDialog = dialog
        dialog.done.connect((error, message) => {
            shareDialog = null
            if (message) win.toast("Couldn't share: " + message, "dialog-warning")
            done(error)
            dialog.destroy()
        })
        dialog.show()
        dialog.requestActivate()
    }
    readonly property var shareable: /^https?:/i
    function shareItems(info) {
        if (!setting("share")) return []
        const items = []
        if (info.linkUrl && shareable.test(info.linkUrl)) items.push({title: "Share Link…", data: {title: info.linkText || info.linkUrl, url: info.linkUrl}})
        if (info.srcUrl && info.mediaType && shareable.test(info.srcUrl)) items.push({title: "Share " + {image: "Image", video: "Video", audio: "Audio"}[info.mediaType] + "…", data: {url: info.srcUrl}})
        if (!info.linkUrl && !info.srcUrl && !info.selectionText && !info.editable && shareable.test(info.pageUrl)) items.push({title: "Share Page…", data: {title: win.currentTab ? win.currentTab.title : "", url: info.pageUrl}})
        return items.map(i => ({title: i.title, iconName: "document-share", run: () => root.share(i.data, () => {})}))
    }

    // For the page's context menu: what it could send, and where to.
    function contextItems(info) {
        return shareItems(info).concat(deviceItems(info))
    }
    function deviceItems(info) {
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
