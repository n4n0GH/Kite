import QtQuick
import org.kde.kite
import "keys.js" as Rules

// Vim keys in a browser window: what pages ask of it (page.js), and link hints,
// during which it has the keyboard so that the keys never reach a page.
Item {
    id: root
    property var win
    property var app
    property var plugin

    function runAction(id) {
        if (id !== "vim-keys.toggleSite") return
        const tab = win.currentTab
        const site = tab ? Rules.siteOf(tab.url) : ""
        if (!site) {
            win.toast("Vim keys: this isn't a website", plugin.icon)
            return
        }
        const list = app.pluginSetting(plugin, "exceptions")
        // The entry that covers it, which may be of a parent domain.
        const entry = list.find(s => Rules.isExcepted(site, [s]))
        app.setPluginSetting(plugin, "exceptions", entry ? list.filter(s => s !== entry) : list.concat([site]))
        win.toast(entry ? "Vim keys on " + entry + ": on" : "Vim keys on " + site + ": off", plugin.icon)
    }

    // ---- what pages already open have ----
    // Turning on or changing a setting needn't wait for pages to load again, and
    // turning off stops the keys at once.
    function eachOpenTab(code) {
        for (let i = 0; i < win.tabs.count; ++i) {
            const t = win.tabAt(i)
            if (t) t.runInFrames(code)
        }
    }
    function installInOpenTabs() {
        const s = app.pluginScripts().find(x => x.name.startsWith("kite:plugin:" + plugin.id + ":"))
        if (s) eachOpenTab(s.sourceCode)
    }
    Component.onCompleted: installInOpenTabs()
    Component.onDestruction: eachOpenTab("globalThis.__kiteVim && __kiteVim.stop(); delete globalThis.__kiteVim")
    Connections {
        target: Browser
        function onSettingChanged(key) {
            if (key.startsWith("plugins." + root.plugin.id + ".") && !key.endsWith(".enabled")) Qt.callLater(root.installInOpenTabs)
        }
    }

    // ---- what pages ask ----
    Connections {
        target: root.win
        function onPluginMessage(tab, message) {
            if (message.plugin === root.plugin.id && tab === root.win.currentTab) root.handle(tab, message.m)
        }
    }
    function handle(tab, m) {
        if (Rules.WINDOW_ACTIONS.includes(m.cmd)) { win.runAction(m.cmd); return }
        switch (m.cmd) {
        case "moveTabLeft": win.stepCurrentTab(-1); break
        case "moveTabRight": win.stepCurrentTab(1); break
        case "find": win.findFromPage(); break
        case "findNext": win.findStep(false); break
        case "findPrevious": win.findStep(true); break
        case "hints": startHints(tab, false); break
        case "hintsNewTab": startHints(tab, true); break
        case "hintsCancel": endHints(); break
        case "open": bar.newTab = false; bar.toggle(); break
        case "openNewTab": bar.newTab = true; bar.toggle(); break
        case "copyUrl":
            Browser.copyText(tab.url)
            win.toast("Copied " + tab.url, "edit-copy")
            break
        case "toast": win.toast(String(m.text).substring(0, 80), plugin.icon); break
        case "scrollTop": tab.runInMainFrame("globalThis.__kiteVim && __kiteVim.scroll(" + JSON.stringify(String(m.scroll)) + ")"); break
        // As opening a link in the background.
        case "openTab":
            if (/^https?:/.test(m.url)) {
                win.openTab(m.url, {background: true, afterCurrent: true, openerTabId: tab.tabId, container: tab.container})
                backToPage.begin(tab)
            }
            break
        }
    }
    VimBar { id: bar; win: root.win }

    // A tab opened behind this one can leave this page without the keyboard, while
    // Qt still counts it as having it, so that asking for it again does nothing: as
    // it's being made, and as it starts loading. So the keyboard goes away and comes
    // back, a few times over that moment. Not if something is being typed into.
    Timer {
        id: backToPage
        property var tab: null
        property int left: 0
        interval: 250
        repeat: true
        function begin(t) {
            tab = t
            left = 3
            bounce()
            restart()
        }
        function bounce() {
            const f = root.win.activeFocusItem
            if (root.win.currentTab !== tab || tab.devToolsFocused || (f && (f.cursorPosition !== undefined || f.editable === true))) {
                stop()
                return
            }
            keyTaker.forceActiveFocus()
            tab.focusPage()
        }
        onTriggered: {
            bounce()
            if (--left <= 0) stop()
        }
    }

    // ---- link hints ----
    property var hintTab: null
    property var hintFrames: ({})    // label → frame
    property string typed: ""
    property int hintRound: 0
    function startHints(tab, newTab) {
        endHints()
        const round = ++hintRound
        const counts = []
        tab.runInFrames("globalThis.__kiteVim ? __kiteVim.hintsCollect() : 0", (frame, count) => {
            if (count > 0) counts.push({frame, count})
        })
        // Frames answer in their own time; those that don't aren't waited for.
        hintWait.tab = tab
        hintWait.newTab = newTab
        hintWait.round = round
        hintWait.counts = counts
        hintWait.restart()
    }
    Timer {
        id: hintWait
        property var tab
        property bool newTab
        property int round
        property var counts
        interval: 120
        onTriggered: if (round === root.hintRound) root.showHints(tab, newTab, counts)
    }
    function showHints(tab, newTab, counts) {
        const total = counts.reduce((sum, c) => sum + c.count, 0)
        if (!total || win.currentTab !== tab) {
            if (!total) win.toast("Nothing to follow on this page", plugin.icon)
            return
        }
        const labels = Rules.labels(total, app.pluginSetting(plugin, "hintChars"))
        if (!labels.length) {
            win.toast("Vim keys: hint letters need at least two different ones", plugin.icon)
            return
        }
        const frames = {}
        let at = 0
        for (const c of counts) {
            const mine = labels.slice(at, at + c.count)
            at += c.count
            for (const l of mine) frames[l] = c.frame
            c.frame.runJavaScript("__kiteVim.hintsShow(" + JSON.stringify(mine) + ", " + newTab + ")", 1, () => {})
        }
        hintFrames = frames
        hintTab = tab
        typed = ""
        keyTaker.forceActiveFocus()
    }
    function toAll(code) {
        if (hintTab) hintTab.runInFrames(code)
    }
    function endHints() {
        hintRound++
        if (!hintTab) return
        const tab = hintTab
        toAll("globalThis.__kiteVim && __kiteVim.hintsEnd()")
        hintTab = null
        hintFrames = {}
        if (win.currentTab === tab) tab.focusPage()
    }
    function typeHint(letter, shifted) {
        const next = typed + letter
        const labels = Object.keys(hintFrames)
        if (!labels.some(l => l.startsWith(next))) return
        typed = next
        if (labels.includes(next)) {
            const frame = hintFrames[next]
            const tab = hintTab
            toAll("globalThis.__kiteVim && __kiteVim.hintsFilter(" + JSON.stringify(next) + ")")
            hintTab = null
            hintFrames = {}
            // The page has the keyboard before it's clicked: a field can then take it.
            tab.focusPage()
            frame.runJavaScript("__kiteVim.hintsPick(" + JSON.stringify(next) + ", " + shifted + ")", 1, () => {})
            tab.runInFrames("globalThis.__kiteVim && __kiteVim.hintsEnd()")
        } else {
            toAll("globalThis.__kiteVim && __kiteVim.hintsFilter(" + JSON.stringify(next) + ")")
        }
    }
    Item {
        id: keyTaker
        onActiveFocusChanged: if (!activeFocus) root.endHints()
        Keys.onPressed: event => {
            if (!root.hintTab) return
            if (event.key === Qt.Key_Escape) root.endHints()
            else if (event.key === Qt.Key_Backspace) {
                root.typed = root.typed.slice(0, -1)
                root.toAll("globalThis.__kiteVim && __kiteVim.hintsFilter(" + JSON.stringify(root.typed) + ")")
            } else if (event.text.length === 1 && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) root.typeHint(event.text.toLowerCase(), !!(event.modifiers & Qt.ShiftModifier))
            else return
            event.accepted = true
        }
    }
    Connections {
        target: root.win
        function onCurrentTabChanged() { root.endHints() }
    }
}
