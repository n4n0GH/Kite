import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami
import org.kde.kite

// The navigation toolbar, built from "toolbar.items" (see ToolbarItems.qml;
// changed with the Customize Toolbar dialog). The address bar, extension
// buttons, downloads button and menu button register themselves with the
// window (win.locationBar, …), which refers to them.
QQC2.ToolBar {
    id: toolbar
    property var win
    PrivateColors { target: toolbar; active: win.purple; palette: win.purpleColors.chrome }
    property ToolbarItems catalog
    position: QQC2.ToolBar.Header

    // Right-click on the toolbar: toolbar options.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: toolbarMenu.popup()
    }
    QQC2.Menu {
        id: toolbarMenu
        QQC2.MenuItem { text: "Menu Bar"; checkable: true; checked: !!toolbar.win.app.liveSettings["appearance.showMenuBar"]; onTriggered: Browser.setSetting("appearance.showMenuBar", JSON.stringify(checked)) }
        QQC2.MenuItem { text: "Bookmarks Toolbar"; checkable: true; checked: toolbar.win.showBookmarksBar; onTriggered: Browser.setSetting("appearance.showBookmarksToolbar", JSON.stringify(checked)) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Customize Toolbar…"; icon.name: "configure-toolbars"; onTriggered: toolbar.win.customizeToolbar() }
    }

    RowLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing
        Repeater {
            model: toolbar.catalog.sanitize(toolbar.win.app.liveSettings["toolbar.items"])
            delegate: Loader {
                required property string modelData
                Layout.fillWidth: modelData === "urlbar" || modelData === "flexibleSpace"
                Layout.fillHeight: true
                sourceComponent: ({
                    back: backItem, forward: forwardItem, reload: reloadItem, home: homeItem,
                    urlbar: urlbarItem, extensions: extensionsItem, downloads: downloadsItem, menu: menuItem,
                    newTab: newTabItem, newWindow: newWindowItem, privateWindow: privateWindowItem,
                    bookmarks: bookmarksItem, history: historyItem, sidebar: sidebarItem, print: printItem,
                    find: findItem, zoom: zoomItem, fullScreen: fullScreenItem, splitView: splitViewItem, addons: addonsItem,
                    settings: settingsItem, flexibleSpace: spaceItem,
                })[modelData] || null
            }
        }
    }

    // A plain toolbar button. (Each fills the toolbar's height, as the address bar
    // does: the Loader above sizes it, and those in a row of their own ask for it.)
    component Button: QQC2.ToolButton {
        property string tip
        display: QQC2.AbstractButton.IconOnly
        text: tip
        QQC2.ToolTip.text: tip; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }
    // A button with a menu: drawn pressed in for as long as the menu is open at it.
    component MenuButton: Button {
        id: withMenu
        property bool menuOpen: false
        checkable: menuOpen
        checked: menuOpen
        // (A click on a checkable button switches it by itself; it follows the menu again.)
        onToggled: checked = Qt.binding(() => withMenu.menuOpen)
        QQC2.ToolTip.visible: hovered && !menuOpen
    }

    readonly property var tab: win.currentTab

    Component {
        id: backItem
        MenuButton { id: backButton; icon.name: "go-previous"; tip: "Back"; enabled: !!toolbar.tab && toolbar.tab.view.canGoBack; menuOpen: toolbar.win.historyMenuAt === backButton; onClicked: toolbar.tab.view.goBack(); onPressAndHold: toolbar.win.showHistoryMenu(true, backButton) }
    }
    Component {
        id: forwardItem
        MenuButton { id: forwardButton; icon.name: "go-next"; tip: "Forward"; enabled: !!toolbar.tab && toolbar.tab.view.canGoForward; menuOpen: toolbar.win.historyMenuAt === forwardButton; onClicked: toolbar.tab.view.goForward(); onPressAndHold: toolbar.win.showHistoryMenu(false, forwardButton) }
    }
    Component {
        id: reloadItem
        Button {
            readonly property bool loading: !!toolbar.tab && toolbar.tab.loading
            icon.name: loading ? "process-stop" : "view-refresh"; tip: loading ? "Stop" : "Reload"
            onClicked: loading ? toolbar.tab.view.stop() : toolbar.tab.view.reload()
        }
    }
    Component {
        id: homeItem
        Button { icon.name: "go-home"; tip: "Home"; onClicked: toolbar.tab.load(toolbar.win.app.homeUrl()) }
    }
    Component {
        id: urlbarItem
        LocationBar {
            win: toolbar.win
            tab: toolbar.tab
            Component.onCompleted: toolbar.win.locationBar = this
        }
    }
    Component {
        id: extensionsItem
        ExtensionButtons {
            win: toolbar.win
            actions: toolbar.win.toolbarActions.filter(a => a.kind === "browserAction")
            Component.onCompleted: toolbar.win.extButtons = this
        }
    }
    Component {
        id: downloadsItem
        DownloadsButton {
            win: toolbar.win
            Component.onCompleted: toolbar.win.downloadsButton = this
        }
    }
    Component {
        id: menuItem
        MenuButton { id: menuButton; icon.name: "application-menu"; tip: "Open Menu"; menuOpen: toolbar.win.mainMenuAt === menuButton; onClicked: toolbar.win.openMainMenu(menuButton) }
    }
    Component {
        id: newTabItem
        Button { icon.name: "tab-new"; tip: "New Tab"; onClicked: toolbar.win.openTab(toolbar.win.app.newTabUrl(), {background: false}).focusLocationBar() }
    }
    Component {
        id: newWindowItem
        Button { icon.name: "window-new"; tip: "New Window"; onClicked: toolbar.win.app.newWindow() }
    }
    Component {
        id: privateWindowItem
        Button { icon.name: "view-private"; tip: "New Private Window"; onClicked: toolbar.win.app.newWindow({privateMode: true}) }
    }
    Component {
        id: bookmarksItem
        MenuButton {
            id: bookmarksButton
            icon.name: "bookmarks"; tip: "Bookmarks"
            menuOpen: bookmarksMenu.visible
            onClicked: bookmarksMenu.opened ? bookmarksMenu.close() : bookmarksMenu.popup(bookmarksButton, 0, bookmarksButton.height)
            QQC2.Menu {
                id: bookmarksMenu
                // The button toggles the menu.
                closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
                QQC2.MenuItem { text: "Bookmark This Page"; icon.name: "bookmark-new"; onTriggered: toolbar.win.toggleBookmarkFromMenu() }
                QQC2.MenuItem { text: "Manage Bookmarks"; icon.name: "bookmarks-organize"; onTriggered: toolbar.win.app.showLibrary("bookmarks") }
                QQC2.MenuSeparator {}
                BookmarkFolderMenu { title: "Bookmarks Toolbar"; folderId: JSON.parse(Browser.bookmarkRoots()).toolbar; bar: toolbar.win.bookmarksOpener }
                BookmarkFolderMenu { title: "Bookmarks Menu"; folderId: JSON.parse(Browser.bookmarkRoots()).menu; bar: toolbar.win.bookmarksOpener }
                BookmarkFolderMenu { title: "Unsorted Bookmarks"; folderId: JSON.parse(Browser.bookmarkRoots()).unsorted; bar: toolbar.win.bookmarksOpener }
            }
        }
    }
    Component {
        id: historyItem
        Button { icon.name: "document-open-recent"; tip: "History"; onClicked: toolbar.win.app.showLibrary("history") }
    }
    Component {
        id: sidebarItem
        Button { icon.name: "view-sidetree"; tip: "Sidebar"; checkable: true; checked: toolbar.win.sidebar !== ""; onClicked: toolbar.win.showSidebar(toolbar.win.sidebar !== "" ? toolbar.win.sidebar : "bookmarks") }
    }
    Component {
        id: printItem
        Button { icon.name: "document-print"; tip: "Print"; onClicked: toolbar.win.printTab() }
    }
    Component {
        id: findItem
        Button { icon.name: "edit-find"; tip: "Find in Page"; onClicked: toolbar.win.openFindBar() }
    }
    Component {
        id: zoomItem
        RowLayout {
            spacing: 0
            Button { icon.name: "zoom-out"; tip: "Zoom Out"; Layout.fillHeight: true; onClicked: toolbar.tab.zoomStep(-1) }
            QQC2.ToolButton {
                Layout.fillHeight: true
                text: toolbar.tab ? Math.round(toolbar.tab.view.zoomFactor * 100) + "%" : "100%"
                onClicked: toolbar.tab.resetZoom()
                QQC2.ToolTip.text: "Reset Zoom"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
            Button { icon.name: "zoom-in"; tip: "Zoom In"; Layout.fillHeight: true; onClicked: toolbar.tab.zoomStep(1) }
        }
    }
    Component {
        id: fullScreenItem
        Button { icon.name: "view-fullscreen"; tip: "Full Screen"; checkable: true; checked: toolbar.win.visibility === Window.FullScreen; onClicked: checked ? toolbar.win.showFullScreen() : toolbar.win.showNormal() }
    }
    Component {
        id: splitViewItem
        Button { icon.name: "view-split-left-right"; tip: "Split View"; checkable: true; checked: toolbar.win.split; onClicked: toolbar.win.toggleSplit() }
    }
    Component {
        id: addonsItem
        Button { icon.name: "kite-extension"; tip: "Extensions"; onClicked: toolbar.win.app.showAddons() }
    }
    Component {
        id: settingsItem
        Button { icon.name: "settings-configure"; tip: "Settings"; onClicked: toolbar.win.app.showPreferences() }
    }
    Component {
        id: spaceItem
        Item { implicitWidth: Kirigami.Units.gridUnit }
    }
}
