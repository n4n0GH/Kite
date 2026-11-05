import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's SideBar: bookmarks, history, or an extension's sidebar_action panel.
QQC2.Pane {
    id: root
    property var win
    property string mode: ""
    function loadExtSidebars() { return JSON.parse(Extensions.sidebars(!!win && win.privateMode)) }
    property var extSidebars: loadExtSidebars()
    Connections { target: Extensions; function onChanged(w) { if (w === "extensions" || w === "actions") root.extSidebars = root.loadExtSidebars() } }
    padding: 0

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            QQC2.ComboBox {
                id: picker
                Layout.fillWidth: true
                model: [{key: "bookmarks", title: "Bookmarks"}, {key: "history", title: "History"}].concat(root.extSidebars.map(s => ({key: "ext:" + s.ext, title: s.title})))
                textRole: "title"
                currentIndex: Math.max(0, model.findIndex(m => m.key === root.mode))
                onActivated: i => root.win.sidebar = model[i].key
            }
            QQC2.ToolButton { icon.name: "window-close"; onClicked: root.win.sidebar = "" }
        }
        Kirigami.SearchField {
            id: search
            visible: !root.mode.startsWith("ext:")
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            onTextChanged: list.reload()
        }
        ListView {
            id: list
            visible: !root.mode.startsWith("ext:")
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            property var rows: []
            property string folder: ""
            model: rows
            function reload() {
                if (root.mode === "history") {
                    rows = JSON.parse(Browser.history(search.text, 0, 0, 300)).map(h => ({type: "url", id: h.id, title: h.title || h.url, url: h.url, sub: new Date(h.date).toLocaleString(Qt.locale(), Locale.ShortFormat)}))
                } else if (root.mode === "bookmarks") {
                    if (search.text !== "") { rows = JSON.parse(Browser.searchBookmarks(search.text)); return }
                    if (folder === "") rows = JSON.parse(Browser.bookmarkRoots()).roots
                    else rows = [{type: "up", title: ".."}].concat(JSON.parse(Browser.bookmarkTree(folder)))
                }
            }
            Connections { target: root; function onModeChanged() { list.folder = ""; list.reload() } }
            Connections { target: Browser; function onBookmarksChanged() { list.reload() } function onHistoryChanged() {
                if (root.mode !== "history") return
                // (Deleting pages one after another keeps the list where it is.)
                const y = list.contentY
                list.reload()
                list.forceLayout()
                list.contentY = Math.max(list.originY, Math.min(y, list.originY + Math.max(0, list.contentHeight - list.height)))
            } }
            Component.onCompleted: reload()
            delegate: QQC2.ItemDelegate {
                id: row
                required property var modelData
                readonly property bool deletable: root.mode === "history" && modelData.id !== undefined
                width: ListView.view.width
                visible: modelData.type !== "separator"
                height: visible ? implicitHeight : 0
                icon.name: modelData.type === "folder" ? "folder-bookmark" : modelData.type === "up" ? "go-up" : root.mode === "history" ? "view-history" : "text-html"
                text: modelData.title || modelData.url || ""
                QQC2.ToolTip.text: modelData.url || ""
                QQC2.ToolTip.visible: hovered && !!modelData.url
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    if (modelData.type === "folder") { list.folder = modelData.id; list.reload() }
                    else if (modelData.type === "up") { list.folder = ""; list.reload() }
                    else if (root.win.currentTab) root.win.currentTab.load(modelData.url)
                }
                TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: if (modelData.url) root.win.openTab(modelData.url, {background: true}) }
                HoverHandler { id: rowHover }
                // Laid out with the × in it, so it lines up with the text, which
                // stops short of it, without touching the row's padding (the
                // theme's highlight follows that). Its place is kept when hidden.
                contentItem: RowLayout {
                    spacing: Kirigami.Units.smallSpacing * 2
                    Kirigami.Icon {
                        source: row.icon.name
                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                        Layout.preferredHeight: Layout.preferredWidth
                    }
                    QQC2.Label {
                        text: row.text
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    CloseButton {
                        visible: row.deletable
                        opacity: rowHover.hovered ? 1 : 0
                        enabled: rowHover.hovered
                        text: "Delete from History"
                        Layout.preferredWidth: implicitWidth
                        Layout.preferredHeight: implicitHeight
                        onClicked: Browser.deleteHistory(JSON.stringify([row.modelData.id]))
                    }
                }
            }
        }
        Loader {
            Layout.fillWidth: true
            Layout.fillHeight: true
            active: root.mode.startsWith("ext:")
            sourceComponent: WebEngineView {
                id: panel
                readonly property var info: root.extSidebars.find(s => "ext:" + s.ext === root.mode) || null
                profile: root.win.app.profile
                url: info ? info.panel : "about:blank"
                webChannel: WebChannel { registeredObjects: [sbBridge] }
                settings.localContentCanAccessRemoteUrls: true
                onNewWindowRequested: request => root.win.openTab(request.requestedUrl.toString(), {background: false})
                ExtBridge { id: sbBridge; viewKey: "sidebar:" + (panel.info ? panel.info.ext : ""); viewKind: "sidebar"; windowId: root.win.windowId; tabId: root.win.currentTab ? root.win.currentTab.tabId : -1 }
            }
        }
    }
}
