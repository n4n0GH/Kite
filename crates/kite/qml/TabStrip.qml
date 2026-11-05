import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// The tab strip, placed where "tabs.position" says: above or below
// the toolbars or below the page (horizontal), or as a list on the left
// (vertical). See BrowserWindow.qml.
Rectangle {
    id: stripRoot
    property var browser
    // "top" | "belowToolbar" | "bottom" | "vertical" (left) | "verticalRight"
    property string position: "top"
    readonly property bool vertical: position === "vertical" || position === "verticalRight"
    // Split view: the pane whose tabs this strip shows (-1: all).
    property int pane: -1
    // New tabs from this strip go to its pane.
    readonly property int newTabPane: pane < 0 ? browser.activePane : pane
    // Side the current tab merges into: the page below it, or above it for
    // a strip under the page.
    readonly property string placement: vertical ? position : position === "bottom" ? "bottom" : "top"

    implicitHeight: vertical ? 0 : Kirigami.Units.gridUnit * 2.2
    implicitWidth: vertical ? Kirigami.Units.gridUnit * 14 : 0
    color: browser.privateMode ? Qt.darker(Kirigami.Theme.backgroundColor, 1.35) : Qt.darker(Kirigami.Theme.backgroundColor, 1.08)

    // Right-click on the bar (outside a tab): tab bar options.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: barMenu.popup()
    }
    TabBarMenu { id: barMenu; browser: stripRoot.browser; withTabActions: true }

    // The wheel scrolls a crowded tab bar (and the vertical list); when
    // all tabs fit, it switches tabs. (A button-less MouseArea on top gets
    // every wheel event while clicks, hover and drags reach the tabs; a
    // WheelHandler here receives none.)
    MouseArea {
        anchors.fill: parent
        z: 1
        acceptedButtons: Qt.NoButton
        onWheel: wheel => {
            const delta = wheel.angleDelta.y !== 0 ? wheel.angleDelta.y : wheel.angleDelta.x
            if (!stripRoot.vertical && !strip.overflowing) { stripRoot.browser.nextTab(delta > 0 ? -1 : 1); return }
            // One wheel step (120) scrolls about half a tab.
            const step = stripRoot.vertical ? Kirigami.Units.gridUnit * 2 : strip.normalWidth
            strip.scrollBy(-delta / 120 * step / 2)
        }
    }

    // Drops on the bar open a new tab where they land: a URL (link, image,
    // video, file) is opened, text (e.g. a selection on a page) is searched
    // with the default search engine.
    DropArea {
        id: dropArea
        anchors.fill: parent
        // Where the new tab would go (shown by dropMarker); -1 while nothing
        // usable is dragged over the bar.
        property int index: -1
        function usable(d) { return (d.hasUrls && d.urls.length > 0) || (d.hasText && d.text.trim() !== "") }
        function update(x, y) {
            const p = strip.mapFromItem(dropArea, x, y)
            if (p.x < 0 || p.y < 0 || p.x > strip.width || p.y > strip.height) { index = strip.count; return }
            const cx = p.x + strip.contentX, cy = p.y + strip.contentY
            const i = strip.indexAt(cx, cy)
            if (i < 0) { index = strip.count; return }
            const item = strip.itemAtIndex(i)
            const after = stripRoot.vertical ? cy - item.y > item.height / 2 : cx - item.x > item.width / 2
            index = i + (after ? 1 : 0)
        }
        onEntered: drag => {
            if (!usable(drag)) { drag.accepted = false; index = -1; return }
            drag.accept(Qt.CopyAction)
            update(drag.x, drag.y)
        }
        onPositionChanged: drag => { if (index >= 0) update(drag.x, drag.y) }
        onExited: index = -1
        onDropped: drop => {
            const at = index
            index = -1
            let url = ""
            if (drop.hasUrls && drop.urls.length > 0) {
                url = drop.urls[0].toString()
                // Not scripts (a dragged "javascript:" bookmarklet link).
                if (/^javascript:/i.test(url)) return
            } else if (drop.hasText && drop.text.trim() !== "") {
                url = Browser.searchUrl(stripRoot.browser.app.setting("general.searchEngine"), drop.text.trim())
            } else return
            // Copy, never move: a selection dragged out of a text field stays.
            drop.accept(Qt.CopyAction)
            stripRoot.browser.openTab(url, {background: false, index: at, pane: stripRoot.newTabPane})
        }
    }
    // Insertion marker for a drop.
    Rectangle {
        id: dropMarker
        z: 2
        visible: dropArea.index >= 0
        color: Kirigami.Theme.highlightColor
        readonly property var edge: {
            if (dropArea.index < 0) return Qt.point(0, 0)
            const n = strip.count
            const item = n === 0 ? null : strip.itemAtIndex(Math.min(dropArea.index, n - 1))
            if (!item) return strip.mapToItem(stripRoot, 0, 0)
            const end = dropArea.index >= n
            return stripRoot.vertical ? strip.mapToItem(stripRoot, 0, item.y - strip.contentY + (end ? item.height : 0))
                                      : strip.mapToItem(stripRoot, item.x - strip.contentX + (end ? item.width : 0), 0)
        }
        x: stripRoot.vertical ? edge.x : Math.max(0, edge.x - width / 2)
        y: stripRoot.vertical ? Math.max(0, edge.y - height / 2) : edge.y
        width: stripRoot.vertical ? strip.width : 2
        height: stripRoot.vertical ? 2 : strip.height
    }

    GridLayout {
        anchors.fill: parent
        flow: stripRoot.vertical ? GridLayout.TopToBottom : GridLayout.LeftToRight
        rows: stripRoot.vertical ? -1 : 1
        columns: stripRoot.vertical ? 1 : -1
        rowSpacing: 0
        columnSpacing: 0

        Kirigami.Icon {
            visible: stripRoot.browser.privateMode
            source: "view-private"
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: Kirigami.Units.iconSizes.small
            Layout.alignment: Qt.AlignCenter
            Layout.margins: Kirigami.Units.smallSpacing
        }

        // Scroll arrows, shown when there are more tabs than fit.
        QQC2.ToolButton {
            visible: strip.overflowing
            icon.name: "go-previous"
            display: QQC2.AbstractButton.IconOnly
            text: "Scroll tabs left"
            enabled: strip.contentX > strip.originX + 0.5
            autoRepeat: true
            Layout.fillHeight: true
            onClicked: strip.scrollBy(-strip.normalWidth)
        }

        ListView {
            id: strip
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: stripRoot.vertical ? ListView.Vertical : ListView.Horizontal
            model: stripRoot.browser.tabs
            // Dragging is for reordering tabs; the vertical list scrolls with
            // the wheel and its scroll bar.
            interactive: false
            clip: true
            // This strip's pane's current tab.
            readonly property int pane: stripRoot.pane
            currentIndex: { stripRoot.browser.currentIndex; return stripRoot.browser.indexOfTab(stripRoot.browser.paneTab[stripRoot.pane < 0 ? stripRoot.browser.activePane : stripRoot.pane]) }
            highlightFollowsCurrentItem: false
            boundsBehavior: Flickable.StopAtBounds
            readonly property real normalWidth: {
                const m = stripRoot.browser.tabs
                stripRoot.browser.rightPaneCount   // (tabs changing pane)
                let pinned = 0
                let shown = 0
                let groupTabs = 0   // group name tabs (collapsed or heading an open group)
                stripRoot.browser.tabGroups; stripRoot.browser.groupsVersion   // (collapsed groups)
                for (let i = 0; i < m.count; ++i) {
                    if (stripRoot.pane >= 0 && m.get(i).pane !== stripRoot.pane) continue
                    const g = m.get(i).group, info = g ? stripRoot.browser.tabGroups[g] : null
                    const first = info && stripRoot.browser.isGroupFirst(m.get(i).tabId)
                    if (info && info.collapsed && !first) continue
                    if (first) groupTabs++
                    if (first && info.collapsed) continue
                    shown++
                    if (m.get(i).pinned) pinned++
                }
                // Leave room for the new-tab button at the end.
                const avail = width - pinned * Kirigami.Units.gridUnit * 2.4 - groupTabs * Kirigami.Units.gridUnit * 8 - (footerItem ? footerItem.width : 0)
                const each = avail / Math.max(1, shown - pinned)
                // "tabs.fillWidth": tabs share the whole strip, however few;
                // otherwise they stop growing at a comfortable width.
                if (stripRoot.browser.app.liveSettings["tabs.fillWidth"]) return Math.max(Kirigami.Units.gridUnit * 5, Math.floor(each))
                return Math.max(Kirigami.Units.gridUnit * 5, Math.min(Kirigami.Units.gridUnit * 14, each))
            }
            QQC2.ScrollBar.vertical: QQC2.ScrollBar { id: verticalScrollBar; active: stripRoot.vertical; visible: stripRoot.vertical && strip.contentHeight > strip.height }
            // Width for tabs in the vertical list: the scroll bar, when shown,
            // gets its own lane instead of covering the close buttons.
            readonly property real itemWidth: width - (verticalScrollBar.visible ? verticalScrollBar.width : 0)
            moveDisplaced: Transition { NumberAnimation { properties: "x,y"; duration: Kirigami.Units.shortDuration } }
            readonly property bool overflowing: !stripRoot.vertical && contentWidth > width + 0.5
            readonly property real maxContentX: originX + Math.max(0, contentWidth - width)
            readonly property real maxContentY: originY + Math.max(0, contentHeight - height)
            // Scroll along the strip (clamped to its ends). Steps in quick
            // succession (wheel, arrows) add up: they continue from where a
            // running scroll animation is heading, not from where it is.
            function scrollBy(d) {
                if (stripRoot.vertical) { contentY = Math.max(originY, Math.min(contentY + d, maxContentY)); return }
                const from = scrollAnimation.enabled && scrollAnimationRunning ? scrollAnimation.targetValue : contentX
                contentX = Math.max(originX, Math.min(from + d, maxContentX))
            }
            property bool scrollAnimationRunning: false
            readonly property real scrollPos: stripRoot.vertical ? contentY : contentX
            // Set by a dragged tab near an end of the strip: scroll speed in
            // px per frame (negative: towards the start).
            property real autoScroll: 0
            Timer {
                interval: 16
                repeat: true
                running: strip.autoScroll !== 0
                onTriggered: strip.scrollBy(strip.autoScroll)
            }
            Behavior on contentX { id: scrollAnimation; enabled: !stripRoot.vertical && strip.autoScroll === 0; NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic; onRunningChanged: strip.scrollAnimationRunning = running } }
            // Fewer tabs or a wider window: don't leave the strip scrolled past
            // its end (nor let a running scroll animation take it there).
            onMaxContentXChanged: if (contentX > maxContentX || scrollAnimation.targetValue > maxContentX) contentX = maxContentX
            // A resized strip (e.g. the window reaching its size at startup)
            // keeps the current tab in view.
            onWidthChanged: Qt.callLater(ensureCurrentVisible)
            onHeightChanged: if (stripRoot.vertical) Qt.callLater(ensureCurrentVisible)
            // Keep the current tab in view (the last tab also brings the
            // new-tab button after it into view).
            function ensureCurrentVisible() {
                if (currentIndex < 0) return
                if (!stripRoot.vertical && currentIndex === count - 1) contentX = maxContentX
                else positionViewAtIndex(currentIndex, ListView.Contain)
            }
            onCurrentIndexChanged: Qt.callLater(ensureCurrentVisible)
            delegate: TabButton {
                browser: stripRoot.browser
                tabList: strip
                placement: stripRoot.placement
            }
            footer: QQC2.ToolButton {
                icon.name: "list-add"
                text: "New Tab"
                display: stripRoot.vertical ? QQC2.AbstractButton.TextBesideIcon : QQC2.AbstractButton.IconOnly
                width: stripRoot.vertical ? strip.itemWidth : implicitWidth
                height: stripRoot.vertical ? Kirigami.Units.gridUnit * 2 : strip.height
                onClicked: stripRoot.browser.openTab(stripRoot.browser.app.newTabUrl(), {background: false, pane: stripRoot.newTabPane}).focusLocationBar()
                QQC2.ToolTip.text: "New Tab (Ctrl+T)"
                QQC2.ToolTip.visible: hovered && !stripRoot.vertical
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        QQC2.ToolButton {
            visible: strip.overflowing
            icon.name: "go-next"
            display: QQC2.AbstractButton.IconOnly
            text: "Scroll tabs right"
            enabled: strip.contentX < strip.maxContentX - 0.5
            autoRepeat: true
            Layout.fillHeight: true
            onClicked: strip.scrollBy(strip.normalWidth)
        }

        QQC2.ToolButton {
            id: tabListButton
            icon.name: "tab-duplicate"
            display: QQC2.AbstractButton.IconOnly
            text: "List and Search Tabs"
            Layout.alignment: Qt.AlignCenter
            onClicked: stripRoot.browser.toggleTabSearch(tabListButton)
            QQC2.ToolTip.text: text + " (" + stripRoot.browser.app.shortcutKeys("tabSearch").map(k => Browser.keySequenceDisplay(k)).join(", ") + ")"
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
}
