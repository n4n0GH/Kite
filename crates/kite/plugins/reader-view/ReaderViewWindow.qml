import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kite

// Reader View in one browser window: its action, and a part in each tab
// (ReaderViewTab.qml) that tells whether the page looks like an article and
// puts the controls over one in reader view.
//
// The article is taken from the page by Readability, in a world of its own, and
// shown by Kite at a kite:reader address (kite_core::reader, which cleans it up:
// the page is on a privileged origin). Tabs show the page it came from.
Item {
    id: root
    property var win
    property var app
    property var plugin

    // (Readability's own world: it needn't see Kite's or anyone else's scripts.)
    readonly property int world: 250
    // Read when first needed: Readability is large.
    property string extractScript: ""
    property string probeScript: ""
    function file(name) { return Browser.pluginFile("plugins/reader-view/" + name) }

    function runAction(id) {
        if (id === "reader-view.toggle" && win.currentTab) toggle(win.currentTab)
    }

    function toggle(tab) {
        if (tab.isReader) {
            if (tab.view.canGoBack) tab.view.goBack()
            else tab.load(tab.url)
            return
        }
        if (!tab.canRead()) return
        if (!extractScript) extractScript = "(() => {" + file("readability/Readability.js") + "\n" + file("extract.js") + "\n})()"
        const source = tab.url
        tab.view.runJavaScript(extractScript, world, json => {
            if (tab.url !== source) return   // gone elsewhere meanwhile
            const t = Kirigami.Theme
            const page = json ? Browser.readerOpen(source, json, t.backgroundColor.toString(), t.textColor.toString(), t.linkColor.toString()) : ""
            if (page === "") root.win.toast("Kite found no article on this page", "dialog-information")
            else tab.view.url = page
        })
    }

    // Whether the page looks like an article: the address bar then offers reader view.
    function probe(tab) {
        if (!tab.visible || tab.isReader || tab.discarded || !/^https?:/i.test(tab.view.url.toString())) return
        if (!probeScript) probeScript = "(() => {" + file("readability/Readability-readerable.js") + "\nreturn isProbablyReaderable(document);})()"
        const page = tab.view.url.toString()
        tab.view.runJavaScript(probeScript, world, ok => { if (tab.view.url.toString() === page) tab.readerAvailable = ok === true })
    }

    // A change of look reaches pages already in reader view (keeping where they are).
    Connections {
        target: Browser
        function onSettingChanged(key) {
            if (!key.startsWith("plugins.reader-view.") || key.endsWith(".enabled")) return
            for (const id in root.parts) {
                const tab = root.parts[id] && root.parts[id].tab
                if (tab && tab.isReader) tab.view.runJavaScript(Browser.readerApplyScript(), root.world)
            }
        }
    }

    // ---- each tab's part ----
    property var parts: ({})
    function attach(tab) {
        if (!tab || parts[tab.tabId]) return
        parts[tab.tabId] = partComponent.createObject(tab.pageArea, {tab: tab, reader: root})
    }
    Component { id: partComponent; ReaderViewTab {} }
    Connections {
        target: root.win
        function onTabOpened(tab) { root.attach(tab) }
        function onTabClosed(tabId) { delete root.parts[tabId] }
    }
    Component.onCompleted: { for (const id in win.tabItems) attach(win.tabItems[id]) }
    // Off: nothing of it is left in the tabs.
    Component.onDestruction: {
        for (const id in parts) {
            const part = parts[id]
            if (!part) continue
            if (part.tab) part.tab.readerAvailable = false
            part.destroy()
        }
    }
}
