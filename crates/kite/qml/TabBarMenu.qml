import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kite

// Tab bar options: where it is placed and whether it hides with a single
// tab ("tabs.position", "tabs.hideWhenSingle"). Opened by right-clicking the
// tab bar (with tab actions on top), and a "Tab Bar" submenu of each tab's
// context menu.
QQC2.Menu {
    id: menu
    property var browser
    property bool withTabActions: false
    title: "Tab Bar"

    readonly property var positions: [
        {k: "top", t: "Above the Address Bar"},
        {k: "belowToolbar", t: "Below the Address Bar"},
        {k: "bottom", t: "Below the Page"},
        {k: "vertical", t: "Vertical List on the Left"},
        {k: "verticalRight", t: "Vertical List on the Right"},
    ]

    // Checkable items lose their bindings when clicked; refresh on open.
    onAboutToShow: {
        for (let i = 0; i < positionItems.count; ++i) positionItems.objectAt(i).checked = menu.positions[i].k === menu.browser.tabPosition
        hideSingle.checked = !!menu.browser.app.liveSettings["tabs.hideWhenSingle"]
        treeStyle.checked = !!menu.browser.app.liveSettings["tabs.tree"]
    }

    QQC2.MenuItem {
        id: newTabItem
        visible: menu.withTabActions
        height: visible ? implicitHeight : 0
        text: "New Tab"
        icon.name: "list-add"
        onTriggered: menu.browser.openTab(menu.browser.app.newTabUrl(), {background: false}).focusLocationBar()
    }
    // (A property: a Menu declared inside a Menu is always added to it.)
    property ContainerMenu _containerMenu: ContainerMenu {
        title: "New Container Tab"
        app: menu.withTabActions && !menu.browser.privateMode ? menu.browser.app : null
        parentMenu: menu
        after: newTabItem
        onChosen: c => menu.browser.openTab(menu.browser.app.newTabUrl(), {background: false, container: c}).focusLocationBar()
    }
    QQC2.MenuItem {
        visible: menu.withTabActions
        height: visible ? implicitHeight : 0
        text: "Reopen Closed Tab"
        icon.name: "edit-undo"
        onTriggered: menu.browser.reopenClosedTab(0)
    }
    QQC2.MenuSeparator { visible: menu.withTabActions; height: visible ? implicitHeight : 0 }

    QQC2.ButtonGroup { id: positionGroup }
    Instantiator {
        id: positionItems
        model: menu.positions
        delegate: QQC2.MenuItem {
            required property var modelData
            text: modelData.t
            checkable: true
            QQC2.ButtonGroup.group: positionGroup
            onTriggered: menu.browser.setTabPosition(modelData.k)
        }
        onObjectAdded: (i, o) => menu.insertItem(i + 3, o)
        onObjectRemoved: (i, o) => menu.removeItem(o)
    }
    QQC2.MenuSeparator {}
    QQC2.MenuItem {
        id: hideSingle
        text: "Hide When Only One Tab Is Open"
        checkable: true
        onTriggered: Browser.setSetting("tabs.hideWhenSingle", JSON.stringify(checked))
    }
    QQC2.MenuItem {
        id: treeStyle
        text: "Tree-Style Tabs in Vertical Lists"
        checkable: true
        onTriggered: Browser.setSetting("tabs.tree", JSON.stringify(checked))
    }
}
