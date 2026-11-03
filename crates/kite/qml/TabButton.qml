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
    property var browser
    property var tabList
    // Where the strip is: "top" / "bottom" (horizontal, the current tab
    // merges into the toolbar or page on that side) or "vertical".
    property string placement: "top"
    readonly property bool vertical: placement === "vertical"
    readonly property bool current: index === browser.currentIndex

    width: vertical ? tabList.width : pinned ? Kirigami.Units.gridUnit * 2.4 : tabList.normalWidth
    height: vertical ? Kirigami.Units.gridUnit * 2 : tabList.height
    z: dragHandler.active ? 10 : 0

    // The tab's face fills its slot (the strip has no inner padding).
    readonly property rect face: Qt.rect(0, 0, width, height)

    // Drop slot: while a tab is dragged, its place in the strip (which moves
    // as the other tabs slide aside) shows where it will land.
    Rectangle {
        x: root.face.x + 1; y: root.face.y + 1
        width: root.face.width - 2; height: root.face.height - 2
        visible: dragHandler.active
        radius: Kirigami.Units.cornerRadius
        color: Qt.alpha(Kirigami.Theme.highlightColor, 0.12)
        border.color: Qt.alpha(Kirigami.Theme.highlightColor, 0.5)
    }

    Kirigami.ShadowedRectangle {
        id: bg
        // While dragging, the tab is lifted and follows the pointer even as
        // the strip moves it to a new index underneath; on release it
        // settles into its slot.
        readonly property bool lifted: dragHandler.active
        readonly property real dragOffset: lifted ? dragHandler.pos - (root.vertical ? root.y : root.x) : 0
        x: root.face.x + (root.vertical ? 0 : dragOffset)
        y: root.face.y + (root.vertical ? dragOffset : 0)
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
             : hover.hovered ? Qt.lighter(Kirigami.Theme.backgroundColor, 1.04) : "transparent"
        border.width: 1
        border.color: lifted || root.current ? Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.15) : "transparent"
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
        Rectangle {
            visible: root.current
            x: root.vertical ? 0 : 0
            y: root.vertical ? 0 : root.placement === "bottom" ? parent.height - 2 : 0
            width: root.vertical ? 3 : parent.width
            height: root.vertical ? parent.height : 2
            radius: 1
            color: Kirigami.Theme.highlightColor
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.smallSpacing * 2
            anchors.rightMargin: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.smallSpacing
            Item {
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                Layout.alignment: root.pinned ? Qt.AlignHCenter : Qt.AlignLeft
                Layout.fillWidth: root.pinned
                QQC2.BusyIndicator { anchors.fill: parent; running: root.loading; visible: root.loading; padding: 0 }
                Kirigami.Icon {
                    anchors.fill: parent
                    visible: !root.loading
                    source: root.icon !== "" ? root.icon : "text-html"
                    fallback: "text-html"
                }
            }
            QQC2.Label {
                visible: !root.pinned
                Layout.fillWidth: true
                text: root.title
                elide: Text.ElideRight
                opacity: root.current ? 1 : 0.8
            }
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
    QQC2.ToolTip.text: root.title
    QQC2.ToolTip.visible: hover.hovered && !dragHandler.active
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay * 2

    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: root.browser.setCurrentIndex(root.index)
        onDoubleTapped: {}
    }
    TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: root.browser.closeTab(root.index) }
    // Exclusive, so the tab bar's own menu doesn't open as well.
    TapHandler { acceptedButtons: Qt.RightButton; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: menu.popup() }

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
        xAxis.enabled: !root.vertical
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
        readonly property real along: root.vertical ? activeTranslation.y : activeTranslation.x
        readonly property real size: root.vertical ? root.height : root.width
        onActiveChanged: {
            if (active) {
                startPos = root.vertical ? root.y : root.x
                startScroll = root.tabList.scrollPos
                pos = startPos
                root.browser.setCurrentIndex(root.index)
            } else {
                root.tabList.autoScroll = 0
            }
        }
        onAlongChanged: if (active) Qt.callLater(update)
        function update() {
            if (!active) return
            pos = startPos + along + root.tabList.scrollPos - startScroll
            const centre = pos + size / 2
            let to = root.vertical ? root.tabList.indexAt(root.width / 2, centre) : root.tabList.indexAt(centre, root.height / 2)
            if (to < 0) to = centre < 0 ? 0 : root.browser.count - 1
            to = root.browser.clampToSection(root.index, to)
            if (to !== root.index) root.browser.moveTab(root.index, to)
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
        QQC2.MenuItem { text: "New Tab"; icon.name: "list-add"; onTriggered: root.browser.openTab(root.browser.app.newTabUrl(), {index: root.index + 1}) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Reload Tab"; icon.name: "view-refresh"; onTriggered: root.browser.tabById(root.tabId).view.reload() }
        QQC2.MenuItem { text: "Duplicate Tab"; icon.name: "tab-duplicate"; onTriggered: { const t = root.browser.tabById(root.tabId); root.browser.openTab(t.url, {index: root.index + 1}) } }
        QQC2.MenuItem { text: root.pinned ? "Unpin Tab" : "Pin Tab"; icon.name: "window-pin"; onTriggered: root.browser.tabById(root.tabId).setPinned(!root.pinned) }
        QQC2.MenuItem { text: root.muted ? "Unmute Tab" : "Mute Tab"; icon.name: root.muted ? "audio-volume-high" : "audio-volume-muted"; onTriggered: { const t = root.browser.tabById(root.tabId); t.view.audioMuted = !t.view.audioMuted } }
        QQC2.MenuItem { text: "Move to New Window"; icon.name: "window-new"; onTriggered: { const t = root.browser.tabById(root.tabId); root.browser.app.newWindow({urls: [t.url]}); root.browser.closeTab(root.index) } }
        QQC2.MenuItem { text: "Bookmark Tab…"; icon.name: "bookmark-new"; onTriggered: { const t = root.browser.tabById(root.tabId); Browser.addBookmark(t.url, t.title, ""); root.browser.toast("Bookmarked " + t.title, "bookmarks") } }
        QQC2.MenuItem { text: "Bookmark All Tabs"; icon.name: "bookmarks"; onTriggered: root.browser.bookmarkAllTabs() }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Close Other Tabs"; onTriggered: root.browser.closeOtherTabs(root.index) }
        QQC2.MenuItem { text: "Close Tabs to the Right"; onTriggered: root.browser.closeTabsToRight(root.index) }
        QQC2.MenuItem { text: "Reopen Closed Tab"; icon.name: "edit-undo"; onTriggered: root.browser.reopenClosedTab(0) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Close Tab"; icon.name: "tab-close"; onTriggered: root.browser.closeTab(root.index) }
        QQC2.MenuSeparator {}
        TabBarMenu { browser: root.browser }
    }
}
