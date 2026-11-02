import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kite

// Lazily populated bookmark folder menu (recursive).
QQC2.Menu {
    id: menu
    property string folderId
    property var bar
    property var items: []
    onAboutToShow: items = JSON.parse(Browser.bookmarkTree(folderId))
    Instantiator {
        model: menu.items
        delegate: QtObject {
            required property var modelData
            property QtObject item: modelData.type === "folder"
                ? subComp.createObject(null, {title: modelData.title, folderId: modelData.id, bar: menu.bar})
                : modelData.type === "separator" ? sepComp.createObject(null) : itemComp.createObject(null, {node: modelData})
        }
        onObjectAdded: (i, o) => { if (o.item instanceof QQC2.Menu) menu.insertMenu(i, o.item); else menu.insertItem(i, o.item) }
        onObjectRemoved: (i, o) => { if (o.item instanceof QQC2.Menu) menu.removeMenu(o.item); else menu.removeItem(o.item); o.item.destroy() }
    }
    QQC2.MenuSeparator { visible: menu.items.length > 0 }
    QQC2.MenuItem {
        text: "Open All in Tabs"
        icon.name: "tab-new"
        enabled: menu.items.some(i => i.type === "url")
        onTriggered: { for (const i of menu.items) if (i.type === "url") menu.bar.win.openTab(i.url, {background: true}) }
    }
    property Component itemComp: Component {
        QQC2.MenuItem {
            property var node
            text: node.title || node.url
            icon.name: "text-html"
            onTriggered: menu.bar.open(node.url, 0)
        }
    }
    property Component subComp: Qt.createComponent("org.kde.kite", "BookmarkFolderMenu")
    property Component sepComp: Component { QQC2.MenuSeparator {} }
}
