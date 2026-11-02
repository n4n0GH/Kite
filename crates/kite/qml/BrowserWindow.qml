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
    property var tabItems: ({})
    property var lastActiveTabId: -1
    property string sidebar: ""            // "", "bookmarks", "history", "ext:<id>"
    readonly property string sidebarExtension: sidebar.startsWith("ext:") ? sidebar.substring(4) : ""
    property bool showBookmarksBar: app.setting("appearance.showBookmarksToolbar")
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

    function registerTab(t) { tabItems[t.tabId] = t }
    function unregisterTab(t) { delete tabItems[t.tabId] }
    function tabById(id) { return tabItems[id] || null }
    function tabAt(i) { return i >= 0 && i < tabModel.count ? tabById(tabModel.get(i).tabId) : null }
    function indexOfTab(id) { for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).tabId === id) return i; return -1 }
    function setTabProperty(id, key, value) { const i = indexOfTab(id); if (i >= 0) tabModel.setProperty(i, key, value) }

    function openTab(url, opts) {
        opts = opts || {}
        const id = app.nextTabId++
        let idx = opts.index !== undefined && opts.index >= 0 ? Math.min(opts.index, tabModel.count) : tabModel.count
        if (opts.index === undefined && opts.afterCurrent && app.setting("tabs.newTabAfterActive")) idx = currentIndex + 1
        if (!opts.pinned) { let firstUnpinned = 0; while (firstUnpinned < tabModel.count && tabModel.get(firstUnpinned).pinned) ++firstUnpinned; idx = Math.max(idx, firstUnpinned) }
        tabModel.insert(idx, {tabId: id, title: opts.title || "New Tab", url: url || "", icon: "", loading: false, pinned: !!opts.pinned, audible: false, muted: false})
        const t = tabById(id)
        t.openerTabId = opts.openerTabId !== undefined ? opts.openerTabId : -1
        Extensions.browserEvent("tabCreated", JSON.stringify({tab: t.extInfo()}))
        if (opts.request) opts.request.openIn(t.view)
        else if (opts.lazy) t.pendingUrl = url
        else if (url) t.load(url)
        if (!opts.background || tabModel.count === 1) setCurrentIndex(idx)
        else if (idx <= currentIndex && tabModel.count > 1) currentIndex++
        reindex()
        return t
    }

    function restoreTab(saved) {
        const t = openTab(saved.url, {background: true, lazy: true, pinned: saved.pinned, title: saved.title})
        if (saved.zoom > 0) t.view.zoomFactor = saved.zoom
        return t
    }

    function closeTab(index) {
        if (index < 0 || index >= tabModel.count) return
        const t = tabAt(index)
        if (t && !privateMode) Browser.pushClosedTab(JSON.stringify(t.saveState()))
        const id = tabModel.get(index).tabId
        const wasCurrent = index === currentIndex
        // Pick the tab to activate: the opener if it exists, else the neighbour.
        let next = index < tabModel.count - 1 ? index : index - 1
        if (t && t.openerTabId >= 0 && indexOfTab(t.openerTabId) >= 0 && wasCurrent) next = indexOfTab(t.openerTabId)
        tabModel.remove(index)
        Extensions.browserEvent("tabRemoved", JSON.stringify({tabId: id, windowId: windowId, windowClosing: false}))
        if (tabModel.count === 0) {
            if (app.setting("tabs.closeWindowWithLastTab") || app.windows.length > 1) { bw.close(); return }
            openTab(app.newTabUrl(), {background: false})
            return
        }
        if (next > index) next--
        if (wasCurrent) { currentIndex = -1; setCurrentIndex(Math.max(0, Math.min(next, tabModel.count - 1))) }
        else if (index < currentIndex) currentIndex--
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
    function focusLocationBarNow() { locationBar.focusAndSelect() }
    function findResult(count, active) { findBar.matches = count; findBar.activeMatch = active }

    // Pinned tabs stay in front of unpinned ones.
    function tabModel_pinnedOk(from, to) { return tabModel.get(from).pinned === tabModel.get(to).pinned }

    function nextTab(step) { if (tabModel.count) setCurrentIndex((currentIndex + step + tabModel.count) % tabModel.count) }

    function reopenClosedTab(i) {
        const saved = JSON.parse(Browser.popClosedTab(i || 0))
        if (saved && saved.url) { const t = openTab(saved.url, {background: false, pinned: saved.pinned}); return t }
        return null
    }

    function closeOtherTabs(keepIndex) { for (let i = tabModel.count - 1; i >= 0; --i) if (i !== keepIndex && !tabModel.get(i).pinned) closeTab(i) }
    function closeTabsToRight(index) { for (let i = tabModel.count - 1; i > index; --i) if (!tabModel.get(i).pinned) closeTab(i) }

    function saveState() {
        const tabs = []
        for (let i = 0; i < tabModel.count; ++i) { const t = tabAt(i); if (t) tabs.push(t.saveState()) }
        return {tabs: tabs, current: currentIndex, geometry: [x, y, width, height], maximized: visibility === Window.Maximized}
    }

    function extInfo(populate) {
        const info = {id: windowId, focused: active, top: y, left: x, width: width, height: height, incognito: privateMode, type: "normal",
                      state: visibility === Window.Maximized ? "maximized" : visibility === Window.Minimized ? "minimized" : visibility === Window.FullScreen ? "fullscreen" : "normal",
                      alwaysOnTop: false, title: title}
        if (populate) { info.tabs = []; for (let i = 0; i < tabModel.count; ++i) info.tabs.push(tabAt(i).extInfo()) }
        return info
    }

    function toast(message, icon) { toastLabel.text = message; toastIcon.source = icon; toastBox.opacity = 1; toastTimer.restart() }
    function downloadStarted() { downloadsActive++; downloadsBadge.restart() }

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
    function toggleBookmarkFromMenu() { locationBar.toggleBookmark() }
    // used by bookmark menus in the main menu
    property QtObject bookmarksOpener: QtObject {
        property var win: bw
        function open(url, mods) { if (bw.currentTab) bw.currentTab.load(url) }
    }

    function openPageActionPopup(action, anchor) { extPopup.show(action, anchor) }

    function openExtensionPopup(extId) {
        const a = toolbarActions.find(x => x.ext === extId && x.kind === "browserAction")
        if (!a) return
        if (a.popup) extPopup.show(a, extButtons.buttonFor(extId))
        else Extensions.actionClicked(extId, a.kind, currentTab ? currentTab.tabId : -1)
    }

    // ---- shortcuts ------------------------------------------------------------------
    Shortcut { sequences: [StandardKey.AddTab]; onActivated: bw.openTab(app.newTabUrl(), {background: false}).focusLocationBar() }
    Shortcut { sequences: [StandardKey.Close]; onActivated: bw.closeTab(bw.currentIndex) }
    Shortcut { sequence: "Ctrl+Shift+T"; onActivated: bw.reopenClosedTab(0) }
    Shortcut { sequence: "Ctrl+N"; onActivated: app.newWindow() }
    Shortcut { sequence: "Ctrl+Shift+P"; onActivated: app.newWindow({privateMode: true}) }
    Shortcut { sequences: ["Ctrl+L", "F6", "Alt+D"]; onActivated: locationBar.focusAndSelect() }
    Shortcut { sequence: "Ctrl+K"; onActivated: { locationBar.focusAndSelect(); locationBar.text = "? " ; locationBar.cursorPosition = 2 } }
    Shortcut { sequences: ["Ctrl+R", "F5"]; onActivated: if (bw.currentTab) bw.currentTab.view.reload() }
    Shortcut { sequences: ["Ctrl+Shift+R", "Ctrl+F5"]; onActivated: if (bw.currentTab) bw.currentTab.view.reloadAndBypassCache() }
    // Only while the find bar is open: otherwise Escape must reach the page
    // (web apps and extensions such as Vimium rely on it).
    Shortcut { sequence: "Esc"; enabled: findBar.visible; onActivated: findBar.close() }
    Shortcut { sequences: [StandardKey.Back, "Alt+Left"]; onActivated: if (bw.currentTab) bw.currentTab.view.goBack() }
    Shortcut { sequences: [StandardKey.Forward, "Alt+Right"]; onActivated: if (bw.currentTab) bw.currentTab.view.goForward() }
    Shortcut { sequence: "Alt+Home"; onActivated: if (bw.currentTab) bw.currentTab.load(app.setting("general.homepage")) }
    Shortcut { sequences: [StandardKey.Find]; onActivated: findBar.open() }
    Shortcut { sequences: [StandardKey.FindNext, "F3"]; onActivated: findBar.find(false) }
    Shortcut { sequences: [StandardKey.FindPrevious, "Shift+F3"]; onActivated: findBar.find(true) }
    Shortcut { sequence: "Ctrl+D"; onActivated: locationBar.toggleBookmark() }
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

        // Tab strip
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Kirigami.Units.gridUnit * 2.2
            color: bw.privateMode ? Qt.darker(Kirigami.Theme.backgroundColor, 1.35) : Qt.darker(Kirigami.Theme.backgroundColor, 1.08)
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Kirigami.Units.smallSpacing
                spacing: 0
                Kirigami.Icon {
                    visible: bw.privateMode
                    source: "view-private"
                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                    Layout.preferredHeight: Kirigami.Units.iconSizes.small
                    Layout.rightMargin: Kirigami.Units.smallSpacing
                }
                ListView {
                    id: strip
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    orientation: ListView.Horizontal
                    model: tabModel
                    interactive: false
                    clip: true
                    currentIndex: bw.currentIndex
                    highlightFollowsCurrentItem: false
                    boundsBehavior: Flickable.StopAtBounds
                    readonly property real normalWidth: {
                        let pinned = 0
                        for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).pinned) pinned++
                        const avail = width - pinned * Kirigami.Units.gridUnit * 2.4
                        return Math.max(Kirigami.Units.gridUnit * 5, Math.min(Kirigami.Units.gridUnit * 14, avail / Math.max(1, tabModel.count - pinned)))
                    }
                    WheelHandler { onWheel: e => bw.nextTab(e.angleDelta.y > 0 ? -1 : 1) }
                    moveDisplaced: Transition { NumberAnimation { properties: "x"; duration: Kirigami.Units.shortDuration } }
                    delegate: TabButton {
                        browser: bw
                        tabList: strip
                    }
                    footer: QQC2.ToolButton {
                        icon.name: "list-add"
                        height: strip.height
                        onClicked: bw.openTab(app.newTabUrl(), {background: false}).focusLocationBar()
                        QQC2.ToolTip.text: "New Tab (Ctrl+T)"
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
                QQC2.ToolButton {
                    icon.name: "tab-duplicate"
                    display: QQC2.AbstractButton.IconOnly
                    text: "List all tabs"
                    onClicked: tabListMenu.popup()
                    QQC2.Menu {
                        id: tabListMenu
                        Instantiator {
                            model: tabModel
                            delegate: QQC2.MenuItem {
                                required property int index
                                required property string title
                                text: title
                                checkable: true
                                checked: index === bw.currentIndex
                                onTriggered: bw.setCurrentIndex(index)
                            }
                            onObjectAdded: (i, o) => tabListMenu.insertItem(i, o)
                            onObjectRemoved: (i, o) => tabListMenu.removeItem(o)
                        }
                        QQC2.MenuSeparator {}
                        QQC2.MenuItem { text: "Reopen Closed Tab"; icon.name: "edit-undo"; onTriggered: bw.reopenClosedTab(0) }
                    }
                }
            }
        }

        // Navigation toolbar
        QQC2.ToolBar {
            Layout.fillWidth: true
            position: QQC2.ToolBar.Header
            RowLayout {
                anchors.fill: parent
                spacing: Kirigami.Units.smallSpacing
                QQC2.ToolButton {
                    icon.name: "go-previous"
                    enabled: bw.currentTab && bw.currentTab.view.canGoBack
                    onClicked: bw.currentTab.view.goBack()
                    onPressAndHold: historyMenu.showFor(true, this)
                    QQC2.ToolTip.text: "Back"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "go-next"
                    enabled: bw.currentTab && bw.currentTab.view.canGoForward
                    onClicked: bw.currentTab.view.goForward()
                    onPressAndHold: historyMenu.showFor(false, this)
                    QQC2.ToolTip.text: "Forward"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: bw.currentTab && bw.currentTab.loading ? "process-stop" : "view-refresh"
                    onClicked: bw.currentTab.loading ? bw.currentTab.view.stop() : bw.currentTab.view.reload()
                    QQC2.ToolTip.text: bw.currentTab && bw.currentTab.loading ? "Stop" : "Reload"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.ToolButton {
                    icon.name: "go-home"
                    visible: app.setting("appearance.showHomeButton")
                    onClicked: bw.currentTab.load(app.setting("general.homepage"))
                    QQC2.ToolTip.text: "Home"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                LocationBar {
                    id: locationBar
                    Layout.fillWidth: true
                    win: bw
                    tab: bw.currentTab
                }
                ExtensionButtons {
                    id: extButtons
                    win: bw
                    actions: bw.toolbarActions.filter(a => a.kind === "browserAction")
                }
                QQC2.ToolButton {
                    icon.name: "download"
                    onClicked: { bw.downloadsActive = 0; app.showDownloads() }
                    QQC2.ToolTip.text: "Downloads"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Rectangle {
                        visible: bw.downloadsActive > 0
                        width: Kirigami.Units.smallSpacing * 2; height: width; radius: width / 2
                        color: Kirigami.Theme.highlightColor
                        anchors { right: parent.right; top: parent.top; margins: Kirigami.Units.smallSpacing }
                        SequentialAnimation on scale { id: downloadsBadge; running: false; NumberAnimation { to: 2; duration: 150 } NumberAnimation { to: 1; duration: 150 } }
                    }
                }
                QQC2.ToolButton {
                    id: menuButton
                    icon.name: "application-menu"
                    onClicked: mainMenu.popup(menuButton, 0, menuButton.height)
                    QQC2.ToolTip.text: "Open Menu"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }

        BookmarksBar {
            Layout.fillWidth: true
            visible: bw.showBookmarksBar
            win: bw
        }
    }

    QQC2.SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal

        Sidebar {
            id: sidebarPanel
            visible: bw.sidebar !== "" && !bw.contentFullScreen
            QQC2.SplitView.preferredWidth: Kirigami.Units.gridUnit * 16
            QQC2.SplitView.minimumWidth: Kirigami.Units.gridUnit * 10
            win: bw
            mode: bw.sidebar
        }

        Item {
            id: content
            QQC2.SplitView.fillWidth: true
            Repeater {
                model: tabModel
                delegate: WebTab {
                    required property int index
                    tabIndex: index
                    window: bw
                    app: bw.app
                    width: content.width
                    height: content.height - (findBar.visible ? findBar.height : 0)
                    visible: index === bw.currentIndex
                }
            }

            FindBar {
                id: findBar
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                tab: bw.currentTab
            }

            // Hovered link
            Rectangle {
                id: statusBubble
                property string text: bw.currentTab ? bw.currentTab.hoveredLink : ""
                visible: text !== "" && app.setting("appearance.showStatusBar")
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
