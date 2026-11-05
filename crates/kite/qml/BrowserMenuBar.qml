import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Window
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.kde.kite

// Classic menu bar (File, Edit, View, History, Bookmarks, Tools, Help), shown
// when "appearance.showMenuBar" is on, or briefly after tapping Alt
// (BrowserWindow.peekMenuBar).
QQC2.MenuBar {
    id: menuBar
    property var win
    readonly property var tab: win.currentTab
    readonly property var app: win.app
    function act(webAction) { if (tab) tab.view.triggerWebAction(webAction) }
    // A menu was used: a peeked menu bar goes away again.
    signal menuClosed()

    component BarMenu: QQC2.Menu { onClosed: menuBar.menuClosed() }

    BarMenu {
        title: "&File"
        QQC2.MenuItem { text: "New &Tab"; icon.name: "tab-new"; onTriggered: menuBar.win.openTab(menuBar.app.newTabUrl(), {background: false}).focusLocationBar() }
        QQC2.MenuItem { text: "New &Window"; icon.name: "window-new"; onTriggered: menuBar.app.newWindow() }
        QQC2.MenuItem { text: "New &Private Window"; icon.name: "view-private"; onTriggered: menuBar.app.newWindow({privateMode: true}) }
        QQC2.MenuSeparator {}
        ProfileMenu {}
        SessionMenu { win: menuBar.win }
        QQC2.MenuItem { text: "&Open File…"; icon.name: "document-open"; onTriggered: menuBar.win.openFile() }
        QQC2.MenuItem { text: "&Save Page As…"; icon.name: "document-save-as"; onTriggered: menuBar.win.savePage() }
        QQC2.MenuItem { text: "&Print…"; icon.name: "document-print"; onTriggered: menuBar.win.printTab() }
        QQC2.MenuItem { text: "Save Page as P&DF…"; icon.name: "application-pdf"; onTriggered: menuBar.win.savePdf() }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "&Close Tab"; icon.name: "tab-close"; onTriggered: menuBar.win.closeTab(menuBar.win.currentIndex) }
        QQC2.MenuItem { text: "Close W&indow"; icon.name: "window-close"; onTriggered: menuBar.win.close() }
        QQC2.MenuItem { text: "&Quit"; icon.name: "application-exit"; onTriggered: menuBar.app.quit() }
    }
    BarMenu {
        title: "&Edit"
        QQC2.MenuItem { text: "&Undo"; icon.name: "edit-undo"; onTriggered: menuBar.act(WebEngineView.Undo) }
        QQC2.MenuItem { text: "&Redo"; icon.name: "edit-redo"; onTriggered: menuBar.act(WebEngineView.Redo) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Cu&t"; icon.name: "edit-cut"; onTriggered: menuBar.act(WebEngineView.Cut) }
        QQC2.MenuItem { text: "&Copy"; icon.name: "edit-copy"; onTriggered: menuBar.act(WebEngineView.Copy) }
        QQC2.MenuItem { text: "&Paste"; icon.name: "edit-paste"; onTriggered: menuBar.act(WebEngineView.Paste) }
        QQC2.MenuItem { text: "Select &All"; icon.name: "edit-select-all"; onTriggered: menuBar.act(WebEngineView.SelectAll) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "&Find in Page…"; icon.name: "edit-find"; onTriggered: menuBar.win.openFindBar() }
    }
    BarMenu {
        title: "&View"
        QQC2.Menu {
            title: "&Toolbars"
            QQC2.MenuItem { text: "&Menu Bar"; checkable: true; checked: !!menuBar.app.liveSettings["appearance.showMenuBar"]; onTriggered: Browser.setSetting("appearance.showMenuBar", JSON.stringify(checked)) }
            QQC2.MenuItem { text: "&Bookmarks Toolbar"; checkable: true; checked: menuBar.win.showBookmarksBar; onTriggered: Browser.setSetting("appearance.showBookmarksToolbar", JSON.stringify(checked)) }
            QQC2.MenuSeparator {}
            QQC2.MenuItem { text: "&Customize Toolbar…"; icon.name: "configure-toolbars"; onTriggered: menuBar.win.customizeToolbar() }
        }
        QQC2.Menu {
            title: "&Sidebar"
            QQC2.MenuItem { text: "&Bookmarks"; checkable: true; checked: menuBar.win.sidebar === "bookmarks"; onTriggered: menuBar.win.showSidebar("bookmarks") }
            QQC2.MenuItem { text: "&History"; checkable: true; checked: menuBar.win.sidebar === "history"; onTriggered: menuBar.win.showSidebar("history") }
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Zoom &In"; icon.name: "zoom-in"; onTriggered: menuBar.tab.zoomStep(1) }
        QQC2.MenuItem { text: "Zoom &Out"; icon.name: "zoom-out"; onTriggered: menuBar.tab.zoomStep(-1) }
        QQC2.MenuItem { text: "&Reset Zoom"; icon.name: "zoom-original"; onTriggered: menuBar.tab.setZoom(1.0) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "&Full Screen"; icon.name: "view-fullscreen"; checkable: true; checked: menuBar.win.visibility === Window.FullScreen; onTriggered: checked ? menuBar.win.showFullScreen() : menuBar.win.showNormal() }
        QQC2.MenuItem { text: "S&plit View"; icon.name: "view-split-left-right"; checkable: true; checked: menuBar.win.split; onTriggered: menuBar.win.toggleSplit() }
        QQC2.MenuItem { text: "R&eader View"; icon.name: "view-readermode"; enabled: menuBar.tab && menuBar.tab.canRead(); onTriggered: menuBar.tab.toggleReader() }
        QQC2.MenuItem { text: "Page So&urce"; icon.name: "text-html"; onTriggered: menuBar.win.openTab("view-source:" + menuBar.tab.url, {background: false, afterCurrent: true}) }
    }
    BarMenu {
        title: "Hi&story"
        QQC2.MenuItem { text: "&Back"; icon.name: "go-previous"; enabled: !!menuBar.tab && menuBar.tab.view.canGoBack; onTriggered: menuBar.tab.view.goBack() }
        QQC2.MenuItem { text: "&Forward"; icon.name: "go-next"; enabled: !!menuBar.tab && menuBar.tab.view.canGoForward; onTriggered: menuBar.tab.view.goForward() }
        QQC2.MenuItem { text: "&Home"; icon.name: "go-home"; onTriggered: menuBar.tab.load(menuBar.app.homeUrl()) }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "Show &All History"; icon.name: "view-history"; onTriggered: menuBar.app.showLibrary("history") }
        QQC2.MenuItem { text: "Reopen Closed &Tab"; icon.name: "edit-undo"; onTriggered: menuBar.win.reopenClosedTab(0) }
        QQC2.MenuItem { text: "&Clear Recent History…"; icon.name: "edit-clear-history"; onTriggered: menuBar.app.showPreferences("privacy") }
    }
    BarMenu {
        title: "&Bookmarks"
        QQC2.MenuItem { text: "&Bookmark This Page"; icon.name: "bookmark-new"; onTriggered: menuBar.win.toggleBookmarkFromMenu() }
        QQC2.MenuItem { text: "Bookmark All &Tabs…"; icon.name: "bookmarks"; onTriggered: menuBar.win.bookmarkAllTabs() }
        QQC2.MenuItem { text: "&Manage Bookmarks"; icon.name: "bookmarks-organize"; onTriggered: menuBar.app.showLibrary("bookmarks") }
        QQC2.MenuSeparator {}
        BookmarkFolderMenu { title: "Bookmarks Toolbar"; folderId: JSON.parse(Browser.bookmarkRoots()).toolbar; bar: menuBar.win.bookmarksOpener }
        BookmarkFolderMenu { title: "Bookmarks Menu"; folderId: JSON.parse(Browser.bookmarkRoots()).menu; bar: menuBar.win.bookmarksOpener }
        BookmarkFolderMenu { title: "Unsorted Bookmarks"; folderId: JSON.parse(Browser.bookmarkRoots()).unsorted; bar: menuBar.win.bookmarksOpener }
    }
    BarMenu {
        title: "&Tools"
        QQC2.MenuItem { text: "&Downloads"; icon.name: "download"; onTriggered: menuBar.app.showDownloads() }
        QQC2.MenuItem { text: "&Add-ons"; icon.name: "kite-extension"; onTriggered: menuBar.app.showAddons() }
        QQC2.MenuItem { text: "&Passwords"; icon.name: "dialog-password"; onTriggered: menuBar.app.showPreferences("passwords") }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "&Developer Tools"; icon.name: "utilities-terminal"; onTriggered: menuBar.tab.showDevTools() }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { text: "&Settings"; icon.name: "settings-configure"; onTriggered: menuBar.app.showPreferences() }
    }
    BarMenu {
        title: "&Help"
        QQC2.MenuItem { text: "&About Kite"; icon.name: "help-about"; onTriggered: menuBar.win.showAbout() }
    }
}
