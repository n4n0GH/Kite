import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Dialogs
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite
import "tabtree.js" as TabTree

// A browser window (Falkon's BrowserWindow): tab strip, navigation toolbar
// with location bar and extension buttons, bookmarks toolbar, sidebar,
// web views, find bar and status bubble.
QQC2.ApplicationWindow {
    id: bw

    required property var app
    required property int windowId
    property bool privateMode: false
    // Private windows in purple ("privacy.privateWindowColor"), told apart at a
    // glance: the chrome by these colors (each part that fixes its own colors
    // has a PrivateColors), the titlebar by the color scheme Main.qml has
    // Plasma give the window. Both are kite_core::privatecolors.
    readonly property bool purple: privateMode && !!app.liveSettings["privacy.privateWindowColor"]
    readonly property var purpleColors: JSON.parse(Browser.privateWindowColors())
    PrivateColors { target: bw; active: bw.purple; palette: bw.purpleColors.chrome }
    PrivateColors { target: headerBox; active: bw.purple; palette: bw.purpleColors.chrome }
    PrivateColors { target: footerSlot; active: bw.purple; palette: bw.purpleColors.chrome }
    PrivateColors { target: splitView; active: bw.purple; palette: bw.purpleColors.chrome }
    property alias count: tabModel.count
    property int currentIndex: 0
    property var currentTab: null

    // ---- selecting several tabs ------------------------------------------------
    // Ctrl+click adds or removes a tab, Shift+click selects the tabs from the
    // active one to the clicked one (in its pane); a plain click ends the
    // selection. The tab menu then acts on all of them.
    property var selectedTabs: []
    function clickTab(index, modifiers) {
        const id = tabModel.get(index).tabId
        const active = currentTab ? currentTab.tabId : -1
        if (modifiers & Qt.ControlModifier) {
            let sel = selectedTabs.length ? selectedTabs.slice() : [active]
            if (sel.includes(id)) { if (id !== active) sel = sel.filter(x => x !== id) }
            else sel.push(id)
            selectedTabs = sel.length > 1 ? sel : []
        } else if (modifiers & Qt.ShiftModifier) {
            const pane = tabModel.get(index).pane
            const from = Math.min(currentIndex, index), to = Math.max(currentIndex, index)
            const sel = []
            for (let i = from; i <= to; ++i) if (tabModel.get(i).pane === pane) sel.push(tabModel.get(i).tabId)
            selectedTabs = sel.length > 1 ? sel : []
        } else {
            selectedTabs = []
            setCurrentIndex(index)
        }
    }
    // The tabs a tab's menu acts on: the selection if the tab is in it.
    function actionTabs(id) {
        if (!selectedTabs.includes(id)) return [id]
        const ids = []
        for (let i = 0; i < tabModel.count; ++i) if (selectedTabs.includes(tabModel.get(i).tabId)) ids.push(tabModel.get(i).tabId)
        return ids
    }
    // Group actions for several tabs.
    // (In a tree, a tab brings its descendants along, wherever they go:
    // `treeRoots` keeps one from being moved twice.)
    function treeRoots(ids) { return treeEnabled ? TabTree.roots(treeRows(), ids) : ids }
    function newGroupFor(ids) {
        const roots = treeRoots(ids)
        const gid = newGroup(roots[0])
        if (gid) for (const id of roots.slice(1)) addToGroup(id, gid)
        selectedTabs = []
        return gid
    }
    function addTabsToGroup(ids, gid) { for (const id of treeRoots(ids)) addToGroup(id, gid); selectedTabs = [] }
    function removeTabsFromGroup(ids) { for (const id of treeRoots(ids).reverse()) removeFromGroup(id); selectedTabs = [] }
    function closeTabs(ids) { selectedTabs = []; for (const id of ids.slice().reverse()) closeTab(indexOfTab(id)) }
    function moveTabsToOtherPane(ids) { selectedTabs = []; for (const id of ids) moveToOtherPane(id) }

    // ---- tab groups --------------------------------------------------------------
    // A tab's "group" role names its group ("" = none); the groups' name,
    // colour and collapsed state are here. A group's tabs are next to each
    // other, in one pane, and not pinned.
    property var tabGroups: ({})            // id → {name, color, collapsed}
    property int groupsVersion: 0           // bumped when membership or order changes
    readonly property var groupColors: ["#3daee9", "#da4453", "#fdbc4b", "#27ae60", "#e93a9a", "#9b59b6", "#1abc9c", "#f67400", "#7f8c8d"]
    function groupOf(id) { const i = indexOfTab(id); return i >= 0 ? tabModel.get(i).group : "" }
    function groupTabs(gid) { const ids = []; for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).group === gid) ids.push(tabModel.get(i).tabId); return ids }
    // Whether the tab is its group's first (which shows the group's chip or
    // header, and stands for the group when it is collapsed).
    function isGroupFirst(id) {
        groupsVersion
        const i = indexOfTab(id)
        if (i < 0) return false
        const g = tabModel.get(i).group
        return g !== "" && (i === 0 || tabModel.get(i - 1).group !== g)
    }
    function updateGroup(gid, change) {
        if (!tabGroups[gid]) return
        const all = Object.assign({}, tabGroups)
        all[gid] = Object.assign({}, all[gid], change)
        tabGroups = all
    }
    function dropEmptyGroups() {
        const all = {}
        for (const gid in tabGroups) if (groupTabs(gid).length) all[gid] = tabGroups[gid]
        tabGroups = all
        groupsVersion++
    }
    // A new group holding the tab; its id.
    function newGroup(tabId) {
        const i = indexOfTab(tabId)
        if (i < 0 || tabModel.get(i).pinned) return ""
        const used = Object.values(tabGroups).map(g => g.color)
        const color = groupColors.find(c => !used.includes(c)) || groupColors[Object.keys(tabGroups).length % groupColors.length]
        const gid = "g" + Date.now().toString(36) + Math.floor(Math.random() * 1000)
        const all = Object.assign({}, tabGroups)
        all[gid] = {name: "Group " + (Object.keys(all).length + 1), color: color, collapsed: false}
        tabGroups = all
        removeFromGroup(tabId)
        const moving = treeTabs(tabId)
        // A group can't sit inside a tree (it would cut off the tabs after it):
        // a tab with a parent moves to the end of the tree it leaves.
        const top = TabTree.ancestors(treeRows(), tabId).pop()
        if (treeEnabled && top !== undefined) {
            const from = indexOfTab(tabId), end = indexOfTab(top) + tabModel.get(indexOfTab(top)).treeCount
            tabModel.setProperty(from, "treeParent", -1)
            if (from + moving.length - 1 < end) moveRangeTo(from, from + moving.length - 1, end)
        }
        for (const id of moving) tabModel.setProperty(indexOfTab(id), "group", gid)
        groupsVersion++
        updateTree()
        return gid
    }
    // Into a group: the tab moves after the group's last tab (and to its pane).
    function addToGroup(tabId, gid) {
        const members = groupTabs(gid)
        if (!members.length || members.includes(tabId) || tabModel.get(indexOfTab(tabId)).pinned) return
        // With its descendants; only its link to a parent outside the group goes.
        const moving = treeTabs(tabId)
        tabModel.setProperty(indexOfTab(tabId), "treeParent", -1)
        removeFromGroup(tabId)
        const from = indexOfTab(tabId)
        const pane = tabModel.get(indexOfTab(members[0])).pane
        if (tabModel.get(from).pane !== pane) {
            for (const id of moving) tabModel.setProperty(indexOfTab(id), "pane", pane)
            countPane1()
        }
        const end = indexOfTab(members[members.length - 1])
        moveRangeTo(from, from + moving.length - 1, from > end ? end + 1 : end)
        for (const id of moving) tabModel.setProperty(indexOfTab(id), "group", gid)
        if (tabGroups[gid].collapsed) updateGroup(gid, {collapsed: false})
        groupsVersion++
        updateTree()
    }
    // Out of its group: a tab in the middle moves after the group's end, with
    // its descendants (`alone`: without them, for a tab leaving the tree too).
    function removeFromGroup(tabId, alone) {
        const i = indexOfTab(tabId)
        if (i < 0) return
        const gid = tabModel.get(i).group
        if (!gid) return
        const members = groupTabs(gid)
        const last = indexOfTab(members[members.length - 1])
        const leaving = alone ? [tabId] : treeTabs(tabId)
        for (const id of leaving) tabModel.setProperty(indexOfTab(id), "group", "")
        const end = i + leaving.length - 1
        if (end !== last && members.indexOf(tabId) !== 0) moveRangeTo(i, end, last)
        dropEmptyGroups()
        if (!alone) updateTree()
    }
    function ungroup(gid) {
        for (const id of groupTabs(gid)) tabModel.setProperty(indexOfTab(id), "group", "")
        dropEmptyGroups()
    }
    function closeGroup(gid) { for (const id of groupTabs(gid).reverse()) closeTab(indexOfTab(id)) }
    // Collapsing a group showing a pane's tab first shows another tab of
    // that pane (if there is none, the group stays open).
    function setGroupCollapsed(gid, collapsed) {
        if (!tabGroups[gid]) return
        if (collapsed) {
            const members = groupTabs(gid)
            for (const pane of [0, 1]) {
                if (!members.includes(paneTab[pane])) continue
                const other = paneTabs(pane).filter(id => !members.includes(id))
                if (!other.length) { toast("The group holds this side's only tabs", "dialog-information"); return }
                const at = paneTabs(pane).indexOf(paneTab[pane])
                const after = paneTabs(pane).slice(at).find(id => !members.includes(id))
                const next = after !== undefined ? after : other[other.length - 1]
                if (pane === activePane) setCurrentIndex(indexOfTab(next))
                else setPaneTab(pane, next)
            }
        }
        updateGroup(gid, {collapsed: collapsed})
    }
    // After tabs move (dragging): a tab left apart from its group leaves it;
    // an ungrouped tab between two tabs of a group joins it.
    function normalizeGroups() {
        const groupAt = i => i >= 0 && i < tabModel.count ? tabModel.get(i).group : ""
        // First the tabs dropped inside a group join it (so its tabs on
        // either side don't look separated)...
        for (let i = 0; i < tabModel.count; ++i)
            if (!tabModel.get(i).group && groupAt(i - 1) && groupAt(i - 1) === groupAt(i + 1) && !tabModel.get(i).pinned) tabModel.setProperty(i, "group", groupAt(i - 1))
        // ...then tabs apart from the rest of their group leave it.
        for (let i = 0; i < tabModel.count; ++i) {
            const g = tabModel.get(i).group
            if (g && groupAt(i - 1) !== g && groupAt(i + 1) !== g && groupTabs(g).length > 1) tabModel.setProperty(i, "group", "")
        }
        dropEmptyGroups()
    }
    // Dragging: where a tab (or a group, the range `first`..`last`) dragged
    // towards index `to` goes. A collapsed group in the way is passed as a
    // whole, never entered. -1: nowhere new.
    function dragTarget(first, last, to) {
        if (to >= first && to <= last) return -1
        const g = tabModel.get(to).group
        if (g && tabGroups[g] && tabGroups[g].collapsed && g !== tabModel.get(first).group) {
            const members = groupTabs(g)
            to = indexOfTab(members[to > last ? members.length - 1 : 0])
        }
        // The same for a collapsed tree (its descendants are out of sight).
        if (to > last && tabModel.get(to).treeCollapsed) to += tabModel.get(to).treeCount
        return to
    }
    // Move a group's tabs as one, its first tab to `to` (dragging left) or
    // its last one (dragging right).
    function moveGroupTo(gid, to) {
        const members = groupTabs(gid)
        moveRangeTo(indexOfTab(members[0]), indexOfTab(members[members.length - 1]), to)
    }
    // (The same for any tabs next to each other: a tab and its descendants.)
    function moveRangeTo(first, last, to) {
        // The neighbours in the way go to the other side, one by one.
        while (to > last) { moveTab(last + 1, first, true); first++; last++ }
        while (to < first) { moveTab(first - 1, last, true); first--; last-- }
    }
    // While a tab (with its descendants) is dragged, its group follows it, so
    // the headers show where it would land. Crossing a tab of a group takes it
    // into that group (not a collapsed one: that is passed as a whole). It stays
    // in until it is no longer next to the group's tabs, or if it is all there
    // is of the group. (`movedDown`: which way it just moved.)
    function dragGroupPreview(id, movedDown) {
        const i = indexOfTab(id)
        if (i < 0 || tabModel.get(i).pinned) return
        const end = i + tabModel.get(i).treeCount
        const prev = i > 0 ? tabModel.get(i - 1).group : "", next = end < tabModel.count - 1 ? tabModel.get(end + 1).group : ""
        const crossed = movedDown ? prev : next
        const cur = tabModel.get(i).group
        let g = ""
        if (crossed && tabGroups[crossed] && !tabGroups[crossed].collapsed) {
            g = crossed
        } else if (cur) {
            let others = 0
            for (let j = 0; j < tabModel.count; ++j) if ((j < i || j > end) && tabModel.get(j).group === cur) others++
            if (prev === cur || next === cur || others === 0) g = cur
        }
        if (cur === g) return
        for (let j = i; j <= end; ++j) tabModel.setProperty(j, "group", g)
        groupsVersion++
    }
    // A drag ended: groups follow where the tab (id, or -1 for a group) was
    // dropped. A tab dragged away from its group leaves it (not the ones
    // left behind); one dropped between two tabs of a group joins it.
    // (In a tree, the tab's descendants come along, and `treeDepth` is the
    // depth it was dragged to.)
    function finishTabDrag(id, treeDepth) {
        treeDragging = false
        const i = indexOfTab(id)
        if (i >= 0) {
            const end = i + tabModel.get(i).treeCount
            let g = tabModel.get(i).group
            const prev = i > 0 ? tabModel.get(i - 1).group : "", next = end < tabModel.count - 1 ? tabModel.get(end + 1).group : ""
            if (g && prev !== g && next !== g && groupTabs(g).length > end - i + 1) g = ""
            // (Dropped between two tabs of a group, descendants and all.)
            if (!g && prev && prev === next && !tabModel.get(i).pinned) g = prev
            for (let j = i; j <= end; ++j) if (tabModel.get(j).group !== g) tabModel.setProperty(j, "group", g)
            if (treeEnabled) tabModel.setProperty(i, "treeParent", TabTree.dropParent(treeRows(), i, end, treeDepth))
        }
        normalizeGroups()
        updateTree()
    }

    function openGroupMenu(gid, item) { groupMenu.gid = gid; groupMenu.popup(item, 0, item.height) }
    function renameGroup(gid) { groupRename.gid = gid; groupRename.open() }

    // ---- tree-style tabs ("tabs.tree") ---------------------------------------
    // In vertical lists, a link opens as a child of its tab. The "treeParent"
    // role holds the links (a tab id, -1 = none) and "treeCollapsed" whether a
    // tab's descendants are hidden; tabtree.js gives the rest ("treeDepth",
    // "treeCount", "treeHidden"). A tab's descendants are always right after it.
    // Other tab bars show a flat list; the links wait for the list to return.
    // Tabs drawn by the application style ("tabs.styleDrawn"; TabButton.qml, horizontal strips).
    readonly property bool styledTabs: !!app.liveSettings["tabs.styleDrawn"]
    readonly property bool treeEnabled: !!app.liveSettings["tabs.tree"] && (tabPosition === "vertical" || tabPosition === "verticalRight")
    onTreeEnabledChanged: updateTree()
    // While dragging (any tab, in any tab bar), tabs pass through places they
    // don't belong.
    property bool treeDragging: false
    // While a window's tabs are restored, one by one (a collapsed tab would
    // lose that for not having its descendants yet).
    property bool treeRestoring: false
    onGroupsVersionChanged: Qt.callLater(updateTree)
    function treeRows() {
        const rows = []
        for (let i = 0; i < tabModel.count; ++i) {
            const r = tabModel.get(i)
            rows.push({id: r.tabId, parent: r.treeParent, collapsed: r.treeCollapsed, pinned: r.pinned, pane: r.pane, group: r.group})
        }
        return rows
    }
    function updateTree() {
        if (treeDragging || treeRestoring) return
        const tree = treeEnabled ? TabTree.compute(treeRows()) : null
        for (let i = 0; i < tabModel.count; ++i) {
            const r = tabModel.get(i)
            const t = tree ? tree[i] : {parent: r.treeParent, depth: 0, hidden: false, count: 0, collapsed: r.treeCollapsed}
            if (r.treeParent !== t.parent) tabModel.setProperty(i, "treeParent", t.parent)
            if (r.treeDepth !== t.depth) tabModel.setProperty(i, "treeDepth", t.depth)
            if (r.treeHidden !== t.hidden) tabModel.setProperty(i, "treeHidden", t.hidden)
            if (r.treeCount !== t.count) tabModel.setProperty(i, "treeCount", t.count)
            if (r.treeCollapsed !== t.collapsed) tabModel.setProperty(i, "treeCollapsed", t.collapsed)
        }
    }
    // The tab with its descendants (ids, in order).
    function treeTabs(id) {
        const i = indexOfTab(id)
        const ids = []
        if (i >= 0) for (let j = i; j <= i + tabModel.get(i).treeCount; ++j) ids.push(tabModel.get(j).tabId)
        return ids
    }
    function setTreeCollapsed(id, collapsed) {
        const i = indexOfTab(id)
        if (i < 0) return
        if (collapsed) {
            // A pane showing one of the tabs that go shows the tab itself.
            const pane = tabModel.get(i).pane
            if (treeTabs(id).includes(paneTab[pane]) && paneTab[pane] !== id) {
                if (pane === activePane) setCurrentIndex(i)
                else setPaneTab(pane, id)
            }
        }
        tabModel.setProperty(i, "treeCollapsed", collapsed)
        updateTree()
    }
    // A tab leaves the tree where it is: its children move up a level.
    function detachFromTree(id) {
        const i = indexOfTab(id)
        if (i < 0) return
        const parent = tabModel.get(i).treeParent
        for (let j = 0; j < tabModel.count; ++j) if (tabModel.get(j).treeParent === id) tabModel.setProperty(j, "treeParent", parent)
        tabModel.setProperty(i, "treeParent", -1)
    }
    function expandTreeTo(id) {
        for (const up of TabTree.ancestors(treeRows(), id)) tabModel.setProperty(indexOfTab(up), "treeCollapsed", false)
        updateTree()
    }
    // A saved tab's place in the tree: the parent as an index in the window.
    function treeState(id) {
        const i = indexOfTab(id)
        if (i < 0) return {treeParent: -1, treeCollapsed: false}
        return {treeParent: indexOfTab(tabModel.get(i).treeParent), treeCollapsed: tabModel.get(i).treeCollapsed}
    }
    // The depth tabs dropped at first..last end up at, given the one asked for.
    function treeDropDepth(first, last, wanted) {
        if (!treeEnabled) return 0
        const parent = TabTree.dropParent(treeRows(), first, last, wanted)
        return parent < 0 ? 0 : tabModel.get(indexOfTab(parent)).treeDepth + 1
    }

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
        // Groups and trees aren't split: the tab leaves them.
        removeFromGroup(id, true)
        detachFromTree(id)
        tabModel.setProperty(indexOfTab(id), "pane", to)
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
    // A page's own fullscreen (a video's fullscreen button): the tab that
    // asked, and whether the window was fullscreen before (F11).
    property bool contentFullScreen: false
    // When the mouse was last pressed in this window (ms since the epoch; bootstrap.cpp).
    property real lastPointerPress: 0
    property var fullScreenTab: null
    property bool wasFullScreen: false
    function enterContentFullScreen(tab) {
        if (!contentFullScreen) wasFullScreen = visibility === Window.FullScreen
        fullScreenTab = tab
        contentFullScreen = true
        showFullScreen()
        // Say who took the screen, and the way out: with Kite's own bars gone,
        // a page could otherwise draw a browser of its own.
        fullScreenNotice.site = Browser.hostOf(tab.url) || "This page"
        fullScreenNotice.opacity = 1
        fullScreenNoticeTimer.restart()
    }
    // The page left fullscreen itself (`fromPage`), or Kite makes it leave
    // (Escape, the window leaving fullscreen).
    function exitContentFullScreen(fromPage) {
        if (!contentFullScreen) return
        const t = fullScreenTab
        contentFullScreen = false
        fullScreenTab = null
        if (!fromPage && t) t.view.fullScreenCancelled()
        if (!wasFullScreen && visibility === Window.FullScreen) showNormal()
    }
    onVisibilityChanged: if (contentFullScreen && visibility !== Window.FullScreen) exitContentFullScreen(false)

    Rectangle {
        id: fullScreenNotice
        property string site: ""
        parent: bw.contentItem
        z: 1000
        opacity: 0
        visible: opacity > 0 && bw.contentFullScreen
        Behavior on opacity { NumberAnimation { duration: Kirigami.Units.longDuration } }
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: Kirigami.Units.gridUnit * 2 }
        width: Math.min(noticeLabel.implicitWidth + Kirigami.Units.gridUnit * 2, parent.width - Kirigami.Units.gridUnit * 2)
        height: noticeLabel.implicitHeight + Kirigami.Units.largeSpacing * 2
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.textColor
        QQC2.Label {
            id: noticeLabel
            anchors.centerIn: parent
            width: Math.min(implicitWidth, parent.parent.width - Kirigami.Units.gridUnit * 4)
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            color: Kirigami.Theme.backgroundColor
            text: fullScreenNotice.site + " is now full screen. Press Esc to leave."
        }
        Timer { id: fullScreenNoticeTimer; interval: 4000; onTriggered: fullScreenNotice.opacity = 0 }
    }
    property var toolbarActions: []
    property int downloadsActive: 0

    width: app.setting("window.width")
    height: app.setting("window.height")
    minimumWidth: 480
    minimumHeight: 320
    // (Ends in the application's name, or Qt adds it again: QPlatformWindow::formatWindowTitle.)
    title: (currentTab && currentTab.title ? currentTab.title + " — " : "") + (privateMode ? "Private Browsing — " : "") + "Kite"
    color: Kirigami.Theme.backgroundColor

    onActiveChanged: if (active) app.windowActivated(bw)
    onClosing: close => {
        // The last window takes Kite with it.
        if (!app.quitting && app.windows.length === 1 && app.askBeforeQuit(() => bw.close(), bw)) {
            close.accepted = false
            return
        }
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
        const menu = part(mainMenuPart)
        if (menu.opened && menu.parent === item) menu.close()
        else menu.popup(item, 0, item.height)
    }
    function customizeToolbar() { part(customizePart).open() }
    function showAbout() { part(aboutPart).open() }
    // The last crash's report; `expanded` shows its text right away.
    function showCrashReport(expanded) { part(crashReportPart).show(expanded) }

    // Parts of the window made the first time they're used, then kept: made
    // with the window, they'd cost every new window, and Kite's startup, the
    // time for menus and dialogs most windows never open.
    property var _parts: new Map()
    function part(component) {
        let o = _parts.get(component)
        if (!o) { o = component.createObject(bw); _parts.set(component, o) }
        return o
    }
    Component { id: mainMenuPart; MainMenu { win: bw; app: bw.app } }
    Component { id: tabSearchPart; TabSearch { win: bw } }
    Component { id: commandPalettePart; CommandPalette { win: bw } }
    Component { id: sessionDialogsPart; SessionDialogs { win: bw } }
    Component { id: customizePart; CustomizeToolbarDialog { win: bw; catalog: bw.toolbarCatalog } }
    Component { id: crashReportPart; CrashReportDialog { parent: bw.contentItem } }
    Component { id: aboutPart; AboutDialog { parent: bw.contentItem } }
    // Closing the last window (or quitting) while Settings has changes not applied:
    // asked here, in the window just clicked. (The Settings window may be behind
    // others, and on Wayland Kite can't bring it to the front.)
    function askAboutUnsavedSettings(prefs, then) {
        const d = part(unsavedSettingsPart)
        d.prefs = prefs
        d.then = then
        d.open()
    }
    Component {
        id: unsavedSettingsPart
        Kirigami.PromptDialog {
            id: unsavedSettings
            property var prefs
            property var then
            parent: bw.contentItem
            title: "Apply Changes?"
            subtitle: "Settings has changes you haven't applied. Apply them or discard them before Kite closes?"
            standardButtons: Kirigami.Dialog.NoButton
            function answer(apply) {
                close()
                const page = prefs && prefs.pageStack.currentItem
                if (page) { if (apply) page.applyChanges(); else page.discardChanges() }
                if (then) then()
            }
            customFooterActions: [
                Kirigami.Action { text: "Apply"; icon.name: "dialog-ok-apply"; onTriggered: unsavedSettings.answer(true) },
                Kirigami.Action { text: "Discard"; icon.name: "edit-delete"; onTriggered: unsavedSettings.answer(false) },
                Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: unsavedSettings.close() }
            ]
        }
    }

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

    function registerTab(t) { tabItems[t.tabId] = t; tabOpened(t) }
    function unregisterTab(t) { delete tabItems[t.tabId] }
    function tabById(id) { return tabItems[id] || null }
    function tabAt(i) { return i >= 0 && i < tabModel.count ? tabById(tabModel.get(i).tabId) : null }
    function indexOfTab(id) { for (let i = 0; i < tabModel.count; ++i) if (tabModel.get(i).tabId === id) return i; return -1 }
    // A tab's view starts up while its row is being inserted, before the tab
    // strip has moved its other rows along: a change now would reach the tab
    // the new one pushes down, and stay there (its own value doesn't change
    // again). The row starts with these values anyway.
    property bool insertingTab: false
    function setTabProperty(id, key, value) { if (insertingTab) return; const i = indexOfTab(id); if (i >= 0) tabModel.setProperty(i, key, value) }

    // Where a tab opened from `tab` (a link, its context menu) goes
    // ("tabs.linkTabPosition"): after it and the tabs it opened before, so a row
    // of links opens in order beside it; right after it; or at the end.
    function linkTabIndex(tab) {
        const where = app.setting("tabs.linkTabPosition")
        const i = indexOfTab(tab.tabId)
        if (where === "end" || i < 0) return undefined
        let idx = i + 1
        if (where !== "afterOpener") while (idx < tabModel.count && tabAt(idx) && tabAt(idx).openerTabId === tab.tabId) ++idx
        return idx
    }

    function openTab(url, opts) {
        opts = opts || {}
        // (An extension's new tab page, say, if it isn't allowed here.)
        if (privateMode && url && url.startsWith("moz-extension:") && !Extensions.urlRunsInPrivate(url)) url = app.privateNewTabUrl()
        const id = app.nextTabId++
        let idx = opts.index !== undefined && opts.index >= 0 ? Math.min(opts.index, tabModel.count) : tabModel.count
        if (opts.index === undefined && opts.afterCurrent && app.setting("tabs.newTabAfterActive")) idx = currentIndex + 1
        if (!opts.pinned) { let firstUnpinned = 0; while (firstUnpinned < tabModel.count && tabModel.get(firstUnpinned).pinned) ++firstUnpinned; idx = Math.max(idx, firstUnpinned) }
        // A container tab ("privacy.containers"): its own cookies and storage.
        const container = !privateMode && opts.container && app.containersEnabled && app.containerInfo(opts.container) ? opts.container : ""
        if (container) app.containerProfile(container)
        // Its pane: as asked, its opener's, or the active one.
        const pane = opts.pane !== undefined ? opts.pane : opts.openerTabId !== undefined && opts.openerTabId >= 0 ? paneOf(opts.openerTabId) : activePane
        // Its group: as asked (a restored session), or its opener's when it
        // opens right after it or the tabs it opened (links from a grouped tab).
        let group = opts.group || ""
        if (!group && opts.openerTabId !== undefined && opts.openerTabId >= 0 && idx > indexOfTab(opts.openerTabId) && indexOfTab(opts.openerTabId) >= 0
                && (idx === indexOfTab(opts.openerTabId) + 1 || tabAt(idx - 1).openerTabId === opts.openerTabId) && paneOf(opts.openerTabId) === pane) group = groupOf(opts.openerTabId)
        if (opts.pinned) group = ""
        // Its parent: its opener, if a link opened it (after the opener's other
        // descendants, in its group); else what the place it opens in calls for.
        let treeParent = opts.treeParent !== undefined ? opts.treeParent : -1
        if (treeEnabled && !opts.pinned && opts.treeParent === undefined) {
            const opener = opts.fromLink ? indexOfTab(opts.openerTabId) : -1
            if (opener >= 0 && !tabModel.get(opener).pinned && tabModel.get(opener).pane === pane) {
                treeParent = opts.openerTabId
                idx = opener + tabModel.get(opener).treeCount + 1
                group = tabModel.get(opener).group
                if (tabModel.get(opener).treeCollapsed) setTreeCollapsed(opts.openerTabId, false)
            } else {
                treeParent = TabTree.parentAt(treeRows(), idx)
            }
        }
        insertingTab = true
        tabModel.insert(idx, {tabId: id, title: opts.title || "New Tab", url: url || "", icon: "", loading: false, pinned: !!opts.pinned, audible: false, muted: false, unloaded: false, container: container, pane: pane, group: group,
                              treeParent: treeParent, treeCollapsed: !!opts.treeCollapsed, treeDepth: 0, treeCount: 0, treeHidden: false})
        insertingTab = false
        if (group && tabGroups[group] && tabGroups[group].collapsed) updateGroup(group, {collapsed: false})
        updateTree()
        groupsVersion++
        countPane1()
        // (showInPane: its pane shows it, even in the background.)
        if (paneTab[pane] < 0 || opts.showInPane) setPaneTab(pane, id)
        const t = tabById(id)
        t.openerTabId = opts.openerTabId !== undefined ? opts.openerTabId : -1
        Extensions.browserEvent("tabCreated", JSON.stringify({tab: t.extInfo()}))
        if (opts.request) { t.openedByPage = true; opts.request.openIn(t.view) }
        else if (opts.lazy) t.pendingUrl = url
        else if (url) t.load(url)
        if (!opts.background || tabModel.count === 1 || (paneTab[pane] === id && pane === activePane)) setCurrentIndex(idx)
        else if (idx <= currentIndex && tabModel.count > 1) currentIndex++
        reindex()
        return t
    }

    // (`inWindow`: restoring a whole window, in order, so its tree comes back
    // too: a parent's saved index is where it is now.)
    function restoreTab(saved, inWindow) {
        const parent = inWindow && saved.treeParent >= 0 && saved.treeParent < tabModel.count ? tabModel.get(saved.treeParent).tabId : -1
        const t = openTab(saved.url, {background: true, lazy: true, pinned: saved.pinned, title: saved.title, container: saved.container, pane: saved.pane === 1 ? 1 : 0, group: saved.group || "",
                                      treeParent: parent, treeCollapsed: inWindow && !!saved.treeCollapsed})
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
        // The tab to show next in its pane: the one to the right, or at the end
        // the one to the left. A tab opened from another ("tabs.selectOpenerOnClose")
        // goes back to it, once the tabs it opened next to this one are closed too.
        const before = paneTabs(pane)
        const at = before.indexOf(id)
        let next = before.length > 1 ? before[at < before.length - 1 ? at + 1 : at - 1] : -1
        const opener = t ? t.openerTabId : -1
        if (opener >= 0 && opener !== id && before.includes(opener) && app.setting("tabs.selectOpenerOnClose")) {
            const right = at < before.length - 1 ? tabById(before[at + 1]) : null
            next = right && right.openerTabId === opener ? right.tabId : opener
        }
        detachFromTree(id)
        tabModel.remove(index)
        updateTree()
        countPane1()
        dropEmptyGroups()
        if (selectedTabs.includes(id)) selectedTabs = selectedTabs.filter(x => x !== id)
        Extensions.browserEvent("tabRemoved", JSON.stringify({tabId: id, windowId: windowId, windowClosing: false}))
        tabClosed(id)
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

    // (`internal`: a move by the group logic itself, which keeps groups as
    // they are; other moves, e.g. dragging, update group membership.)
    function moveTab(from, to, internal) {
        if (from === to || from < 0 || to < 0 || to >= tabModel.count) return
        const cur = tabModel.get(currentIndex).tabId
        const id = tabModel.get(from).tabId
        tabModel.move(from, to, 1)
        currentIndex = indexOfTab(cur)
        Extensions.browserEvent("tabMoved", JSON.stringify({tabId: id, windowId: windowId, fromIndex: from, toIndex: to}))
        if (!internal) normalizeGroups()
        groupsVersion++
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
        // A tab in a collapsed group opens the group.
        if (row.group && tabGroups[row.group] && tabGroups[row.group].collapsed) updateGroup(row.group, {collapsed: false})
        // So does a tab under a collapsed tab.
        if (row.treeHidden) expandTreeTo(row.tabId)
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
        treeRestoring = false
        updateTree()
        if (saved.splitWidth > 0) rightPaneWidth = saved.splitWidth
        if (saved.groups) { tabGroups = saved.groups; dropEmptyGroups() }
        const pc = saved.paneCurrent || []
        for (const pane of [0, 1]) {
            const t = tabAt(pc[pane])
            if (t && tabModel.get(pc[pane]).pane === pane) setPaneTab(pane, t.tabId)
            else { const ids = paneTabs(pane); setPaneTab(pane, ids.length ? ids[0] : -1) }
        }
        setCurrentIndex(Math.min(saved.current || 0, count - 1))
        // (The other pane's tab is on view too, though it isn't the current one.)
        for (const pane of [0, 1]) { const t = tabById(paneTab[pane]); if (t) t.wake() }
    }
    function focusLocationBarNow() { if (locationBar) locationBar.focusAndSelect() }
    // A menu gives the keyboard back to what had it as it finishes closing, after
    // its item has run: the address bar asks again then.
    function keepFocusOnLocationBar(tab) {
        locationBarRefocus.tab = tab
        locationBarRefocus.restart()
    }
    Timer {
        id: locationBarRefocus
        property var tab: null
        interval: 200
        onTriggered: if (bw.currentTab === tab && bw.locationBar && !bw.locationBar.activeFocus && !bw.activeFocusItem?.cursorVisible) bw.focusLocationBarNow()
    }
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
        // (Not the tabs hidden in collapsed trees.)
        const ids = paneTabs(activePane).filter(id => !tabModel.get(indexOfTab(id)).treeHidden)
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
        return {tabs: tabs, current: currentIndex, paneCurrent: [indexOfTab(paneTab[0]), indexOfTab(paneTab[1])], splitWidth: rightPaneWidth, groups: tabGroups,
                geometry: [x, y, width, height], maximized: visibility === Window.Maximized}
    }

    function extInfo(populate) {
        const info = {id: windowId, focused: active, top: y, left: x, width: width, height: height, incognito: privateMode, type: "normal",
                      state: visibility === Window.Maximized ? "maximized" : visibility === Window.Minimized ? "minimized" : visibility === Window.FullScreen ? "fullscreen" : "normal",
                      alwaysOnTop: false, title: title}
        if (populate) { info.tabs = []; for (let i = 0; i < tabModel.count; ++i) info.tabs.push(tabAt(i).extInfo()) }
        return info
    }

    // An address for another program, chosen by the user.
    function openExternal(url) {
        const ok = Qt.openUrlExternally(url)
        toast(ok ? "Opening " + url + " in another app" : "No app here can open " + url, ok ? "document-open" : "dialog-error")
    }

    function toast(message, icon, ms) { toastLabel.text = message; toastIcon.source = icon; toastBox.opacity = 1; toastTimer.interval = ms || 4000; toastTimer.restart() }
    function downloadStarted() { downloadsActive++; if (downloadsButton) downloadsButton.pulse() }

    function showSidebar(which) { sidebar = (sidebar === which ? "" : which) }

    // (`tab`: a page asking with window.print(), else the current one.)
    function printTab(tab) { app.printTab(tab || currentTab) }
    function savePdf() { pdfDialog.open() }

    // ---- extension toolbar -------------------------------------------------------
    function refreshToolbar() { toolbarActions = JSON.parse(Extensions.toolbar(currentTab ? currentTab.tabId : -1, privateMode)) }
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
    // The keys come from app.shortcutActions and the user's changes
    // (Settings → Shortcuts); runAction does the work.
    function runAction(id) {
        const t = currentTab
        switch (id) {
        case "newTab": openTab(app.newTabUrl(), {background: false}).focusLocationBar(); break
        case "closeTab": closeTab(currentIndex); break
        case "reopenTab": reopenClosedTab(0); break
        case "nextTab": nextTab(1); break
        case "previousTab": nextTab(-1); break
        case "tabLast": setCurrentIndex(count - 1); break
        case "muteTab": if (t) t.view.audioMuted = !t.view.audioMuted; break
        case "tabToOtherPane": if (t) moveToOtherPane(t.tabId); break
        case "tabSearch": toggleTabSearch(); break
        case "commandPalette": toggleCommandPalette(); break
        case "saveSession": saveSessionAs(); break
        case "manageSessions": manageSessions(); break
        case "newWindow": app.newWindow(); break
        case "newPrivateWindow": app.newWindow({privateMode: true}); break
        case "fullScreen": visibility === Window.FullScreen ? showNormal() : showFullScreen(); break
        case "quit": app.quit(); break
        case "focusAddress": if (locationBar) locationBar.focusAndSelect(); break
        case "searchWeb": if (locationBar) { locationBar.focusAndSelect(); locationBar.text = "? "; locationBar.cursorPosition = 2 } break
        case "back": if (t) t.view.goBack(); break
        case "forward": if (t) t.view.goForward(); break
        case "home": if (t) t.load(app.homeUrl()); break
        case "reload": if (t) t.view.reload(); break
        case "reloadBypass": if (t) t.view.reloadAndBypassCache(); break
        case "find": findBar.open(); break
        case "findNext": findBar.find(false); break
        case "findPrevious": findBar.find(true); break
        case "zoomIn": if (t) t.zoomStep(1); break
        case "zoomOut": if (t) t.zoomStep(-1); break
        case "zoomReset": if (t) t.resetZoom(); break
        case "viewSource": if (t) openTab("view-source:" + t.url, {background: false, afterCurrent: true}); break
        case "print": printTab(); break
        case "savePage": saveDialog.open(); break
        case "openFile": openDialog.open(); break
        case "devTools": if (t) t.toggleDevTools(); break
        case "bookmarkPage": if (locationBar) locationBar.toggleBookmark(); break
        case "bookmarkAllTabs": bookmarkAllTabs(); break
        case "bookmarksSidebar": showSidebar("bookmarks"); break
        case "historySidebar": showSidebar("history"); break
        case "bookmarksLibrary": app.showLibrary("bookmarks"); break
        case "historyLibrary": app.showLibrary("history"); break
        case "downloads": app.showDownloads(); break
        case "addons": app.showAddons(); break
        case "settings": app.showPreferences(); break
        default:
            if (/^tab[1-8]$/.test(id)) setCurrentIndex(Math.min(count - 1, Number(id.substring(3)) - 1))
            else runPluginAction(id)
        }
    }
    Instantiator {
        model: app.shortcutActions
        delegate: Shortcut {
            required property var modelData
            sequences: app.shortcutKeys(modelData.id, modelData.keys)
            onActivated: bw.runAction(modelData.id)
        }
    }
    // Only while the find bar is open: otherwise Escape must reach the page
    // (web apps and extensions such as Vimium rely on it).
    Shortcut { sequence: "Esc"; enabled: findBar.visible && !bw.contentFullScreen; onActivated: findBar.close() }
    // Escape leaves a page's fullscreen (as in other browsers).
    Shortcut { sequence: "Esc"; enabled: bw.contentFullScreen; onActivated: bw.exitContentFullScreen(false) }
    // Extension keyboard commands: the keys the extension's manifest gives
    // (extensions handle their own shortcut configuration).
    property var extCommands: JSON.parse(Extensions.commands(privateMode))
    Connections { target: Extensions; function onRevisionChanged() { bw.extCommands = JSON.parse(Extensions.commands(bw.privateMode)) } }
    Instantiator {
        model: bw.extCommands
        delegate: Shortcut {
            required property var modelData
            sequence: modelData.shortcut.replace("MacCtrl", "Ctrl").replace("Command", "Ctrl")
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
        id: headerBox
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
        id: footerSlot
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
            // Each tab's view sits in its pane's area; each pane shows its
            // current tab. (z: under what's shown over the page, wherever it moves.)
            Component {
                id: webTabComponent
                WebTab {
                    window: bw
                    app: bw.app
                    z: -1
                    parent: bw.paneArea(pane)
                    width: parent ? parent.width : 0
                    height: parent ? parent.height - (findBar.visible && findBar.parent === parent ? findBar.height : 0) : 0
                    visible: tabId === bw.paneTab[pane]
                }
            }
            // A tab's row only starts and ends its view. The view can't be the
            // Repeater's delegate itself: a removed delegate loses its QML context
            // at once but is deleted a moment later, and the result of a script
            // arriving in between crashes Qt (it asks that view for its engine).
            Repeater {
                model: tabModel
                delegate: Item {
                    required property int index
                    required property int tabId
                    required property string container
                    required property int pane
                    visible: false
                    property var tab: null
                    onIndexChanged: if (tab) tab.tabIndex = index
                    onPaneChanged: if (tab) tab.pane = pane
                    Component.onCompleted: tab = webTabComponent.createObject(bw, {tabId: tabId, container: container, pane: pane, tabIndex: index})
                    Component.onDestruction: if (tab) tab.retire()
                }
            }

            FindBar {
                id: findBar
                win: bw
                parent: bw.paneArea(bw.activePane)
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                tab: bw.currentTab
            }

            // Hovered link: at the bottom left, or the bottom right while the
            // pointer is where it would be on the left (so it doesn't cover
            // what's being pointed at).
            Rectangle {
                id: statusBubble
                parent: bw.paneArea(bw.activePane)
                z: 1
                property string text: bw.currentTab ? bw.currentTab.hoveredLink : ""
                visible: text !== "" && app.liveSettings["appearance.showStatusBar"]
                readonly property real margin: Kirigami.Units.gridUnit
                // The pointer over the page (WebTab.pointer), in this area.
                readonly property bool onRight: {
                    const t = bw.currentTab
                    if (!t || !t.pointerInside || !parent) return false
                    const p = t.view.mapToItem(parent, t.pointer.x, t.pointer.y)
                    const bottom = findBar.visible ? findBar.y : parent.height
                    return p.x <= width + margin && p.y >= bottom - height - margin
                }
                x: onRight ? parent.width - width : 0
                anchors.bottom: findBar.visible ? findBar.top : parent.bottom
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
                    QQC2.Label { id: toastLabel; textFormat: Text.PlainText; color: Kirigami.Theme.backgroundColor; wrapMode: Text.Wrap; Layout.fillWidth: true }
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

    function toggleTabSearch() { part(tabSearchPart).toggle() }
    function toggleCommandPalette() { part(commandPalettePart).toggle() }
    // Mouse gestures (bootstrap.cpp): the button the input filter holds back on
    // pages (a Qt.MouseButton, 0 = none), and what the pointer does with it.
    // A click without moving never shows up here: it reaches the page as usual.
    property int gestureButton: 0
    signal gestureStarted(real x, real y)
    signal gestureMoved(real x, real y)
    signal gestureFinished()
    signal gestureCancelled()

    // The window's part of plugins that are on. Sized zero: nothing here may
    // cover the pages (a plugin puts up what it shows itself).
    // The current tab a spot to the left (-1) or right (1), by keyboard.
    function stepCurrentTab(step) {
        if (!currentTab) return
        const id = currentTab.tabId
        const first = indexOfTab(id)
        const last = first + tabModel.get(first).treeCount
        // (In a flat list the tree's links wait; they don't hold tabs together.)
        const rows = treeRows().map(r => treeEnabled ? r : Object.assign(r, {parent: -1}))
        const target = TabTree.stepTarget(rows, first, last, step)
        if (!target) return
        if (target.leaveGroup) removeFromGroup(id)
        else moveRangeTo(first, last, target.to < first ? target.to : target.to + (last - first))
        groupsVersion++
        updateTree()
        reindex()
    }
    function findFromPage() { findBar.openReturningToPage() }
    function findStep(backward) { findBar.step(backward) }
    // A plugin's page script told the window something: {plugin, m}.
    signal pluginMessage(var tab, var message)
    signal tabClosed(int tabId)
    // (For plugins: a tab of this window, new or come from another one.)
    signal tabOpened(var tab)
    // (For plugins: a tab's page finished loading.)
    signal pageLoaded(var tab)
    // What plugins add to a page's context menu, for what it was opened on (`info`, as
    // extensions get it). A plugin's window item offers them with contextItems(info):
    // a list of {title, iconName, run} or {title, iconName, children}.
    function pluginContextItems(info) {
        let items = []
        for (let i = 0; i < pluginWindows.count; ++i) {
            const loader = pluginWindows.itemAt(i)
            if (loader.item && loader.item.contextItems) items = items.concat(loader.item.contextItems(info))
        }
        return items
    }
    // A plugin's item in this window, while it's on (else null).
    function pluginItem(pluginId) {
        for (let i = 0; i < pluginWindows.count; ++i) {
            const loader = pluginWindows.itemAt(i)
            if (loader.modelData.id === pluginId) return loader.item || null
        }
        return null
    }
    function runPluginAction(id) {
        for (let i = 0; i < pluginWindows.count; ++i) {
            const loader = pluginWindows.itemAt(i)
            if (loader.item && id.startsWith(loader.modelData.id + ".")) loader.item.runAction(id)
        }
    }
    // Buttons plugins put in the address bar: each window item's
    // `locationButtons`, a list of {icon, text, highlighted, run(button)} that it
    // keeps up to date (for the current tab, say). LocationBar.qml shows them.
    // `text` and `highlighted` may be functions: a new list makes new buttons
    // (closing a popup on one), so what changes while one is shown goes there.
    readonly property var pluginLocationButtons: {
        let out = []
        for (let i = 0; i < pluginWindows.count; ++i) {
            const loader = pluginWindows.itemAt(i)
            const item = loader ? loader.item : null
            if (item && Array.isArray(item.locationButtons)) out = out.concat(item.locationButtons)
        }
        return out
    }
    // Whether a plugin's action is on now, for actions that switch something
    // (the command palette's On/Off): its window's actionChecked(id), if it has
    // one; undefined for the rest.
    function pluginActionChecked(id) {
        for (let i = 0; i < pluginWindows.count; ++i) {
            const loader = pluginWindows.itemAt(i)
            if (loader.item && id.startsWith(loader.modelData.id + ".") && typeof loader.item.actionChecked === "function") return loader.item.actionChecked(id)
        }
        return undefined
    }
    Item {
        Repeater {
            id: pluginWindows
            model: bw.app.plugins.filter(p => p.window)
            delegate: Loader {
                id: pluginLoader
                required property var modelData
                function sync() {
                    if (!bw.app.pluginEnabled(modelData)) source = ""
                    else if (!item) setSource(bw.app.pluginUrl(modelData, modelData.window), {win: bw, app: bw.app, plugin: modelData})
                }
                Component.onCompleted: sync()
                Connections {
                    target: Browser
                    function onSettingChanged(key) { if (key === "plugins." + pluginLoader.modelData.id + ".enabled") pluginLoader.sync() }
                }
            }
        }
    }

    function saveSessionAs() { part(sessionDialogsPart).save() }
    function manageSessions() { part(sessionDialogsPart).manage() }

    // A tab group's menu (its chip or header).
    QQC2.Menu {
        id: groupMenu
        property string gid
        readonly property var info: bw.tabGroups[gid] || ({})
        QQC2.MenuItem { text: "Rename Group…"; icon.name: "edit-rename"; onTriggered: bw.renameGroup(groupMenu.gid) }
        QQC2.Menu {
            id: colorMenu
            title: "Colour"
            Instantiator {
                model: bw.groupColors
                delegate: QQC2.MenuItem {
                    required property string modelData
                    icon.name: "tag"
                    icon.color: modelData
                    text: ["Blue", "Red", "Yellow", "Green", "Pink", "Purple", "Cyan", "Orange", "Grey"][bw.groupColors.indexOf(modelData)]
                    checkable: true
                    checked: groupMenu.info.color === modelData
                    onTriggered: bw.updateGroup(groupMenu.gid, {color: modelData})
                }
                onObjectAdded: (i, o) => colorMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => colorMenu.removeItem(o)
            }
        }
        QQC2.MenuItem { text: groupMenu.info.collapsed ? "Expand Group" : "Collapse Group"; icon.name: groupMenu.info.collapsed ? "arrow-down" : "arrow-right"; onTriggered: bw.setGroupCollapsed(groupMenu.gid, !groupMenu.info.collapsed) }
        QQC2.MenuItem {
            text: "New Tab in Group"
            icon.name: "tab-new"
            onTriggered: {
                const members = bw.groupTabs(groupMenu.gid)
                const last = bw.indexOfTab(members[members.length - 1])
                bw.openTab(bw.app.newTabUrl(), {background: false, index: last + 1, pane: bw.paneOf(members[0]), group: groupMenu.gid}).focusLocationBar()
            }
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Ungroup"; icon.name: "object-ungroup"; onTriggered: bw.ungroup(groupMenu.gid) }
        QQC2.MenuItem { text: "Close Group"; icon.name: "tab-close"; onTriggered: bw.closeGroup(groupMenu.gid) }
    }
    Kirigami.PromptDialog {
        id: groupRename
        property string gid
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        title: "Name and Colour"
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        // The colour applies at once (the tabs show it); Cancel restores it.
        property string startColor: ""
        readonly property string color: (bw.tabGroups[gid] || {}).color || ""
        onOpened: { startColor = color; groupName.text = (bw.tabGroups[gid] || {}).name || ""; groupName.selectAll(); groupName.forceActiveFocus() }
        onAccepted: if (groupName.text.trim() !== "") bw.updateGroup(gid, {name: groupName.text.trim()})
        onRejected: bw.updateGroup(gid, {color: startColor})
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            QQC2.TextField { id: groupName; Layout.fillWidth: true; Layout.minimumWidth: Kirigami.Units.gridUnit * 14; onAccepted: groupRename.accept() }
            RowLayout {
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: bw.groupColors
                    delegate: QQC2.AbstractButton {
                        id: swatch
                        required property string modelData
                        required property int index
                        readonly property bool chosen: groupRename.color === modelData
                        implicitWidth: Kirigami.Units.iconSizes.medium
                        implicitHeight: implicitWidth
                        text: ["Blue", "Red", "Yellow", "Green", "Pink", "Purple", "Cyan", "Orange", "Grey"][index]
                        onClicked: bw.updateGroup(groupRename.gid, {color: modelData})
                        contentItem: Item {}
                        background: Rectangle {
                            radius: width / 2
                            color: swatch.modelData
                            // The chosen colour has a ring around it.
                            border.width: swatch.chosen ? 3 : swatch.hovered ? 2 : 0
                            border.color: swatch.chosen ? Kirigami.Theme.textColor : Qt.alpha(Kirigami.Theme.textColor, 0.5)
                        }
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        Accessible.role: Accessible.RadioButton
                        Accessible.checked: chosen
                    }
                }
            }
        }
    }

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
        title: "Save Page as PDF"
        fileMode: FileDialog.SaveFile
        nameFilters: ["PDF (*.pdf)"]
        currentFile: "file://" + app.setting("downloads.directory") + "/" + ((bw.currentTab ? bw.currentTab.title : "page").replace(/[\/\\:*?"<>|]/g, "_") || "page") + ".pdf"
        onAccepted: {
            const path = selectedFile.toString().replace("file://", "")
            bw.app.savePdf(bw.currentTab, path)
        }
    }

    Component.onCompleted: {
        if (app.setting("window.maximized")) showMaximized()
    }
}
