import QtQuick
import QtQuick.Controls as QQC2
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.kde.kite

// Web page context menu, including extension `menus` entries.
QQC2.Menu {
    id: menu
    property var tab
    property var req: null
    property string linkUrl: ""
    property string mediaUrl: ""
    property int mediaType: 0
    property string selection: ""
    // The selected text, if it is an address without being a link on the page.
    property string selectionLink: ""
    property bool editable: false
    property var extItems: []
    property var info: ({})

    function show(request) {
        req = request
        linkUrl = request.linkUrl.toString()
        mediaUrl = request.mediaUrl.toString()
        mediaType = request.mediaType
        // (Triple-clicking takes the line's end along.)
        selection = request.selectedText.trim()
        selectionLink = !linkUrl && selection ? Browser.selectionUrl(selection) : ""
        editable = request.isContentEditable
        const contexts = []
        if (linkUrl) contexts.push("link")
        if (mediaType === ContextMenuRequest.MediaTypeImage) contexts.push("image")
        if (mediaType === ContextMenuRequest.MediaTypeVideo) contexts.push("video")
        if (mediaType === ContextMenuRequest.MediaTypeAudio) contexts.push("audio")
        if (selection) contexts.push("selection")
        if (editable) contexts.push("editable")
        if (contexts.length === 0) contexts.push("page")
        contexts.push("all")
        info = {contexts: contexts, pageUrl: tab.url, frameUrl: tab.url, linkUrl: linkUrl || undefined, linkText: request.linkText || undefined,
                srcUrl: mediaUrl || undefined, selectionText: selection || undefined, editable: editable,
                mediaType: mediaType === ContextMenuRequest.MediaTypeImage ? "image" : mediaType === ContextMenuRequest.MediaTypeVideo ? "video" : mediaType === ContextMenuRequest.MediaTypeAudio ? "audio" : undefined}
        extItems = tab.isExtensionPage ? [] : JSON.parse(Extensions.menus(JSON.stringify(Object.assign({incognito: tab.window.privateMode}, info))))
        popup()
    }

    function act(a) { tab.view.triggerWebAction(a) }

    // Navigation (page)
    QQC2.MenuItem { visible: !menu.linkUrl && !menu.selection && !menu.mediaUrl && !menu.editable; height: visible ? implicitHeight : 0; text: "Back"; icon.name: "go-previous"; enabled: menu.tab && menu.tab.view.canGoBack; onTriggered: menu.tab.view.goBack() }
    QQC2.MenuItem { visible: !menu.linkUrl && !menu.selection && !menu.mediaUrl && !menu.editable; height: visible ? implicitHeight : 0; text: "Forward"; icon.name: "go-next"; enabled: menu.tab && menu.tab.view.canGoForward; onTriggered: menu.tab.view.goForward() }
    QQC2.MenuItem { visible: !menu.linkUrl && !menu.selection && !menu.mediaUrl && !menu.editable; height: visible ? implicitHeight : 0; text: "Reload"; icon.name: "view-refresh"; onTriggered: menu.tab.view.reload() }
    QQC2.MenuItem { visible: !menu.linkUrl && !menu.selection && !menu.mediaUrl && !menu.editable; height: visible ? implicitHeight : 0; text: "Bookmark Page"; icon.name: "bookmark-new"; onTriggered: { Browser.addBookmark(menu.tab.url, menu.tab.title, ""); menu.tab.window.toast("Bookmarked", "bookmarks") } }
    QQC2.MenuItem { visible: !menu.linkUrl && !menu.selection && !menu.mediaUrl && !menu.editable; height: visible ? implicitHeight : 0; text: "Save Page As…"; icon.name: "document-save-as"; onTriggered: menu.tab.window.savePage() }

    // Links
    QQC2.MenuItem { id: openLinkInTab; visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Open Link in New Tab"; icon.name: "tab-new"; onTriggered: menu.tab.window.openTab(menu.linkUrl, {background: !menu.tab.app.setting("tabs.switchToNewTabFromLink"), index: menu.tab.window.linkTabIndex(menu.tab), openerTabId: menu.tab.tabId, fromLink: true, container: menu.tab.container}) }
    // Split view: in the other pane (starting split view), shown there while
    // this pane stays the active one.
    QQC2.MenuItem { visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Open Link in New Split Tab"; icon.name: "view-split-left-right"; onTriggered: menu.tab.window.openTab(menu.linkUrl, {background: true, showInPane: true, pane: 1 - menu.tab.pane, openerTabId: menu.tab.tabId, container: menu.tab.container}) }
    // (A property: a Menu declared inside a Menu is always added to it.)
    property ContainerMenu _containerMenu: ContainerMenu {
        title: "Open Link in Container Tab"
        app: menu.linkUrl && !menu.tab.window.privateMode ? menu.tab.app : null
        parentMenu: menu
        after: openLinkInTab
        onChosen: c => menu.tab.window.openTab(menu.linkUrl, {background: true, index: menu.tab.window.linkTabIndex(menu.tab), container: c})
    }
    QQC2.MenuItem { visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Open Link in New Window"; icon.name: "window-new"; onTriggered: menu.tab.app.newWindow({urls: [menu.linkUrl], privateMode: menu.tab.window.privateMode}) }
    QQC2.MenuItem { visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Open Link in Private Window"; icon.name: "view-private"; onTriggered: menu.tab.app.newWindow({urls: [menu.linkUrl], privateMode: true}) }
    QQC2.MenuItem { visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Bookmark Link"; icon.name: "bookmark-new"; onTriggered: Browser.addBookmark(menu.linkUrl, menu.req.linkText || menu.linkUrl, "") }
    QQC2.MenuItem { visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Save Link As…"; icon.name: "document-save"; onTriggered: menu.act(WebEngineView.DownloadLinkToDisk) }
    QQC2.MenuItem { visible: !!menu.linkUrl; height: visible ? implicitHeight : 0; text: "Copy Link"; icon.name: "edit-copy"; onTriggered: menu.act(WebEngineView.CopyLinkToClipboard) }

    // Media
    QQC2.MenuItem {
        visible: menu.mediaType === ContextMenuRequest.MediaTypeVideo && menu.tab.app.pipAvailable; height: visible ? implicitHeight : 0
        text: menu.tab && menu.tab.inPip ? "Back from Picture-in-Picture" : "Picture-in-Picture"; icon.name: "kite-pip"
        // The video under the pointer (request position is in view pixels).
        onTriggered: { const z = menu.tab.view.zoomFactor || 1; menu.tab.app.startPip(menu.tab, menu.req.position.x / z, menu.req.position.y / z) }
    }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeImage; height: visible ? implicitHeight : 0; text: "Open Image in New Tab"; icon.name: "tab-new"; onTriggered: menu.tab.window.openTab(menu.mediaUrl, {background: true, index: menu.tab.window.linkTabIndex(menu.tab), container: menu.tab.container}) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeImage; height: visible ? implicitHeight : 0; text: "Open Image in New Split Tab"; icon.name: "view-split-left-right"; onTriggered: menu.tab.window.openTab(menu.mediaUrl, {background: true, showInPane: true, pane: 1 - menu.tab.pane, container: menu.tab.container}) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeImage; height: visible ? implicitHeight : 0; text: "Copy Image"; icon.name: "edit-copy"; onTriggered: menu.act(WebEngineView.CopyImageToClipboard) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeImage; height: visible ? implicitHeight : 0; text: "Copy Image Address"; icon.name: "edit-copy"; onTriggered: menu.act(WebEngineView.CopyImageUrlToClipboard) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeImage; height: visible ? implicitHeight : 0; text: "Save Image As…"; icon.name: "document-save"; onTriggered: menu.act(WebEngineView.DownloadImageToDisk) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeVideo || menu.mediaType === ContextMenuRequest.MediaTypeAudio; height: visible ? implicitHeight : 0; text: "Save Media As…"; icon.name: "document-save"; onTriggered: menu.act(WebEngineView.DownloadMediaToDisk) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeVideo || menu.mediaType === ContextMenuRequest.MediaTypeAudio; height: visible ? implicitHeight : 0; text: "Copy Media Address"; icon.name: "edit-copy"; onTriggered: menu.act(WebEngineView.CopyMediaUrlToClipboard) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeVideo || menu.mediaType === ContextMenuRequest.MediaTypeAudio; height: visible ? implicitHeight : 0; text: "Play / Pause"; icon.name: "media-playback-start"; onTriggered: menu.act(WebEngineView.ToggleMediaPlayPause) }
    QQC2.MenuItem { visible: menu.mediaType === ContextMenuRequest.MediaTypeVideo; height: visible ? implicitHeight : 0; text: "Show Controls"; checkable: true; onTriggered: menu.act(WebEngineView.ToggleMediaControls) }

    // Selected text that is an address
    QQC2.MenuItem { visible: !!menu.selectionLink; height: visible ? implicitHeight : 0; text: "Open Link"; icon.name: "go-jump"; onTriggered: menu.tab.load(menu.selectionLink) }
    QQC2.MenuItem { visible: !!menu.selectionLink; height: visible ? implicitHeight : 0; text: "Open Link in New Tab"; icon.name: "tab-new"; onTriggered: menu.tab.window.openTab(menu.selectionLink, {background: !menu.tab.app.setting("tabs.switchToNewTabFromLink"), index: menu.tab.window.linkTabIndex(menu.tab), openerTabId: menu.tab.tabId, container: menu.tab.container}) }
    QQC2.MenuItem { visible: !!menu.selectionLink; height: visible ? implicitHeight : 0; text: "Open Link in Private Window"; icon.name: "view-private"; onTriggered: menu.tab.app.newWindow({urls: [menu.selectionLink], privateMode: true}) }
    QQC2.MenuSeparator { visible: !!menu.selectionLink; height: visible ? implicitHeight : 0 }

    // Editing / selection
    QQC2.MenuItem { visible: menu.editable; height: visible ? implicitHeight : 0; text: "Undo"; icon.name: "edit-undo"; onTriggered: menu.act(WebEngineView.Undo) }
    QQC2.MenuItem { visible: menu.editable; height: visible ? implicitHeight : 0; text: "Cut"; icon.name: "edit-cut"; onTriggered: menu.act(WebEngineView.Cut) }
    QQC2.MenuItem { visible: menu.editable || !!menu.selection; height: visible ? implicitHeight : 0; text: "Copy"; icon.name: "edit-copy"; onTriggered: menu.act(WebEngineView.Copy) }
    QQC2.MenuItem { visible: menu.editable; height: visible ? implicitHeight : 0; text: "Paste"; icon.name: "edit-paste"; onTriggered: menu.act(WebEngineView.Paste) }
    QQC2.MenuItem { visible: menu.editable; height: visible ? implicitHeight : 0; text: "Paste as Plain Text"; onTriggered: menu.act(WebEngineView.PasteAndMatchStyle) }
    QQC2.MenuItem { visible: menu.editable || !!menu.selection; height: visible ? implicitHeight : 0; text: "Select All"; icon.name: "edit-select-all"; onTriggered: menu.act(WebEngineView.SelectAll) }
    QQC2.MenuItem {
        visible: !!menu.selection; height: visible ? implicitHeight : 0
        text: "Search " + menu.tab.app.setting("general.searchEngine") + " for “" + (menu.selection.length > 24 ? menu.selection.substring(0, 24) + "…" : menu.selection) + "”"
        icon.name: "search"
        onTriggered: menu.tab.window.openTab(Browser.searchUrl(menu.tab.app.setting("general.searchEngine"), menu.selection), {background: false, index: menu.tab.window.linkTabIndex(menu.tab), container: menu.tab.container})
    }

    // Extension items
    QQC2.MenuSeparator { visible: menu.extItems.length > 0; height: visible ? implicitHeight : 0 }
    Instantiator {
        model: menu.extItems
        delegate: QtObject {
            required property var modelData
            property QtObject item: modelData.children && modelData.children.length > 0
                ? extSubComp.createObject(null, {node: modelData, ctxMenu: menu})
                : modelData.type === "separator" ? sepComp.createObject(null) : extItemComp.createObject(null, {node: modelData, ctxMenu: menu})
        }
        onObjectAdded: (i, o) => {
            const at = menu.count
            if (o.item instanceof QQC2.Menu) menu.insertMenu(at, o.item); else menu.insertItem(at, o.item)
        }
        onObjectRemoved: (i, o) => { if (o.item instanceof QQC2.Menu) menu.removeMenu(o.item); else menu.removeItem(o.item); o.item.destroy() }
    }
    function clickExt(node) {
        Extensions.menuClicked(node.ext, node.id, JSON.stringify(info), tab.tabId)
    }
    property Component extItemComp: Component {
        QQC2.MenuItem {
            property var node
            property var ctxMenu
            text: node.title
            enabled: node.enabled !== false
            checkable: node.type === "checkbox" || node.type === "radio"
            checked: !!node.checked
            icon.source: node.icon || ""
            icon.color: "transparent"
            onTriggered: ctxMenu.clickExt(node)
        }
    }
    property Component extSubComp: Component {
        QQC2.Menu {
            id: sub
            property var node
            property var ctxMenu
            title: node.title
            icon.source: node.icon || ""
            icon.color: "transparent"
            Instantiator {
                model: sub.node.children
                delegate: QtObject {
                    required property var modelData
                    property QtObject item: modelData.children && modelData.children.length > 0
                        ? menu.extSubComp.createObject(null, {node: modelData, ctxMenu: sub.ctxMenu})
                        : modelData.type === "separator" ? menu.sepComp.createObject(null) : menu.extItemComp.createObject(null, {node: modelData, ctxMenu: sub.ctxMenu})
                }
                onObjectAdded: (i, o) => { if (o.item instanceof QQC2.Menu) sub.insertMenu(i, o.item); else sub.insertItem(i, o.item) }
                onObjectRemoved: (i, o) => { if (o.item instanceof QQC2.Menu) sub.removeMenu(o.item); else sub.removeItem(o.item); o.item.destroy() }
            }
        }
    }
    property Component sepComp: Component { QQC2.MenuSeparator {} }

    QQC2.MenuSeparator {}
    QQC2.MenuItem { text: "View Page Source"; icon.name: "text-html"; onTriggered: menu.tab.window.openTab("view-source:" + menu.tab.url, {background: false, index: menu.tab.window.linkTabIndex(menu.tab), container: menu.tab.container}) }
    QQC2.MenuItem { text: "Inspect"; icon.name: "utilities-terminal"; onTriggered: { menu.act(WebEngineView.InspectElement); menu.tab.showDevTools() } }
}
