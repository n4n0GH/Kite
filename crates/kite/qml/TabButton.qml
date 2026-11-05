import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// One tab in the tab strip (horizontal or vertical, see TabStrip.qml). Supports drag-to-reorder, middle-click close,
// pinning, audio indicator and a context menu (Falkon's TabContextMenu).
Item {
    id: root
    required property int index
    required property int tabId
    required property string title
    required property string icon
    required property bool loading
    required property bool pinned
    required property bool audible
    required property bool muted
    required property bool unloaded
    // Container tab id ("privacy.containers"), and its name and colour.
    required property string container
    readonly property var containerInfo: container ? browser.app.containerInfo(container) : null
    property var browser
    property var tabList
    // Where the strip is: "top" / "bottom" (horizontal, the current tab
    // merges into the toolbar or page on that side) or "vertical".
    property string placement: "top"
    readonly property bool vertical: placement === "vertical" || placement === "verticalRight"
    // Split view: the pane the tab is in; a strip of one pane doesn't show
    // the other's tabs (they take no space).
    required property int pane
    readonly property bool inStrip: tabList.pane < 0 || tabList.pane === pane
    // The tab its pane shows (the active pane's is the window's current tab).
    readonly property bool current: browser.paneTab[pane] === tabId
    readonly property bool activePane: !browser.split || browser.activePane === pane
    // Selected with Ctrl/Shift+click (BrowserWindow.selectedTabs).
    readonly property bool selected: browser.selectedTabs.includes(tabId)

    // Tab groups: the group, whether this is its first tab (showing the
    // group's chip, its tree header, or the group itself when collapsed).
    required property string group
    readonly property var groupInfo: group && browser.tabGroups[group] ? browser.tabGroups[group] : null
    readonly property bool groupFirst: !!groupInfo && browser.isGroupFirst(tabId)
    readonly property bool groupCollapsed: !!groupInfo && groupInfo.collapsed
    readonly property bool hiddenInGroup: groupCollapsed && !groupFirst
    readonly property bool collapsedTab: groupFirst && groupCollapsed
    readonly property int groupSize: { browser.groupsVersion; return groupInfo ? browser.groupTabs(group).length : 0 }
    // Horizontal: the group's own tab in front of its tabs (all that is left
    // of it when collapsed); vertical: the tree header above it, and the
    // group's tabs indented under it.
    readonly property real groupTabWidth: groupFirst && !vertical ? Kirigami.Units.gridUnit * 8 : 0
    readonly property real headerHeight: groupFirst && vertical ? Kirigami.Units.gridUnit * 1.8 : 0
    readonly property real indent: groupInfo && vertical ? Kirigami.Units.gridUnit * 1.4 : 0

    // Tree-style tabs (BrowserWindow): how deep the tab is, how many tabs are
    // under it, and whether they are hidden or it is. (Deep trees stop
    // indenting so the tabs stay readable.)
    required property int treeDepth
    required property int treeCount
    required property bool treeCollapsed
    required property bool treeHidden
    readonly property real treeStep: Kirigami.Units.gridUnit * 0.9
    readonly property int shownDepth: Math.min(treeDepth, 6)
    readonly property bool hiddenTab: hiddenInGroup || treeHidden
    visible: inStrip && !hiddenTab

    width: !inStrip || hiddenTab ? 0 : vertical ? tabList.itemWidth : collapsedTab ? groupTabWidth : pinned ? Kirigami.Units.gridUnit * 2.4 : tabList.normalWidth + groupTabWidth
    height: !inStrip || hiddenTab ? 0 : vertical ? (collapsedTab ? headerHeight : Kirigami.Units.gridUnit * 2 + headerHeight) : tabList.height
    z: dragHandler.active ? 10 : 0

    // The tab's face: its slot, after the group's chip or header.
    readonly property real treeIndent: shownDepth * treeStep
    readonly property rect face: vertical ? Qt.rect(indent + treeIndent, headerHeight, width - indent - treeIndent, Kirigami.Units.gridUnit * 2) : Qt.rect(groupTabWidth, 0, width - groupTabWidth, height)

    function toggleGroup() { browser.setGroupCollapsed(group, !groupCollapsed) }

    // ---- the group's tab (horizontal): folder, name, arrow (down: collapsed,
    // right: expanded) and count; clicking it collapses or expands the group.
    Rectangle {
        visible: root.groupTabWidth > 0
        x: 1; y: 1
        width: root.groupTabWidth - 2
        height: parent.height - 2
        radius: Kirigami.Units.cornerRadius
        color: groupTabHover.hovered ? Qt.lighter(Kirigami.Theme.backgroundColor, 1.04) : "transparent"
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.smallSpacing * 2
            anchors.rightMargin: Kirigami.Units.smallSpacing * 2
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Icon { source: "folder"; isMask: true; color: root.groupInfo ? root.groupInfo.color : Kirigami.Theme.textColor; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
            QQC2.Label { text: root.groupInfo ? root.groupInfo.name : ""; Layout.fillWidth: true; elide: Text.ElideRight }
            Kirigami.Icon { source: root.groupCollapsed ? "arrow-down" : "arrow-right"; opacity: 0.7; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
            QQC2.Label { text: root.groupSize; opacity: 0.6; font: Kirigami.Theme.smallFont }
        }
        Rectangle {
            anchors { left: parent.left; right: parent.right }
            y: root.placement === "bottom" ? 0 : parent.height - height
            height: 2
            color: root.groupInfo ? root.groupInfo.color : "transparent"
        }
        // Exclusive, so the group's first tab isn't selected too.
        TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.toggleGroup() }
        TapHandler { acceptedButtons: Qt.RightButton; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.browser.openGroupMenu(root.group, parent) }
        HoverHandler { id: groupTabHover }
        QQC2.ToolTip.text: (root.groupInfo ? root.groupInfo.name : "") + (root.groupCollapsed ? ": click to expand" : ": click to collapse")
        QQC2.ToolTip.visible: groupTabHover.hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }

    // ---- tree header and guide line (vertical) ----------------------------
    Item {
        visible: root.groupFirst && root.vertical
        width: parent.width
        height: root.headerHeight
        Rectangle { anchors.fill: parent; anchors.margins: 1; radius: Kirigami.Units.cornerRadius; visible: headerHover.hovered; color: Qt.lighter(Kirigami.Theme.backgroundColor, 1.06) }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.smallSpacing
            anchors.rightMargin: Kirigami.Units.smallSpacing * 2
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Icon { source: root.groupCollapsed ? "arrow-right" : "arrow-down"; opacity: 0.7; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
            Kirigami.Icon { source: root.groupCollapsed ? "folder" : "folder-open"; isMask: true; color: root.groupInfo ? root.groupInfo.color : Kirigami.Theme.textColor; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
            QQC2.Label { text: root.groupInfo ? root.groupInfo.name : ""; font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
            QQC2.Label { text: root.groupSize; opacity: 0.6; font: Kirigami.Theme.smallFont }
        }
        HoverHandler { id: headerHover }
        TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.toggleGroup() }
        TapHandler { acceptedButtons: Qt.RightButton; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.browser.openGroupMenu(root.group, parent) }
    }
    Rectangle {
        visible: !!root.groupInfo && root.vertical && !root.groupCollapsed
        x: root.placement === "verticalRight" ? parent.width - Kirigami.Units.gridUnit * 0.7 : Kirigami.Units.gridUnit * 0.7
        y: root.headerHeight
        width: 1
        height: parent.height - root.headerHeight
        color: root.groupInfo ? root.groupInfo.color : "transparent"
    }

    // Tree-style tabs: a guide line for each level above the tab.
    Repeater {
        model: root.vertical && !root.hiddenTab ? root.shownDepth : 0
        delegate: Rectangle {
            required property int index
            x: root.indent + (index + 0.5) * root.treeStep
            y: root.headerHeight
            width: 1
            height: root.height - root.headerHeight
            color: Kirigami.Theme.textColor
            opacity: 0.2
        }
    }

    // Drop slot: while a tab is dragged, its place in the strip (which moves
    // as the other tabs slide aside) shows where it will land.
    // (In a tree, at the depth it would get.)
    Rectangle {
        readonly property real slotX: root.browser.treeEnabled ? root.indent + Math.min(dragHandler.dropDepth, 6) * root.treeStep : root.face.x
        x: slotX + 1; y: root.face.y + 1
        width: root.face.x + root.face.width - slotX - 2; height: root.face.height - 2
        visible: dragHandler.active
        radius: Kirigami.Units.cornerRadius
        color: Qt.alpha(Kirigami.Theme.highlightColor, 0.12)
        border.color: Qt.alpha(Kirigami.Theme.highlightColor, 0.5)
    }

    Kirigami.ShadowedRectangle {
        id: bg
        visible: !root.collapsedTab
        // While dragging, the tab is lifted and follows the pointer even as
        // the strip moves it to a new index underneath; on release it
        // settles into its slot.
        readonly property bool lifted: dragHandler.active
        readonly property real dragOffset: lifted ? dragHandler.pos - (root.vertical ? root.y : root.x) : 0
        x: root.face.x + (root.vertical ? 0 : dragOffset)
        // (Lifted, it stays under the pointer as a group's header comes or goes.)
        y: lifted ? dragHandler.startFaceY + (root.vertical ? dragOffset : 0) : root.face.y
        width: root.face.width
        height: root.face.height
        Behavior on x { enabled: !bg.lifted; NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic } }
        Behavior on y { enabled: !bg.lifted; NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic } }
        scale: lifted ? 1.04 : 1
        Behavior on scale { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        opacity: lifted ? 0.9 : 1
        shadow.size: lifted ? Kirigami.Units.gridUnit * 0.6 : 0
        shadow.yOffset: lifted ? 2 : 0
        shadow.color: Qt.rgba(0, 0, 0, 0.35)
        Behavior on shadow.size { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        radius: Kirigami.Units.cornerRadius
        // The current tab merges into the toolbar/page next to the strip:
        // square corners on that side, so its side borders reach the edge.
        readonly property bool merged: root.current && !root.vertical && !lifted
        corners.topLeftRadius: merged && root.placement === "bottom" ? 0 : -1
        corners.topRightRadius: merged && root.placement === "bottom" ? 0 : -1
        corners.bottomLeftRadius: merged && root.placement !== "bottom" ? 0 : -1
        corners.bottomRightRadius: merged && root.placement !== "bottom" ? 0 : -1
        color: lifted || root.current ? Kirigami.Theme.backgroundColor
             : root.selected ? Qt.alpha(Kirigami.Theme.highlightColor, hover.hovered ? 0.36 : 0.28)
             : hover.hovered ? Qt.lighter(Kirigami.Theme.backgroundColor, 1.04) : "transparent"
        border.width: 1
        border.color: root.selected ? Qt.alpha(Kirigami.Theme.highlightColor, 0.7)
                    : lifted || root.current ? Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.15) : "transparent"
        // ...and has no border line on that edge (only between its side
        // borders); it is marked on the opposite edge.
        Rectangle {
            visible: bg.merged
            anchors { left: parent.left; right: parent.right; leftMargin: parent.border.width; rightMargin: parent.border.width }
            anchors.top: root.placement === "bottom" ? parent.top : undefined
            anchors.bottom: root.placement === "bottom" ? undefined : parent.bottom
            height: parent.border.width
            color: parent.color
        }
        // The current tab's marker; container tabs always show theirs, in
        // the container's colour.
        Rectangle {
            visible: root.current || !!root.containerInfo
            // (Softer for the current tab of the pane that isn't active.)
            opacity: root.current && root.activePane ? 1 : 0.5
            // (A list on the right marks the side facing the page.)
            x: root.placement === "verticalRight" ? parent.width - width : 0
            y: root.vertical ? 0 : root.placement === "bottom" ? parent.height - 2 : 0
            width: root.vertical ? 3 : parent.width
            height: root.vertical ? parent.height : 2
            radius: 1
            color: root.containerInfo ? root.containerInfo.color : Kirigami.Theme.highlightColor
        }

        // A grouped tab's bar in the group's colour, on the side facing the page.
        Rectangle {
            visible: !!root.groupInfo && !root.vertical
            anchors { left: parent.left; right: parent.right }
            y: root.placement === "bottom" ? 0 : parent.height - height
            height: 2
            color: root.groupInfo ? root.groupInfo.color : "transparent"
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.smallSpacing * 2
            anchors.rightMargin: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.smallSpacing
            // Tree-style tabs: collapse or expand what is under the tab.
            Kirigami.Icon {
                visible: root.treeCount > 0
                source: root.treeCollapsed ? "arrow-right" : "arrow-down"
                opacity: twistyHover.hovered ? 1 : 0.7
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Layout.preferredWidth
                Accessible.role: Accessible.Button
                Accessible.name: root.treeCollapsed ? "Expand Tree" : "Collapse Tree"
                Accessible.onPressAction: root.browser.setTreeCollapsed(root.tabId, !root.treeCollapsed)
                HoverHandler { id: twistyHover }
                // Exclusive, so this doesn't also select the tab.
                TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.browser.setTreeCollapsed(root.tabId, !root.treeCollapsed) }
            }
            Item {
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                Layout.alignment: root.pinned ? Qt.AlignHCenter : Qt.AlignLeft
                Layout.fillWidth: root.pinned
                QQC2.BusyIndicator { anchors.fill: parent; running: root.loading; visible: root.loading; padding: 0 }
                TabIcon {
                    anchors.fill: parent
                    visible: !root.loading
                    // Unloaded tabs are drawn faded.
                    opacity: root.unloaded && !root.current ? 0.5 : 1
                    source: root.icon
                }
            }
            QQC2.Label {
                visible: !root.pinned
                Layout.fillWidth: true
                text: root.title
                elide: Text.ElideRight
                opacity: root.current ? 1 : root.unloaded ? 0.55 : 0.8
                font.italic: root.unloaded && !root.current
            }
            // How many tabs a collapsed tree holds.
            QQC2.Label { visible: root.treeCollapsed && root.treeCount > 0; text: root.treeCount; opacity: 0.6; font: Kirigami.Theme.smallFont }
            QQC2.ToolButton {
                visible: root.audible || root.muted
                icon.name: root.muted ? "audio-volume-muted" : "audio-volume-high"
                Layout.preferredWidth: Kirigami.Units.iconSizes.small + Kirigami.Units.smallSpacing * 2
                Layout.preferredHeight: Layout.preferredWidth
                padding: 0
                onClicked: { const t = root.browser.tabById(root.tabId); t.view.audioMuted = !t.view.audioMuted }
            }
            // Close button, drawn like Breeze's tab close indicator (as in
            // Dolphin): a plain × that becomes a white × on a red circle on
            // hover. Drawn here rather than taken from the icon theme, as
            // themes differ (e.g. Qogir's close icons are all red circles).
            Item {
                visible: !root.pinned && (hover.hovered || root.current)
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                Layout.rightMargin: Kirigami.Units.smallSpacing / 2
                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    visible: closeHover.hovered
                    color: Kirigami.Theme.negativeTextColor
                }
                Kirigami.Icon {
                    anchors.fill: parent
                    anchors.margins: closeHover.hovered ? 3 : 2
                    source: "window-close-symbolic"
                    isMask: true
                    color: closeHover.hovered ? "white" : Kirigami.Theme.textColor
                }
                Accessible.role: Accessible.Button
                Accessible.name: "Close Tab"
                Accessible.onPressAction: root.browser.closeTab(root.index)
                HoverHandler { id: closeHover }
                // Exclusive, so closing doesn't also select the tab.
                TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.browser.closeTab(root.index) }
                QQC2.ToolTip.text: "Close Tab"
                QQC2.ToolTip.visible: closeHover.hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }

    HoverHandler { id: hover }
    QQC2.ToolTip.text: root.containerInfo ? root.title + "\n" + root.containerInfo.name + " container" : root.title
    QQC2.ToolTip.visible: hover.hovered && !dragHandler.active
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay * 2

    // A click selects the tab; with Ctrl or Shift it selects several.
    TapHandler {
        acceptedButtons: Qt.LeftButton
        acceptedModifiers: Qt.NoModifier
        onTapped: root.collapsedTab ? root.toggleGroup() : root.browser.clickTab(root.index, Qt.NoModifier)
        onDoubleTapped: {}
    }
    TapHandler { acceptedButtons: Qt.LeftButton; acceptedModifiers: Qt.ControlModifier; onTapped: if (!root.collapsedTab) root.browser.clickTab(root.index, Qt.ControlModifier) }
    TapHandler { acceptedButtons: Qt.LeftButton; acceptedModifiers: Qt.ShiftModifier; onTapped: if (!root.collapsedTab) root.browser.clickTab(root.index, Qt.ShiftModifier) }
    TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: if (!root.collapsedTab) root.browser.closeTab(root.index) }
    // Exclusive, so the tab bar's own menu doesn't open as well.
    TapHandler { acceptedButtons: Qt.RightButton; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: root.collapsedTab ? root.browser.openGroupMenu(root.group, root) : menu.popup() }

    Connections {
        target: root.tabList
        enabled: dragHandler.active
        function onScrollPosChanged() { Qt.callLater(dragHandler.update) }
    }

    // Drag to reorder: the tab swaps places with its neighbours as its
    // centre passes over them (pinned and unpinned tabs stay in their own
    // sections).
    DragHandler {
        id: dragHandler
        target: null
        // (In a tree, sideways too: that changes the tab's depth.)
        xAxis.enabled: !root.vertical || root.browser.treeEnabled
        yAxis.enabled: root.vertical
        cursorShape: active ? Qt.ClosedHandCursor : Qt.ArrowCursor
        // Position along the strip at drag start, distance dragged, and how
        // far the strip has scrolled meanwhile give the dragged tab's
        // position in the strip's content (`pos`). It is updated once per
        // event-loop pass, as reacting to it moves tabs and scrolls the
        // strip, which would otherwise feed back into it.
        property real startPos: 0
        property real startScroll: 0
        property real pos: 0
        // Dragging the whole group: a collapsed group, or an open one by its
        // chip or tree header.
        property bool wholeGroup: false
        // Tree-style tabs: the depth at drag start, the one asked for by
        // dragging sideways, and the one the tab would get here.
        // Where its face was in it at drag start.
        property real startFaceY: 0
        property int startDepth: 0
        property int wantedDepth: 0
        property int dropDepth: 0
        readonly property real along: root.vertical ? activeTranslation.y : activeTranslation.x
        readonly property real size: root.vertical ? root.height : root.width
        onActiveChanged: {
            if (active) {
                startFaceY = root.face.y
                startDepth = wantedDepth = dropDepth = root.treeDepth
                root.browser.treeDragging = true
                startPos = root.vertical ? root.y : root.x
                startScroll = root.tabList.scrollPos
                pos = startPos
                const p = centroid.pressPosition
                wholeGroup = root.collapsedTab || (root.groupFirst && (root.vertical ? p.y < root.headerHeight : p.x < root.groupTabWidth))
                if (!wholeGroup) root.browser.setCurrentIndex(root.index)
            } else {
                root.tabList.autoScroll = 0
                root.browser.finishTabDrag(wholeGroup ? -1 : root.tabId, wantedDepth)
            }
        }
        onAlongChanged: if (active) Qt.callLater(update)
        onActiveTranslationChanged: if (active && root.browser.treeEnabled) Qt.callLater(update)
        function update() {
            if (!active) return
            pos = startPos + along + root.tabList.scrollPos - startScroll
            const centre = pos + size / 2
            let to = root.vertical ? root.tabList.indexAt(root.width / 2, centre) : root.tabList.indexAt(centre, root.height / 2)
            if (to < 0) to = centre < 0 ? 0 : root.browser.count - 1
            to = root.browser.clampToSection(root.index, to)
            if (wholeGroup) {
                const members = root.browser.groupTabs(root.group)
                const first = root.browser.indexOfTab(members[0]), last = root.browser.indexOfTab(members[members.length - 1])
                to = root.browser.dragTarget(first, last, to)
                if (to >= 0) root.browser.moveGroupTo(root.group, to)
            } else {
                // (Its descendants come along. Group membership and the place
                // in the tree follow when the drag ends.)
                const last = root.index + root.treeCount
                to = root.browser.dragTarget(root.index, last, to)
                if (to >= 0) {
                    const down = to > root.index
                    root.browser.moveRangeTo(root.index, last, to)
                    root.browser.dragGroupPreview(root.tabId, down)
                }
                wantedDepth = startDepth + Math.round(activeTranslation.x / root.treeStep)
                dropDepth = root.browser.treeDropDepth(root.index, root.index + root.treeCount, wantedDepth)
            }
            // Near an end of a crowded strip, scroll it (faster the further
            // the tab is pulled past the edge).
            const view = pos - root.tabList.scrollPos
            const length = root.vertical ? root.tabList.height : root.tabList.width
            const edge = size / 3
            let speed = 0
            if (view < edge) speed = -Math.min(20, 2 + (edge - view) / 4)
            else if (view + size > length - edge) speed = Math.min(20, 2 + (view + size - length + edge) / 4)
            root.tabList.autoScroll = speed
        }
    }

    QQC2.Menu {
        id: menu
        // The tabs it acts on: the selection, if this tab is in it.
        property var targets: [root.tabId]
        readonly property int n: targets.length
        readonly property string tabsWord: n > 1 ? n + " Tabs" : "Tab"
        // The targets that can be unloaded now (not shown, playing sound, …).
        property var unloadable: []
        onAboutToShow: {
            targets = root.browser.actionTabs(root.tabId)
            unloadable = targets.filter(id => { const t = root.browser.tabById(id); return t && t.keepLoadedReason() === "" })
        }
        function inGroup() { return targets.some(id => root.browser.groupOf(id) !== "") }
        QQC2.MenuItem { text: "New Tab"; icon.name: "list-add"; onTriggered: root.browser.openTab(root.browser.app.newTabUrl(), {index: root.index + 1}) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Reload Tab"; icon.name: "view-refresh"; onTriggered: root.browser.tabById(root.tabId).view.reload() }
        QQC2.MenuItem { id: duplicateItem; text: "Duplicate Tab"; icon.name: "tab-duplicate"; onTriggered: { const t = root.browser.tabById(root.tabId); root.browser.openTab(t.url, {container: t.container, index: root.index + 1}) } }
        // Split view: the tab to the other pane (starting split view).
        QQC2.MenuItem {
            text: root.browser.split ? (menu.n > 1 ? "Move " + menu.n + " Tabs to Other Side" : "Move to Other Side") : (menu.n > 1 ? "Open " + menu.n + " Tabs in Split View" : "Open in Split View")
            icon.name: "view-split-left-right"
            enabled: root.browser.split || root.browser.count > menu.n
            onTriggered: root.browser.moveTabsToOtherPane(menu.targets)
        }
        // Tab groups.
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            id: newGroupItem
            visible: !root.pinned && (menu.n > 1 || !root.group)
            height: visible ? implicitHeight : 0
            text: "Add " + menu.tabsWord + " to New Group"
            icon.name: "folder-new"
            onTriggered: { const gid = root.browser.newGroupFor(menu.targets); if (gid) root.browser.renameGroup(gid) }
        }
        Instantiator {
            // The other groups of the tab's pane.
            // (The groups not holding all of the tabs already.)
            model: { root.browser.groupsVersion; menu.targets; return root.pinned ? [] : Object.keys(root.browser.tabGroups).filter(g => root.browser.paneOf(root.browser.groupTabs(g)[0]) === root.pane && menu.targets.some(id => root.browser.groupOf(id) !== g)) }
            delegate: QQC2.MenuItem {
                required property string modelData
                text: (menu.n > 1 ? "Add " + menu.n + " Tabs to “" : "Add to “") + (root.browser.tabGroups[modelData] || {}).name + "”"
                icon.name: "tag"
                icon.color: (root.browser.tabGroups[modelData] || {}).color
                // (Once the menu is closed: this changes the groups it lists, which
                // would remove this very item while it is being triggered.)
                onTriggered: {
                    const targets = menu.targets, group = modelData, browser = root.browser
                    menu.close()
                    Qt.callLater(() => browser.addTabsToGroup(targets, group))
                }
            }
            onObjectAdded: (i, o) => { let at = 0; for (let j = 0; j < menu.count; ++j) if (menu.itemAt(j) === newGroupItem) at = j + 1; menu.insertItem(at + i, o) }
            onObjectRemoved: (i, o) => menu.removeItem(o)
        }
        QQC2.MenuItem {
            visible: { root.browser.groupsVersion; return menu.inGroup() }
            height: visible ? implicitHeight : 0
            text: menu.n > 1 ? "Remove " + menu.n + " Tabs from Group" : "Remove from Group"
            icon.name: "edit-delete-remove"
            onTriggered: root.browser.removeTabsFromGroup(menu.targets)
        }
        // (A property: a Menu declared inside a Menu is always added to it.)
        property ContainerMenu _containerMenu: ContainerMenu {
            title: "Reopen in Container"
            app: root.browser.privateMode ? null : root.browser.app
            parentMenu: menu
            after: duplicateItem
            onChosen: c => { const t = root.browser.tabById(root.tabId); root.browser.openTab(t.url, {container: c, index: root.index + 1, background: false}) }
        }
        // Tree-style tabs.
        QQC2.MenuItem {
            visible: root.treeCount > 0
            height: visible ? implicitHeight : 0
            text: root.treeCollapsed ? "Expand Tree" : "Collapse Tree"
            icon.name: root.treeCollapsed ? "arrow-down" : "arrow-right"
            onTriggered: root.browser.setTreeCollapsed(root.tabId, !root.treeCollapsed)
        }
        QQC2.MenuItem {
            visible: root.treeCount > 0
            height: visible ? implicitHeight : 0
            text: "Close Tree (" + (root.treeCount + 1) + " Tabs)"
            icon.name: "tab-close"
            onTriggered: root.browser.closeTabs(root.browser.treeTabs(root.tabId))
        }
        QQC2.MenuItem { text: root.pinned ? "Unpin Tab" : "Pin Tab"; icon.name: "window-pin"; onTriggered: root.browser.tabById(root.tabId).setPinned(!root.pinned) }
        QQC2.MenuItem {
            // Unloading frees a tab's memory; it reloads when shown again.
            text: menu.unloadable.length > 1 ? "Unload " + menu.unloadable.length + " Tabs" : "Unload Tab"
            icon.name: "media-playback-pause"
            enabled: menu.unloadable.length > 0
            onTriggered: { for (const id of menu.unloadable) root.browser.tabById(id).unload() }
        }
        QQC2.MenuItem { text: root.muted ? "Unmute Tab" : "Mute Tab"; icon.name: root.muted ? "audio-volume-high" : "audio-volume-muted"; onTriggered: { const t = root.browser.tabById(root.tabId); t.view.audioMuted = !t.view.audioMuted } }
        QQC2.MenuItem { text: "Move to New Window"; icon.name: "window-new"; onTriggered: { const t = root.browser.tabById(root.tabId); root.browser.app.newWindow({urls: [t.url]}); root.browser.closeTab(root.index) } }
        QQC2.MenuItem { text: "Bookmark Tab…"; icon.name: "bookmark-new"; onTriggered: { const t = root.browser.tabById(root.tabId); Browser.addBookmark(t.url, t.title, ""); root.browser.toast("Bookmarked " + t.title, "bookmarks") } }
        QQC2.MenuItem { text: "Bookmark All Tabs"; icon.name: "bookmarks"; onTriggered: root.browser.bookmarkAllTabs() }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Close Other Tabs"; onTriggered: root.browser.closeOtherTabs(root.index) }
        QQC2.MenuItem { text: "Close Tabs to the Right"; onTriggered: root.browser.closeTabsToRight(root.index) }
        QQC2.MenuItem { text: "Reopen Closed Tab"; icon.name: "edit-undo"; onTriggered: root.browser.reopenClosedTab(0) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Close " + menu.tabsWord; icon.name: "tab-close"; onTriggered: root.browser.closeTabs(menu.targets) }
        QQC2.MenuSeparator {}
        TabBarMenu { browser: root.browser }
    }
}
