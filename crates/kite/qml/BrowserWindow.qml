import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Dialogs
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite

// A browser window (Falkon's BrowserWindow): tab strip, navigation toolbar
// with location bar and extension buttons, bookmarks toolbar, sidebar,
// web views, find bar and status bubble.
QQC2.ApplicationWindow {
    id: bw

    required property var app
    required property int windowId
    property bool privateMode: false
    property alias count: tabModel.count
    property int currentIndex: 0
    property var currentTab: null

    // ---- split view ------------------------------------------------------------
    // Each tab is in a pane ("pane" role): 0 = left, 1 = right. With tabs in
    // both, the window shows them side by side, each pane its own current
    // tab; `currentTab` is the active pane's (the address bar, toolbar and
    // shortcuts act on it).
    property var paneTab: [-1, -1]          // each pane's current tab id
    property int activePane: 0
    property int rightPaneCount: 0          // tabs in the right pane
    property real rightPaneWidth: 0         // as the user dragged it (0: half)
    // Each pane's area for its web views (and the find bar of the active one).
    function paneArea(pane) { return pane === 1 && split ? rightContent : content }
    // Where the right pane begins, for the tab bar's split.
    readonly property real splitPoint: rightContent.x
    readonly property bool split: rightPaneCount > 0
    function paneOf(id) { const i = indexOfTab(id); return i >= 0 ? tabModel.get(i).pane : 0 }
    function setPaneTab(pane, id) { const p = paneTab.slice(); p[pane] = id; paneTab = p }
    function countPane1() { let n = 0; for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).pane === 1) n++; rightPaneCount = n }
    // The tab ids of a pane, in tab order.
    function paneTabs(pane) { const ids = []; for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).pane === pane) ids.push(tabModel.get(i).tabId); return ids }
    // Clicking into a pane makes it the active one.
    function activatePane(pane) {
        if (pane === activePane || paneTab[pane] < 0) return
        setCurrentIndex(indexOfTab(paneTab[pane]))
    }
    // Move a tab to the other pane (starting split view, or ending it when
    // its pane is left empty).
    function moveToOtherPane(id) {
        const i = indexOfTab(id)
        if (i < 0) return
        const from = tabModel.get(i).pane, to = 1 - from
        if (from === 0 && paneTabs(0).length === 1 && !split) return   // nothing would be left
        tabModel.setProperty(i, "pane", to)
        countPane1()
        // The pane it left shows its next tab, or ends.
        const rest = paneTabs(from)
        if (paneTab[from] === id) setPaneTab(from, rest.length ? rest[Math.min(rest.length - 1, Math.max(0, rest.indexOf(id)))] : -1)
        if (rest.length === 0) endSplitIfEmpty()
        setCurrentIndex(indexOfTab(id))
    }
    // A pane without tabs: the right one closes; the left one takes the
    // right one's tabs.
    function endSplitIfEmpty() {
        if (paneTabs(0).length === 0 && split) {
            for (let i = 0; i < tabModel.count; ++i) tabModel.setProperty(i, "pane", 0)
            paneTab = [paneTab[1], -1]
        } else if (paneTabs(1).length === 0) {
            paneTab = [paneTab[0], -1]
        }
        countPane1()
        if (!split) activePane = 0
    }
    // "Split View": a new tab on the right, or back to one pane.
    function toggleSplit() {
        if (split) {
            for (let i = 0; i < tabModel.count; ++i) tabModel.setProperty(i, "pane", 0)
            countPane1()
            paneTab = [currentTab ? currentTab.tabId : paneTab[0], -1]
            activePane = 0
            if (currentTab) setCurrentIndex(indexOfTab(currentTab.tabId))
        } else {
            openTab(app.newTabUrl(), {background: false, pane: 1}).focusLocationBar()
        }
    }
    property var tabItems: ({})
    property var lastActiveTabId: -1
    property string sidebar: ""            // "", "bookmarks", "history", "ext:<id>"
    readonly property string sidebarExtension: sidebar.startsWith("ext:") ? sidebar.substring(4) : ""
    readonly property bool showBookmarksBar: app.liveSettings["appearance.showBookmarksToolbar"]
    property bool contentFullScreen: false
    property var toolbarActions: []
    property int downloadsActive: 0

    width: app.setting("window.width")
    height: app.setting("window.height")
    minimumWidth: 480
    minimumHeight: 320
    title: (currentTab && currentTab.title ? currentTab.title + " — " : "") + "Kite" + (privateMode ? " (Private Browsing)" : "")
    color: Kirigami.Theme.backgroundColor

    onActiveChanged: if (active) app.windowActivated(bw)
    onClosing: close => {
        if (!app.quitting && app.windows.length === 1 && count > 1 && false) close.accepted = false
        for (let i = count - 1; i >= 0; --i) {
            const t = tabById(tabModel.get(i).tabId)
            if (t) Extensions.browserEvent("tabRemoved", JSON.stringify({tabId: t.tabId, windowId: windowId, windowClosing: true}))
        }
        Qt.callLater(() => { app.windowClosed(bw); bw.destroy() })
    }

    // ---- tab model -----------------------------------------------------------
    ListModel { id: tabModel }
    readonly property ListModel tabs: tabModel

    // Toolbar items that register themselves (NavigationToolbar); null if
    // removed from the toolbar.
    property var locationBar: null
    property var extButtons: null
    property var downloadsButton: null
    readonly property ToolbarItems toolbarCatalog: ToolbarItems {}
    function showHistoryMenu(back, item) { historyMenu.showFor(back, item) }
    // The menu button toggles the menu: presses on it don't close the menu
    // (MainMenu's closePolicy), so they'd otherwise close and reopen it.
    function openMainMenu(item) {
        if (mainMenu.opened && mainMenu.parent === item) mainMenu.close()
        else mainMenu.popup(item, 0, item.height)
    }
    function customizeToolbar() { customizeDialog.open() }
    function showAbout() { mainMenu.aboutDialog.open() }

    // Menu bar shown briefly after tapping Alt while it's otherwise hidden.
    // The C++ input filter (bootstrap.cpp) calls altTapped(), and
    // windowPressed() for presses while it is shown: web pages consume
    // presses before QML handlers see them.
    property bool menuBarPeek: false
    function altTapped() { if (!app.liveSettings["appearance.showMenuBar"]) menuBarPeek = !menuBarPeek }
    function windowPressed(x, y) {
        const p = browserMenuBar.mapFromItem(null, x, y)
        if (!browserMenuBar.contains(p)) menuBarPeek = false
    }
    // "top" | "belowToolbar" | "bottom" | "vertical" (on the left) |
    // "verticalRight" (TabStrip.qml)
    readonly property string tabPosition: app.liveSettings["tabs.position"] || "top"
    // The tab bar can hide while the window has a single tab.
    // … but shows while something is dragged over the window, to be
    // dropped on it (TabStrip.qml).
    readonly property bool tabBarShown: !(app.liveSettings["tabs.hideWhenSingle"] && tabModel.count <= 1) || dragOver
    // Something is dragged over the window (the C++ input filter calls
    // dragEntered() and dragLeft()). Cleared a moment after the drag leaves
    // or is dropped, so a drop's new tab is there before the bar would hide.
    property bool dragOver: false
    function dragEntered() { dragOverTimer.stop(); dragOver = true }
    function dragLeft() { dragOverTimer.restart() }
    Timer { id: dragOverTimer; interval: 250; onTriggered: bw.dragOver = false }
    function setTabPosition(p) { Browser.setSetting("tabs.position", JSON.stringify(p)) }

    function registerTab(t) { tabItems[t.tabId] = t }
    function unregisterTab(t) { delete tabItems[t.tabId] }
    function tabById(id) { return tabItems[id] || null }
    function tabAt(i) { return i >= 0 && i < tabModel.count ? tabById(tabModel.get(i).tabId) : null }
    function indexOfTab(id) { for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).tabId === id) return i; return -1 }
    function setTabProperty(id, key, value) { const i = indexOfTab(id); if (i >= 0) tabModel.setProperty(i, key, value) }

    // Where a tab opened from `tab` (a link, its context menu) goes: next to
    // it ("tabs.newTabAfterActive"), or at the end.
    function linkTabIndex(tab) { return app.setting("tabs.newTabAfterActive") ? indexOfTab(tab.tabId) + 1 : undefined }

    function openTab(url, opts) {
        opts = opts || {}
        const id = app.nextTabId++
        let idx = opts.index !== undefined && opts.index >= 0 ? Math.min(opts.index, tabModel.count) : tabModel.count
        if (opts.index === undefined && opts.afterCurrent && app.setting("tabs.newTabAfterActive")) idx = currentIndex + 1
        if (!opts.pinned) { let firstUnpinned = 0; while (firstUnpinned < tabModel.count && tabModel.get(firstUnpinned).pinned) ++firstUnpinned; idx = Math.max(idx, firstUnpinned) }
        // A container tab ("privacy.containers"): its own cookies and storage.
        const container = !privateMode && opts.container && app.containersEnabled && app.containerInfo(opts.container) ? opts.container : ""
        if (container) app.containerProfile(container)
        // Its pane: as asked, its opener's, or the active one.
        const pane = opts.pane !== undefined ? opts.pane : opts.openerTabId !== undefined && opts.openerTabId >= 0 ? paneOf(opts.openerTabId) : activePane
        tabModel.insert(idx, {tabId: id, title: opts.title || "New Tab", url: url || "", icon: "", loading: false, pinned: !!opts.pinned, audible: false, muted: false, container: container, pane: pane})
        countPane1()
        // (showInPane: its pane shows it, even in the background.)
        if (paneTab[pane] < 0 || opts.showInPane) setPaneTab(pane, id)
        const t = tabById(id)
        t.openerTabId = opts.openerTabId !== undefined ? opts.openerTabId : -1
        Extensions.browserEvent("tabCreated", JSON.stringify({tab: t.extInfo()}))
        if (opts.request) opts.request.openIn(t.view)
        else if (opts.lazy) t.pendingUrl = url
        else if (url) t.load(url)
        if (!opts.background || tabModel.count === 1 || (paneTab[pane] === id && pane === activePane)) setCurrentIndex(idx)
        else if (idx <= currentIndex && tabModel.count > 1) currentIndex++
        reindex()
        return t
    }

    function restoreTab(saved) {
        const t = openTab(saved.url, {background: true, lazy: true, pinned: saved.pinned, title: saved.title, container: saved.container, pane: saved.pane === 1 ? 1 : 0})
        t.restoreLook(saved.title, saved.url)
        if (saved.zoom > 0) t.view.zoomFactor = saved.zoom
        return t
    }

    function closeTab(index) {
        if (index < 0 || index >= tabModel.count) return
        const t = tabAt(index)
        if (t && !privateMode) Browser.pushClosedTab(JSON.stringify(t.saveState()))
        const id = tabModel.get(index).tabId
        const pane = tabModel.get(index).pane
        const wasCurrent = index === currentIndex
        const wasPaneCurrent = paneTab[pane] === id
        // The tab to show next in its pane: the opener (if it is there),
        // else the neighbour.
        const before = paneTabs(pane)
        const at = before.indexOf(id)
        let next = before.length > 1 ? before[at < before.length - 1 ? at + 1 : at - 1] : -1
        if (t && t.openerTabId >= 0 && before.includes(t.openerTabId)) next = t.openerTabId
        tabModel.remove(index)
        countPane1()
        Extensions.browserEvent("tabRemoved", JSON.stringify({tabId: id, windowId: windowId, windowClosing: false}))
        if (tabModel.count === 0) {
            if (app.setting("tabs.closeWindowWithLastTab") || app.windows.length > 1) { bw.close(); return }
            paneTab = [-1, -1]
            openTab(app.newTabUrl(), {background: false, pane: 0})
            return
        }
        if (wasPaneCurrent) setPaneTab(pane, next)
        if (next < 0) endSplitIfEmpty()
        if (wasCurrent) {
            currentIndex = -1
            setCurrentIndex(indexOfTab(paneTab[activePane] >= 0 ? paneTab[activePane] : paneTab[0]))
        } else {
            currentIndex = currentTab ? indexOfTab(currentTab.tabId) : 0
        }
        reindex()
        currentTab = tabAt(currentIndex)
    }

    function closeTabById(id) { closeTab(indexOfTab(id)) }

    function moveTab(from, to) {
        if (from === to || from < 0 || to < 0 || to >= tabModel.count) return
        const cur = tabModel.get(currentIndex).tabId
        const id = tabModel.get(from).tabId
        tabModel.move(from, to, 1)
        currentIndex = indexOfTab(cur)
        Extensions.browserEvent("tabMoved", JSON.stringify({tabId: id, windowId: windowId, fromIndex: from, toIndex: to}))
        reindex()
    }

    function reindex() {
        const order = []
        for (let i = 0; i < tabModel.count; ++i) order.push(tabModel.get(i).tabId)
        Extensions.browserEvent("tabsReindexed", JSON.stringify({windowId: windowId, order: order}))
    }

    function setCurrentIndex(i) {
        if (i < 0 || i >= tabModel.count) return
        const prev = currentTab ? currentTab.tabId : -1
        const row = tabModel.get(i)
        // The tab's pane shows it and becomes the active one.
        setPaneTab(row.pane, row.tabId)
        activePane = row.pane
        currentIndex = i
        currentTab = tabAt(i)
        if (!currentTab) return
        currentTab.activated()
        if (currentTab.tabId !== prev) {
            lastActiveTabId = prev
            Extensions.browserEvent("tabActivated", JSON.stringify({tabId: currentTab.tabId, windowId: windowId, previousTabId: prev}))
            refreshToolbar()
        }
    }

    function tabModel_get(i) { return tabModel.get(i) }
    // After a session's tabs are back: each pane's current tab.
    function restorePanes(saved) {
        if (saved.splitWidth > 0) rightPaneWidth = saved.splitWidth
        const pc = saved.paneCurrent || []
        for (const pane of [0, 1]) {
            const t = tabAt(pc[pane])
            if (t && tabModel.get(pc[pane]).pane === pane) setPaneTab(pane, t.tabId)
            else { const ids = paneTabs(pane); setPaneTab(pane, ids.length ? ids[0] : -1) }
        }
        setCurrentIndex(Math.min(saved.current || 0, count - 1))
    }
    function focusLocationBarNow() { if (locationBar) locationBar.focusAndSelect() }
    function findResult(count, active) { findBar.matches = count; findBar.activeMatch = active }

    // Pinned tabs stay in front of unpinned ones.
    // Nearest index to `to` that the tab at `from` may move to: pinned tabs
    // stay before unpinned ones.
    function clampToSection(from, to) {
        let pinned = 0
        for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).pinned) pinned++
        return tabModel.get(from).pinned ? Math.min(to, pinned - 1) : Math.max(to, pinned)
    }

    // Ctrl+Tab and friends: within the active pane.
    function nextTab(step) {
        const ids = paneTabs(activePane)
        if (!ids.length) return
        const at = Math.max(0, ids.indexOf(currentTab ? currentTab.tabId : -1))
        setCurrentIndex(indexOfTab(ids[(at + step + ids.length) % ids.length]))
    }

    function reopenClosedTab(i) {
        const saved = JSON.parse(Browser.popClosedTab(i || 0))
        if (saved && saved.url) { const t = openTab(saved.url, {background: false, pinned: saved.pinned, container: saved.container}); return t }
        return null
    }

    function closeOtherTabs(keepIndex) { for (let i = tabModel.count - 1; i >= 0; --i) if (i !== keepIndex && !tabModel.get(i).pinned) closeTab(i) }
    function closeTabsToRight(index) { for (let i = tabModel.count - 1; i > index; --i) if (!tabModel.get(i).pinned) closeTab(i) }

    function saveState() {
        const tabs = []
        for (let i = 0; i < tabModel.count; ++i) { const t = tabAt(i); if (t) tabs.push(t.saveState()) }
        // (Split view: each pane's current tab, and the right pane's width.)
        return {tabs: tabs, current: currentIndex, paneCurrent: [indexOfTab(paneTab[0]), indexOfTab(paneTab[1])], splitWidth: rightPaneWidth,
                geometry: [x, y, width, height], maximized: visibility === Window.Maximized}
    }

    function extInfo(populate) {
        const info = {id: windowId, focused: active, top: y, left: x, width: width, height: height, incognito: privateMode, type: "normal",
                      state: visibility === Window.Maximized ? "maximized" : visibility === Window.Minimized ? "minimized" : visibility === Window.FullScreen ? "fullscreen" : "normal",
                      alwaysOnTop: false, title: title}
        if (populate) { info.tabs = []; for (let i = 0; i < tabModel.count; ++i) info.tabs.push(tabAt(i).extInfo()) }
        return info
    }

    function toast(message, icon) { toastLabel.text = message; toastIcon.source = icon; toastBox.opacity = 1; toastTimer.restart() }
    function downloadStarted() { downloadsActive++; if (downloadsButton) downloadsButton.pulse() }

    function showSidebar(which) { sidebar = (sidebar === which ? "" : which) }

    function printTab() { pdfDialog.open() }

    // ---- extension toolbar -------------------------------------------------------
    function refreshToolbar() { toolbarActions = JSON.parse(Extensions.toolbar(currentTab ? currentTab.tabId : -1)) }
    Connections {
        target: Extensions
        function onChanged(what) { if (what === "actions" || what === "extensions") bw.refreshToolbar() }
    }

    function openFile() { openDialog.open() }
    function savePage() { saveDialog.open() }
    function openFindBar() { findBar.open() }
    function toggleBookmarkFromMenu() { if (locationBar) locationBar.toggleBookmark() }
    // used by bookmark menus in the main menu
    property QtObject bookmarksOpener: QtObject {
        property var win: bw
        function open(url, mods) { if (bw.currentTab) bw.currentTab.load(url) }
    }

    function openPageActionPopup(action, anchor) { extPopup.show(action, anchor) }
    // Closes the extension popup if it is open at `anchor`; true if it was.
    function closeExtensionPopupAt(anchor) {
        if (!extPopup.opened || extPopup.anchorItem !== anchor) return false
        extPopup.close()
        return true
    }

    function openExtensionPopup(extId) {
        const a = toolbarActions.find(x => x.ext === extId && x.kind === "browserAction")
        if (!a) return
        // Under its button, or the address bar when extensions aren't on the toolbar.
        if (a.popup) extPopup.show(a, (extButtons && extButtons.buttonFor(extId)) || locationBar)
        else Extensions.actionClicked(extId, a.kind, currentTab ? currentTab.tabId : -1)
    }

    // ---- shortcuts ------------------------------------------------------------------
    Shortcut { sequences: [StandardKey.AddTab]; onActivated: bw.openTab(app.newTabUrl(), {background: false}).focusLocationBar() }
    Shortcut { sequences: [StandardKey.Close]; onActivated: bw.closeTab(bw.currentIndex) }
    Shortcut { sequence: "Ctrl+Shift+T"; onActivated: bw.reopenClosedTab(0) }
    Shortcut { sequence: "Ctrl+N"; onActivated: app.newWindow() }
    Shortcut { sequence: "Ctrl+Shift+P"; onActivated: app.newWindow({privateMode: true}) }
    Shortcut { sequences: ["Ctrl+L", "F6", "Alt+D"]; onActivated: if (locationBar) locationBar.focusAndSelect() }
    Shortcut { sequence: "Ctrl+K"; onActivated: if (locationBar) { locationBar.focusAndSelect(); locationBar.text = "? " ; locationBar.cursorPosition = 2 } }
    Shortcut { sequences: ["Ctrl+R", "F5"]; onActivated: if (bw.currentTab) bw.currentTab.view.reload() }
    Shortcut { sequences: ["Ctrl+Shift+R", "Ctrl+F5"]; onActivated: if (bw.currentTab) bw.currentTab.view.reloadAndBypassCache() }
    // Only while the find bar is open: otherwise Escape must reach the page
    // (web apps and extensions such as Vimium rely on it).
    Shortcut { sequence: "Esc"; enabled: findBar.visible; onActivated: findBar.close() }
    Shortcut { sequences: [StandardKey.Back, "Alt+Left"]; onActivated: if (bw.currentTab) bw.currentTab.view.goBack() }
    Shortcut { sequences: [StandardKey.Forward, "Alt+Right"]; onActivated: if (bw.currentTab) bw.currentTab.view.goForward() }
    Shortcut { sequence: "Alt+Home"; onActivated: if (bw.currentTab) bw.currentTab.load(app.homeUrl()) }
    Shortcut { sequences: [StandardKey.Find]; onActivated: findBar.open() }
    Shortcut { sequences: [StandardKey.FindNext, "F3"]; onActivated: findBar.find(false) }
    Shortcut { sequences: [StandardKey.FindPrevious, "Shift+F3"]; onActivated: findBar.find(true) }
    Shortcut { sequence: "Ctrl+D"; onActivated: if (locationBar) locationBar.toggleBookmark() }
    Shortcut { sequence: "Ctrl+Shift+D"; onActivated: bw.bookmarkAllTabs() }
    Shortcut { sequence: "Ctrl+H"; onActivated: bw.showSidebar("history") }
    Shortcut { sequence: "Ctrl+B"; onActivated: bw.showSidebar("bookmarks") }
    Shortcut { sequence: "Ctrl+Shift+O"; onActivated: app.showLibrary("bookmarks") }
    Shortcut { sequence: "Ctrl+Shift+H"; onActivated: app.showLibrary("history") }
    Shortcut { sequences: ["Ctrl+Shift+Y", "Ctrl+J"]; onActivated: app.showDownloads() }
    Shortcut { sequence: "Ctrl+Shift+A"; onActivated: app.showAddons() }
    Shortcut { sequences: [StandardKey.Preferences, "Ctrl+,"]; onActivated: app.showPreferences() }
    Shortcut { sequences: ["Ctrl+Tab", "Ctrl+PgDown"]; onActivated: bw.nextTab(1) }
    Shortcut { sequences: ["Ctrl+Shift+Tab", "Ctrl+PgUp"]; onActivated: bw.nextTab(-1) }
    Shortcut { sequences: [StandardKey.ZoomIn, "Ctrl+="]; onActivated: if (bw.currentTab) bw.currentTab.zoomStep(1) }
    Shortcut { sequences: [StandardKey.ZoomOut]; onActivated: if (bw.currentTab) bw.currentTab.zoomStep(-1) }
    Shortcut { sequence: "Ctrl+0"; onActivated: if (bw.currentTab) bw.currentTab.setZoom(app.setting("appearance.defaultZoom") / 100) }
    Shortcut { sequence: "F11"; onActivated: bw.visibility === Window.FullScreen ? bw.showNormal() : bw.showFullScreen() }
    Shortcut { sequences: ["F12", "Ctrl+Shift+I"]; onActivated: if (bw.currentTab) bw.currentTab.showDevTools() }
    Shortcut { sequence: "Ctrl+U"; onActivated: if (bw.currentTab) bw.openTab("view-source:" + bw.currentTab.url, {background: false, afterCurrent: true}) }
    Shortcut { sequences: [StandardKey.Print]; onActivated: bw.printTab() }
    Shortcut { sequences: [StandardKey.Save]; onActivated: saveDialog.open() }
    Shortcut { sequences: [StandardKey.Open]; onActivated: openDialog.open() }
    Shortcut { sequences: [StandardKey.Quit]; onActivated: app.quit() }
    Shortcut { sequence: "Ctrl+M"; onActivated: if (bw.currentTab) bw.currentTab.view.audioMuted = !bw.currentTab.view.audioMuted }
    Repeater {
        model: 9
        delegate: Item {
            required property int index
            Shortcut { sequence: "Alt+" + (index + 1); onActivated: bw.setCurrentIndex(index === 8 ? bw.count - 1 : index) }
        }
    }
    // Extension keyboard commands
    property var extCommands: JSON.parse(Extensions.commands())
    Connections { target: Extensions; function onRevisionChanged() { bw.extCommands = JSON.parse(Extensions.commands()) } }
    Instantiator {
        model: bw.extCommands
        delegate: Shortcut {
            required property var modelData
            sequence: modelData.shortcut.replace("Command", "Ctrl").replace("MacCtrl", "Ctrl")
            onActivated: {
                const tabId = bw.currentTab ? bw.currentTab.tabId : -1
                if (modelData.name === "_execute_browser_action" || modelData.name === "_execute_action") bw.openExtensionPopup(modelData.ext)
                else if (modelData.name === "_execute_sidebar_action") bw.showSidebar("ext:" + modelData.ext)
                else Extensions.command(modelData.ext, modelData.name, tabId)
            }
        }
    }

    function bookmarkAllTabs() {
        const roots = JSON.parse(Browser.bookmarkRoots())
        const folder = Browser.addFolder(roots.unsorted, "Tabs " + new Date().toLocaleString(Qt.locale(), Locale.ShortFormat))
        for (let i = 0; i < tabModel.count; ++i) { const t = tabAt(i); if (t && t.url) Browser.addBookmark(t.url, t.title, folder) }
        toast("Bookmarked " + tabModel.count + " tabs", "bookmark-new")
    }

    // ---- layout ------------------------------------------------------------------------
    header: ColumnLayout {
        spacing: 0
        visible: !bw.contentFullScreen

        BrowserMenuBar {
            id: browserMenuBar
            Layout.fillWidth: true
            win: bw
            visible: bw.app.liveSettings["appearance.showMenuBar"] || bw.menuBarPeek
            onMenuClosed: bw.menuBarPeek = false
        }

        TabStripSlot {
            Layout.fillWidth: true
            browser: bw
            position: "top"
            inUse: bw.tabPosition === "top"
            shown: bw.tabBarShown
        }

        // Navigation toolbar (customizable: "toolbar.items")
        NavigationToolbar {
            Layout.fillWidth: true
            win: bw
            catalog: bw.toolbarCatalog
        }

        // The "Bookmarks Toolbar" folder ("appearance.showBookmarksToolbar")
        BookmarksBar {
            Layout.fillWidth: true
            visible: bw.showBookmarksBar
            win: bw
        }

        TabStripSlot {
            Layout.fillWidth: true
            browser: bw
            position: "belowToolbar"
            inUse: bw.tabPosition === "belowToolbar"
            shown: bw.tabBarShown
        }
    }

    footer: TabStripSlot {
        browser: bw
        position: "bottom"
        inUse: bw.tabPosition === "bottom" && !bw.contentFullScreen
        shown: bw.tabBarShown
    }

    // Split view: a line along the top of the active pane.
    component PaneMarker: Rectangle {
        property int pane
        z: 2
        visible: bw.split && bw.activePane === pane
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 2
        color: Kirigami.Theme.highlightColor
    }

    // Vertical tab list, left or right of the page: slides in from its side
    // when shown (and out when hidden), at the width the user last gave it.
    component SideTabs: Item {
        id: sideTabs
        property string side   // "left" | "right"
        // Split view lists each pane's tabs on its side; else the list is on
        // the chosen side.
        readonly property bool verticalTabs: bw.tabPosition === "vertical" || bw.tabPosition === "verticalRight"
        readonly property bool inUse: !bw.contentFullScreen && (bw.split ? verticalTabs : bw.tabPosition === (side === "left" ? "vertical" : "verticalRight"))
        readonly property bool shown: inUse && bw.tabBarShown
        property real savedWidth: Kirigami.Units.gridUnit * 14
        property real shownWidth: shown ? savedWidth : 0
        Behavior on shownWidth { NumberAnimation { id: sideTabsAnimation; duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic } }
        visible: inUse && shownWidth > 0
        clip: true
        QQC2.SplitView.preferredWidth: shownWidth
        QQC2.SplitView.minimumWidth: shown && !sideTabsAnimation.running ? Kirigami.Units.gridUnit * 8 : 0
        // Remember the width the user drags it to (not the window's own
        // layout changes, e.g. while it opens).
        onWidthChanged: if (shown && splitView.resizing && width > 0) savedWidth = width
        Loader {
            active: sideTabs.inUse
            width: sideTabs.savedWidth
            height: sideTabs.height
            // Stays at the window's edge while the list slides in or out.
            x: sideTabs.side === "left" ? sideTabs.width - width : 0
            sourceComponent: TabStrip { browser: bw; position: sideTabs.side === "left" ? "vertical" : "verticalRight"; pane: bw.split ? (sideTabs.side === "left" ? 0 : 1) : -1 }
        }
    }

    QQC2.SplitView {
        id: splitView
        anchors.fill: parent
        orientation: Qt.Horizontal

        SideTabs { side: "left" }

        Sidebar {
            id: sidebarPanel
            visible: bw.sidebar !== "" && !bw.contentFullScreen
            QQC2.SplitView.preferredWidth: Kirigami.Units.gridUnit * 16
            QQC2.SplitView.minimumWidth: Kirigami.Units.gridUnit * 10
            win: bw
            mode: bw.sidebar
        }

        // The left pane (the only one without split view).
        Item {
            id: content
            QQC2.SplitView.fillWidth: true
            QQC2.SplitView.minimumWidth: Kirigami.Units.gridUnit * 12
            Repeater {
                model: tabModel
                // Each tab's view sits in its pane's area; each pane shows its
                // current tab.
                delegate: WebTab {
                    required property int index
                    tabIndex: index
                    window: bw
                    app: bw.app
                    parent: bw.paneArea(pane)
                    width: parent ? parent.width : 0
                    height: parent ? parent.height - (findBar.visible && findBar.parent === parent ? findBar.height : 0) : 0
                    visible: tabId === bw.paneTab[pane]
                }
            }

            FindBar {
                id: findBar
                parent: bw.paneArea(bw.activePane)
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                tab: bw.currentTab
            }

            // Hovered link
            Rectangle {
                id: statusBubble
                parent: bw.paneArea(bw.activePane)
                z: 1
                property string text: bw.currentTab ? bw.currentTab.hoveredLink : ""
                visible: text !== "" && app.liveSettings["appearance.showStatusBar"]
                anchors { left: parent.left; bottom: findBar.visible ? findBar.top : parent.bottom }
                width: Math.min(statusText.implicitWidth + Kirigami.Units.largeSpacing, parent.width * 0.6)
                height: statusText.implicitHeight + Kirigami.Units.smallSpacing * 2
                color: Kirigami.Theme.backgroundColor
                border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
                QQC2.Label {
                    id: statusText
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.largeSpacing
                    text: statusBubble.text
                    elide: Text.ElideMiddle
                    font: Kirigami.Theme.smallFont
                }
            }

            PaneMarker { pane: 0 }

            // Toasts
            Rectangle {
                id: toastBox
                opacity: 0
                visible: opacity > 0
                Behavior on opacity { NumberAnimation { duration: Kirigami.Units.longDuration } }
                anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Kirigami.Units.gridUnit * 2 }
                width: Math.min(toastRow.implicitWidth + Kirigami.Units.gridUnit * 2, parent.width - Kirigami.Units.gridUnit * 2)
                height: toastRow.implicitHeight + Kirigami.Units.largeSpacing * 2
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.textColor
                RowLayout {
                    id: toastRow
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.gridUnit * 2
                    Kirigami.Icon { id: toastIcon; visible: source !== ""; Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium; Layout.preferredHeight: Layout.preferredWidth }
                    QQC2.Label { id: toastLabel; color: Kirigami.Theme.backgroundColor; wrapMode: Text.Wrap; Layout.fillWidth: true }
                }
                Timer { id: toastTimer; interval: 4000; onTriggered: toastBox.opacity = 0 }
                TapHandler { onTapped: toastBox.opacity = 0 }
            }
        }

        // The right pane (split view).
        Item {
            id: rightContent
            visible: bw.split
            QQC2.SplitView.preferredWidth: bw.rightPaneWidth > 0 ? bw.rightPaneWidth : (content.width + width) / 2
            QQC2.SplitView.minimumWidth: Kirigami.Units.gridUnit * 12
            // Remember the width the user drags it to.
            onWidthChanged: if (bw.split && splitView.resizing && width > 0) bw.rightPaneWidth = width
            PaneMarker { pane: 1 }
        }

        SideTabs { side: "right" }
    }

    // ---- popups & menus -----------------------------------------------------------------
    ExtensionPopup { id: extPopup; win: bw; profile: bw.app.profile }

    QQC2.Menu {
        id: historyMenu
        property var historyModel: null
        function showFor(back, anchor) {
            const t = bw.currentTab; if (!t) return
            historyModel = back ? t.view.history.backItems : t.view.history.forwardItems
            popup(anchor, 0, anchor.height)
        }
        Instantiator {
            model: historyMenu.historyModel
            delegate: QQC2.MenuItem {
                required property string title
                required property url url
                required property int offset
                text: title || url
                onTriggered: bw.currentTab.view.goBackOrForward(offset)
            }
            onObjectAdded: (i, o) => historyMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => historyMenu.removeItem(o)
        }
    }

    MainMenu { id: mainMenu; win: bw; app: bw.app }
    CustomizeToolbarDialog { id: customizeDialog; win: bw; catalog: bw.toolbarCatalog }

    FileDialog {
        id: openDialog
        title: "Open File"
        onAccepted: bw.openTab(selectedFile.toString(), {background: false})
    }
    FileDialog {
        id: saveDialog
        title: "Save Page As"
        fileMode: FileDialog.SaveFile
        nameFilters: ["Web page, complete (*.html)", "Web page, HTML only (*.html)", "Single file (*.mhtml)"]
        currentFile: "file://" + app.setting("downloads.directory") + "/" + ((bw.currentTab ? bw.currentTab.title : "page").replace(/[\/\\:*?"<>|]/g, "_") || "page") + ".html"
        onAccepted: {
            const fmt = selectedNameFilter.index === 2 ? WebEngineDownloadRequest.MimeHtmlSaveFormat
                      : selectedNameFilter.index === 1 ? WebEngineDownloadRequest.SingleHtmlSaveFormat : WebEngineDownloadRequest.CompleteHtmlSaveFormat
            bw.currentTab.view.save(selectedFile.toString().replace("file://", ""), fmt)
        }
    }
    FileDialog {
        id: pdfDialog
        title: "Print to PDF"
        fileMode: FileDialog.SaveFile
        nameFilters: ["PDF (*.pdf)"]
        currentFile: "file://" + app.setting("downloads.directory") + "/" + ((bw.currentTab ? bw.currentTab.title : "page").replace(/[\/\\:*?"<>|]/g, "_") || "page") + ".pdf"
        onAccepted: {
            const path = selectedFile.toString().replace("file://", "")
            bw.currentTab.view.printToPdf(path)
            bw.toast("Saved " + path, "document-save")
        }
    }

    Component.onCompleted: {
        if (app.setting("window.maximized")) showMaximized()
    }
}
