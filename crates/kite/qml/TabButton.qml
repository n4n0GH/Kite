import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One tab in the tab tabList. Supports drag-to-reorder, middle-click close,
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
    readonly property bool current: index === browser.currentIndex

    width: pinned ? Kirigami.Units.gridUnit * 2.4 : tabList.normalWidth
    height: tabList.height
    z: dragHandler.active ? 10 : 0

    // Drop slot: while a tab is dragged, its place in the strip (which moves
    // as the other tabs slide aside) shows where it will land.
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: Kirigami.Units.smallSpacing
        anchors.margins: 1
        visible: dragHandler.active
        radius: Kirigami.Units.cornerRadius
        color: Qt.alpha(Kirigami.Theme.highlightColor, 0.12)
        border.color: Qt.alpha(Kirigami.Theme.highlightColor, 0.5)
    }

    Kirigami.ShadowedRectangle {
        id: bg
        // Only vertical anchors: horizontal ones would override x.
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.topMargin: Kirigami.Units.smallSpacing
        width: parent.width
        // While dragging, the tab is lifted and follows the pointer even as
        // the strip moves it to a new index underneath; on release it
        // settles into its slot.
        readonly property bool lifted: dragHandler.active
        x: lifted ? dragHandler.startX + dragHandler.activeTranslation.x - root.x : 0
        Behavior on x { enabled: !bg.lifted; NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic } }
        scale: lifted ? 1.04 : 1
        Behavior on scale { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        opacity: lifted ? 0.9 : 1
        shadow.size: lifted ? Kirigami.Units.gridUnit * 0.6 : 0
        shadow.yOffset: lifted ? 2 : 0
        shadow.color: Qt.rgba(0, 0, 0, 0.35)
        Behavior on shadow.size { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        radius: Kirigami.Units.cornerRadius
        color: lifted || root.current ? Kirigami.Theme.backgroundColor
             : hover.hovered ? Qt.lighter(Kirigami.Theme.backgroundColor, 1.04) : "transparent"
        border.width: 1
        border.color: lifted || root.current ? Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.15) : "transparent"
        Rectangle { visible: root.current; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: parent.radius + 1; color: parent.color }
        Rectangle { visible: root.current; anchors { left: parent.left; right: parent.right; top: parent.top } height: 2; radius: 1; color: Kirigami.Theme.highlightColor }

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
            QQC2.ToolButton {
                visible: !root.pinned && (hover.hovered || root.current)
                icon.name: "tab-close"
                Layout.preferredWidth: Kirigami.Units.iconSizes.small + Kirigami.Units.smallSpacing * 2
                Layout.preferredHeight: Layout.preferredWidth
                padding: 0
                onClicked: root.browser.closeTab(root.index)
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
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: menu.popup() }

    // Drag to reorder: the tab swaps places with its neighbours as its
    // centre passes over them (pinned and unpinned tabs stay in their own
    // sections).
    DragHandler {
        id: dragHandler
        target: null
        xAxis.enabled: true
        yAxis.enabled: false
        cursorShape: active ? Qt.ClosedHandCursor : Qt.ArrowCursor
        property real startX: 0
        onActiveChanged: {
            if (active) {
                startX = root.x
                root.browser.setCurrentIndex(root.index)
            }
        }
        onActiveTranslationChanged: {
            if (!active) return
            const centre = startX + activeTranslation.x + root.width / 2
            let to = root.tabList.indexAt(centre, root.height / 2)
            if (to < 0) to = centre < 0 ? 0 : root.browser.count - 1
            to = root.browser.clampToSection(root.index, to)
            if (to !== root.index) root.browser.moveTab(root.index, to)
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
    }
}
