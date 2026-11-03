import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The tab strip, placed where "tabs.position" says: above or below
// the toolbars or below the page (horizontal), or as a list on the left
// (vertical). See BrowserWindow.qml.
Rectangle {
    id: stripRoot
    property var browser
    // "top" | "belowToolbar" | "bottom" | "vertical"
    property string position: "top"
    readonly property bool vertical: position === "vertical"
    // Side the current tab merges into: the page below it, or above it for
    // a strip under the page.
    readonly property string placement: vertical ? "vertical" : position === "bottom" ? "bottom" : "top"

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
            currentIndex: stripRoot.browser.currentIndex
            highlightFollowsCurrentItem: false
            boundsBehavior: Flickable.StopAtBounds
            readonly property real normalWidth: {
                const m = stripRoot.browser.tabs
                let pinned = 0
                for (let i = 0; i < m.count; ++i) if (m.get(i).pinned) pinned++
                // Leave room for the new-tab button at the end.
                const avail = width - pinned * Kirigami.Units.gridUnit * 2.4 - (footerItem ? footerItem.width : 0)
                return Math.max(Kirigami.Units.gridUnit * 5, Math.min(Kirigami.Units.gridUnit * 14, avail / Math.max(1, m.count - pinned)))
            }
            QQC2.ScrollBar.vertical: QQC2.ScrollBar { active: stripRoot.vertical; visible: stripRoot.vertical && strip.contentHeight > strip.height }
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
                width: stripRoot.vertical ? strip.width : implicitWidth
                height: stripRoot.vertical ? Kirigami.Units.gridUnit * 2 : strip.height
                onClicked: stripRoot.browser.openTab(stripRoot.browser.app.newTabUrl(), {background: false}).focusLocationBar()
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
            icon.name: "tab-duplicate"
            display: QQC2.AbstractButton.IconOnly
            text: "List all tabs"
            Layout.alignment: Qt.AlignCenter
            onClicked: tabListMenu.popup()
            QQC2.Menu {
                id: tabListMenu
                Instantiator {
                    model: stripRoot.browser.tabs
                    delegate: QQC2.MenuItem {
                        required property int index
                        required property string title
                        text: title
                        checkable: true
                        checked: index === stripRoot.browser.currentIndex
                        onTriggered: stripRoot.browser.setCurrentIndex(index)
                    }
                    onObjectAdded: (i, o) => tabListMenu.insertItem(i, o)
                    onObjectRemoved: (i, o) => tabListMenu.removeItem(o)
                }
                QQC2.MenuSeparator {}
                QQC2.MenuItem { text: "Reopen Closed Tab"; icon.name: "edit-undo"; onTriggered: stripRoot.browser.reopenClosedTab(0) }
            }
        }
    }
}
