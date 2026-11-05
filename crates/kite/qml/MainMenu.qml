import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Window
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's "super menu".
QQC2.Menu {
    id: menu
    property var win
    property var app
    property var closedTabs: []
    property var closedWindows: []
    // Not closed by presses on the menu button, which toggles it.
    closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
    onAboutToShow: { closedTabs = JSON.parse(Browser.closedTabs()); closedWindows = JSON.parse(Browser.closedWindows()) }

    QQC2.MenuItem { text: "New Tab"; icon.name: "tab-new"; onTriggered: win.openTab(app.newTabUrl(), {background: false}).focusLocationBar() }
    QQC2.MenuItem { text: "New Window"; icon.name: "window-new"; onTriggered: app.newWindow() }
    QQC2.MenuItem { id: newPrivateWindow; text: "New Private Window"; icon.name: "view-private"; onTriggered: app.newWindow({privateMode: true}) }
    // (A property: a Menu declared inside a Menu is always added to it.)
    property ContainerMenu _containerMenu: ContainerMenu {
        title: "New Container Tab"
        app: menu.app
        parentMenu: menu
        after: newPrivateWindow
        onChosen: c => menu.win.openTab(menu.app.newTabUrl(), {background: false, container: c}).focusLocationBar()
    }
    QQC2.MenuItem { text: "Open File…"; icon.name: "document-open"; onTriggered: win.openFile() }
    QQC2.MenuItem { text: "Save Page As…"; icon.name: "document-save-as"; onTriggered: win.savePage() }
    QQC2.MenuItem { text: "Print…"; icon.name: "document-print"; onTriggered: win.printTab() }
    QQC2.MenuItem { text: "Save Page as PDF…"; icon.name: "application-pdf"; onTriggered: win.savePdf() }
    QQC2.MenuSeparator {}

    QQC2.Menu {
        title: "Bookmarks"
        icon.name: "bookmarks"
        QQC2.MenuItem { text: "Bookmark This Page"; icon.name: "bookmark-new"; onTriggered: win.toggleBookmarkFromMenu() }
        QQC2.MenuItem { text: "Bookmark All Tabs…"; icon.name: "bookmarks"; onTriggered: win.bookmarkAllTabs() }
        QQC2.MenuItem { text: "Manage Bookmarks"; icon.name: "bookmarks-organize"; onTriggered: app.showLibrary("bookmarks") }
        QQC2.MenuItem { text: "Bookmarks Sidebar"; icon.name: "view-sidetree"; onTriggered: win.showSidebar("bookmarks") }
        QQC2.MenuItem { text: "Show Bookmarks Toolbar"; checkable: true; checked: win.showBookmarksBar; onTriggered: Browser.setSetting("appearance.showBookmarksToolbar", JSON.stringify(checked)) }
        QQC2.MenuSeparator {}
        BookmarkFolderMenu { title: "Bookmarks Menu"; folderId: JSON.parse(Browser.bookmarkRoots()).menu; bar: win.bookmarksOpener }
        BookmarkFolderMenu { title: "Unsorted Bookmarks"; folderId: JSON.parse(Browser.bookmarkRoots()).unsorted; bar: win.bookmarksOpener }
    }
    QQC2.Menu {
        id: historyMenu
        title: "History"
        icon.name: "view-history"
        QQC2.MenuItem { text: "Show All History"; icon.name: "view-history"; onTriggered: app.showLibrary("history") }
        QQC2.MenuItem { text: "History Sidebar"; icon.name: "view-sidetree"; onTriggered: win.showSidebar("history") }
        QQC2.MenuItem { text: "Clear Recent History…"; icon.name: "edit-clear-history"; onTriggered: app.showPreferences("privacy") }
        QQC2.MenuSeparator {}
        QQC2.Menu {
            id: closedTabsMenu
            title: "Recently Closed Tabs"
            enabled: menu.closedTabs.length > 0
            Instantiator {
                model: menu.closedTabs
                delegate: QQC2.MenuItem { required property var modelData; required property int index; text: modelData.title || modelData.url; onTriggered: win.reopenClosedTab(index) }
                onObjectAdded: (i, o) => closedTabsMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => closedTabsMenu.removeItem(o)
            }
        }
        QQC2.Menu {
            id: closedWindowsMenu
            title: "Recently Closed Windows"
            enabled: menu.closedWindows.length > 0
            Instantiator {
                model: menu.closedWindows
                delegate: QQC2.MenuItem {
                    required property var modelData; required property int index
                    text: (modelData.tabs[modelData.current] || modelData.tabs[0] || {title: "Window"}).title + " (" + modelData.tabs.length + " tabs)"
                    onTriggered: { const w = JSON.parse(Browser.popClosedWindow(index)); if (w) app.restoreWindow(w) }
                }
                onObjectAdded: (i, o) => closedWindowsMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => closedWindowsMenu.removeItem(o)
            }
        }
    }
    QQC2.MenuItem { text: "Downloads"; icon.name: "download"; onTriggered: app.showDownloads() }
    QQC2.MenuItem { text: "Add-ons"; icon.name: "kite-extension"; onTriggered: app.showAddons() }
    QQC2.MenuSeparator {}

    QQC2.Menu {
        title: "View"
        icon.name: "view-visible"
        QQC2.MenuItem { text: "Zoom In"; icon.name: "zoom-in"; onTriggered: win.currentTab.zoomStep(1) }
        QQC2.MenuItem { text: "Zoom Out"; icon.name: "zoom-out"; onTriggered: win.currentTab.zoomStep(-1) }
        QQC2.MenuItem { text: "Reset Zoom"; icon.name: "zoom-original"; onTriggered: win.currentTab.setZoom(1.0) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Full Screen"; icon.name: "view-fullscreen"; checkable: true; checked: win.visibility === Window.FullScreen; onTriggered: checked ? win.showFullScreen() : win.showNormal() }
        QQC2.MenuItem { text: "Split View"; icon.name: "view-split-left-right"; checkable: true; checked: win.split; onTriggered: win.toggleSplit() }
        QQC2.MenuItem { text: "Reader View"; icon.name: "view-readermode"; enabled: win.currentTab && win.currentTab.canRead(); onTriggered: win.currentTab.toggleReader() }
        QQC2.MenuItem { text: "Page Source"; icon.name: "text-html"; onTriggered: win.openTab("view-source:" + win.currentTab.url, {background: false, afterCurrent: true}) }
        QQC2.MenuItem { text: "Developer Tools"; icon.name: "utilities-terminal"; onTriggered: win.currentTab.showDevTools() }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Bookmarks Sidebar"; checkable: true; checked: win.sidebar === "bookmarks"; onTriggered: win.showSidebar("bookmarks") }
        QQC2.MenuItem { text: "History Sidebar"; checkable: true; checked: win.sidebar === "history"; onTriggered: win.showSidebar("history") }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Menu Bar"; checkable: true; checked: !!app.liveSettings["appearance.showMenuBar"]; onTriggered: Browser.setSetting("appearance.showMenuBar", JSON.stringify(checked)) }
        QQC2.MenuItem { text: "Customize Toolbar…"; icon.name: "configure-toolbars"; onTriggered: win.customizeToolbar() }
    }
    QQC2.MenuItem { text: "Find in Page…"; icon.name: "edit-find"; onTriggered: win.openFindBar() }
    QQC2.MenuItem { text: "Settings"; icon.name: "settings-configure"; onTriggered: app.showPreferences() }
    QQC2.MenuSeparator {}
    QQC2.MenuItem { text: "About Kite"; icon.name: "help-about"; onTriggered: aboutDialog.open() }
    QQC2.MenuItem { text: "Quit"; icon.name: "application-exit"; onTriggered: app.quit() }

    property Kirigami.PromptDialog aboutDialog: Kirigami.PromptDialog {
        parent: menu.win.contentItem
        title: "About Kite"
        subtitle: "Kite " + Qt.application.version + "\n\nA KDE web browser written in Rust, inspired by Falkon, with native support for Firefox WebExtensions installed from addons.mozilla.org.\n\nRendering engine: QtWebEngine (Chromium).\nLicensed under the GNU GPL v3 or later."
        standardButtons: Kirigami.Dialog.Close
    }
}
