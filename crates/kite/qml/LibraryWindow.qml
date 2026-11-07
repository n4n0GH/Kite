import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's BrowsingLibrary: History and Bookmarks managers.
ManagerWindow {
    id: w
    property string page: "history"
    title: "Library — Kite"
    width: Kirigami.Units.gridUnit * 52
    height: Kirigami.Units.gridUnit * 36
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.ToolBar
    pageStack.initialPage: page === "bookmarks" ? bookmarksPage : historyPage
    onPageChanged: pageStack.replace(page === "bookmarks" ? bookmarksPage : historyPage)

    function openUrl(u, bg) { app.openUrl(u, bg ? "background" : "tab") }

    globalDrawer: Kirigami.GlobalDrawer {
        modal: false
        collapsible: true
        collapsed: true
        showHeaderWhenCollapsed: true
        actions: [
            Kirigami.Action { text: "History"; icon.name: "view-history"; checked: w.page === "history"; onTriggered: w.page = "history" },
            Kirigami.Action { text: "Bookmarks"; icon.name: "bookmarks"; checked: w.page === "bookmarks"; onTriggered: w.page = "bookmarks" }
        ]
    }

    Component {
        id: historyPage
        Kirigami.ScrollablePage {
            id: hp
            title: "History"
            property var rows: []
            property var selected: ({})
            function reload() {
                const now = Date.now()
                const day = 86400000
                let start = 0
                if (range.currentIndex === 1) start = new Date().setHours(0, 0, 0, 0)
                else if (range.currentIndex === 2) start = now - 7 * day
                else if (range.currentIndex === 3) start = now - 30 * day
                rows = JSON.parse(Browser.history(search.text, start, 0, 2000))
                selected = {}
            }
            Component.onCompleted: reload()
            Connections { target: Browser; function onHistoryChanged() { hp.reload() } }
            header: QQC2.ToolBar {
                RowLayout {
                    anchors.fill: parent
                    Kirigami.SearchField { id: search; Layout.fillWidth: true; onTextChanged: hp.reload() }
                    QQC2.ComboBox { id: range; model: ["All time", "Today", "Last 7 days", "Last 30 days"]; onActivated: hp.reload() }
                    QQC2.Button { text: "Delete Selected"; icon.name: "edit-delete"; enabled: Object.keys(hp.selected).length > 0; onClicked: Browser.deleteHistory(JSON.stringify(Object.keys(hp.selected).map(Number))) }
                    QQC2.Button { text: "Clear All History…"; icon.name: "edit-clear-history"; onClicked: confirmClear.open() }
                }
            }
            Kirigami.PromptDialog {
                id: confirmClear
                title: "Clear All History"
                subtitle: "This removes every page from your browsing history. This cannot be undone."
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                onAccepted: Browser.clearHistory()
            }
            ListView {
                model: hp.rows
                section.property: "day"
                section.delegate: Kirigami.ListSectionHeader { required property string section; width: ListView.view.width; text: section }
                Kirigami.PlaceholderMessage { anchors.centerIn: parent; visible: hp.rows.length === 0; text: "No history"; icon.name: "view-history" }
                delegate: QQC2.ItemDelegate {
                    required property var modelData
                    readonly property string day: new Date(modelData.date).toLocaleDateString(Qt.locale(), Locale.LongFormat)
                    width: ListView.view.width
                    highlighted: !!hp.selected[modelData.id]
                    contentItem: RowLayout {
                        QQC2.CheckBox { checked: !!hp.selected[modelData.id]; onToggled: { const s = Object.assign({}, hp.selected); if (checked) s[modelData.id] = true; else delete s[modelData.id]; hp.selected = s } }
                        ColumnLayout {
                            spacing: 0
                            Layout.fillWidth: true
                            QQC2.Label { text: Browser.displayUrl(modelData.url); color: Kirigami.Theme.linkColor; elide: Text.ElideMiddle; Layout.fillWidth: true }
                            QQC2.Label { visible: !!modelData.title; text: modelData.title; opacity: 0.7; elide: Text.ElideRight; font: Kirigami.Theme.smallFont; Layout.fillWidth: true }
                        }
                        QQC2.Label { text: new Date(modelData.date).toLocaleString(Qt.locale(), Locale.ShortFormat) + "  ·  " + modelData.count + "×"; opacity: 0.7 }
                    }
                    onDoubleClicked: w.openUrl(modelData.url, false)
                    onClicked: w.openUrl(modelData.url, false)
                    TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: w.openUrl(modelData.url, true) }
                }
            }
        }
    }

    Component {
        id: bookmarksPage
        Kirigami.ScrollablePage {
            id: bp
            title: "Bookmarks"
            property var roots: JSON.parse(Browser.bookmarkRoots())
            property string folder: roots.toolbar
            property var path: [{id: roots.toolbar, title: "Bookmarks Toolbar"}]
            property var rows: []
            property var editing: null
            function reload() {
                rows = search.text !== "" ? JSON.parse(Browser.searchBookmarks(search.text)) : JSON.parse(Browser.bookmarkTree(folder))
            }
            function enter(id, title, reset) {
                folder = id
                path = reset ? [{id: id, title: title}] : path.concat([{id: id, title: title}])
                reload()
            }
            Component.onCompleted: reload()
            Connections { target: Browser; function onBookmarksChanged() { bp.reload() } }
            header: ColumnLayout {
                spacing: 0
                QQC2.ToolBar {
                    Layout.fillWidth: true
                    RowLayout {
                        anchors.fill: parent
                        QQC2.ComboBox {
                            model: bp.roots.roots
                            textRole: "title"
                            onActivated: i => bp.enter(bp.roots.roots[i].id, bp.roots.roots[i].title, true)
                        }
                        Kirigami.SearchField { id: search; Layout.fillWidth: true; onTextChanged: bp.reload() }
                        QQC2.Button { text: "New Folder"; icon.name: "folder-new"; onClicked: { newFolder.text = ""; folderDialog.open() } }
                        QQC2.Button { text: "Separator"; icon.name: "distribute-vertical-center"; onClicked: Browser.addSeparator(bp.folder) }
                        QQC2.Button {
                            id: importButton
                            text: "Import…"
                            icon.name: "document-import"
                            onClicked: importMenu.popup(importButton, 0, importButton.height)
                            QQC2.Menu {
                                id: importMenu
                                QQC2.MenuItem { text: "From Another Browser…"; icon.name: "internet-web-browser"; onTriggered: browserImport.start() }
                                QQC2.MenuItem { text: "From a File…"; icon.name: "document-open"; onTriggered: importDialog.open() }
                            }
                        }
                        QQC2.Button { text: "Export…"; icon.name: "document-export"; onClicked: exportDialog.open() }
                    }
                }
                RowLayout {
                    Layout.margins: Kirigami.Units.smallSpacing
                    visible: search.text === ""
                    Repeater {
                        model: bp.path
                        delegate: QQC2.ToolButton {
                            required property var modelData; required property int index
                            text: modelData.title
                            icon.name: index === 0 ? "bookmarks" : "go-next"
                            onClicked: { bp.path = bp.path.slice(0, index + 1); bp.folder = modelData.id; bp.reload() }
                        }
                    }
                }
            }
            // The browsers found on this machine, each profile by itself.
            Kirigami.Dialog {
                id: browserImport
                title: "Import Bookmarks from Another Browser"
                padding: Kirigami.Units.largeSpacing
                standardButtons: Kirigami.Dialog.NoButton
                property var sources: []
                property int chosen: 0
                function start() {
                    sources = JSON.parse(Browser.bookmarkSources())
                    chosen = 0
                    open()
                }
                customFooterActions: [
                    Kirigami.Action {
                        text: "Import"
                        icon.name: "document-import"
                        enabled: browserImport.sources.length > 0
                        onTriggered: {
                            const source = browserImport.sources[browserImport.chosen].source
                            browserImport.close()
                            w.app.toast(Browser.importBookmarksFrom(JSON.stringify(source)))
                        }
                    },
                    Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: browserImport.close() }
                ]
                ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 24
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: browserImport.sources.length
                            ? "They go into a new folder in the Bookmarks Menu. Those you already have stay out, so you can do this again later."
                            : "No other browsers found on this computer. If yours exports bookmarks to a file, you can import that instead."
                    }
                    Repeater {
                        model: browserImport.sources
                        delegate: QQC2.RadioButton {
                            required property var modelData
                            required property int index
                            text: modelData.label
                            checked: browserImport.chosen === index
                            onToggled: browserImport.chosen = index
                        }
                    }
                }
            }
            FileDialog { id: importDialog; title: "Import Bookmarks (HTML or Falkon bookmarks.json)"; nameFilters: ["Bookmarks (*.html *.htm *.json)"]; onAccepted: w.app.toast(Browser.importBookmarks(selectedFile.toString())) }
            FileDialog { id: exportDialog; title: "Export Bookmarks"; fileMode: FileDialog.SaveFile; nameFilters: ["HTML (*.html)"]; currentFile: "file://" + w.app.setting("downloads.directory") + "/bookmarks.html"; onAccepted: w.app.toast(Browser.exportBookmarks(selectedFile.toString())) }
            Kirigami.PromptDialog {
                id: folderDialog
                title: "New Folder"
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                QQC2.TextField { id: newFolder; placeholderText: "Folder name" }
                onAccepted: if (newFolder.text) Browser.addFolder(bp.folder, newFolder.text)
            }
            Kirigami.Dialog {
                id: editDialog
                title: "Edit Bookmark"
                standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
                padding: Kirigami.Units.largeSpacing
                Kirigami.FormLayout {
                    QQC2.TextField { id: eTitle; Kirigami.FormData.label: "Name:"; Layout.preferredWidth: Kirigami.Units.gridUnit * 20 }
                    QQC2.TextField { id: eUrl; Kirigami.FormData.label: "Address:"; visible: bp.editing && bp.editing.type === "url" }
                    QQC2.TextField { id: eKeyword; Kirigami.FormData.label: "Keyword:"; visible: bp.editing && bp.editing.type === "url"; placeholderText: "Type this in the location bar to open the bookmark" }
                    QQC2.TextField { id: eDesc; Kirigami.FormData.label: "Description:"; visible: bp.editing && bp.editing.type === "url" }
                }
                onAccepted: Browser.updateBookmark(bp.editing.id, JSON.stringify({title: eTitle.text, url: bp.editing.type === "url" ? eUrl.text : undefined, keyword: eKeyword.text, description: eDesc.text}))
            }
            ListView {
                model: bp.rows
                Kirigami.PlaceholderMessage { anchors.centerIn: parent; visible: bp.rows.length === 0; text: "This folder is empty"; icon.name: "bookmarks" }
                moveDisplaced: Transition { NumberAnimation { properties: "y"; duration: Kirigami.Units.shortDuration } }
                delegate: QQC2.ItemDelegate {
                    id: row
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    icon.name: modelData.type === "folder" ? "folder-bookmark" : modelData.type === "separator" ? "" : "text-html"
                    contentItem: RowLayout {
                        Kirigami.Icon { source: row.icon.name; visible: row.icon.name !== ""; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                        Kirigami.Separator { visible: row.modelData.type === "separator"; Layout.fillWidth: true }
                        ColumnLayout {
                            visible: row.modelData.type !== "separator"
                            spacing: 0
                            Layout.fillWidth: true
                            QQC2.Label { text: row.modelData.title + (row.modelData.type === "folder" ? "  (" + row.modelData.children + ")" : ""); elide: Text.ElideRight; Layout.fillWidth: true }
                            QQC2.Label { visible: !!row.modelData.url; text: row.modelData.url + (row.modelData.keyword ? "   ⌨ " + row.modelData.keyword : ""); color: Kirigami.Theme.linkColor; elide: Text.ElideMiddle; font: Kirigami.Theme.smallFont; Layout.fillWidth: true }
                        }
                        QQC2.ToolButton { icon.name: "go-up"; visible: row.index > 0 && search.text === ""; onClicked: Browser.moveBookmark(row.modelData.id, bp.folder, row.index - 1) }
                        QQC2.ToolButton { icon.name: "go-down"; visible: row.index < bp.rows.length - 1 && search.text === ""; onClicked: Browser.moveBookmark(row.modelData.id, bp.folder, row.index + 2) }
                        QQC2.ToolButton {
                            icon.name: "document-edit"; visible: row.modelData.type !== "separator"
                            onClicked: { bp.editing = row.modelData; eTitle.text = row.modelData.title; eUrl.text = row.modelData.url || ""; eKeyword.text = row.modelData.keyword || ""; eDesc.text = row.modelData.description || ""; editDialog.open() }
                        }
                        QQC2.ToolButton {
                            icon.name: "folder-move"; visible: row.modelData.type !== "separator"
                            onClicked: { moveMenu.folders = JSON.parse(Browser.bookmarkFolders()).filter(f => f.id !== row.modelData.id); moveMenu.popup() }
                            QQC2.Menu {
                                id: moveMenu
                                property var folders: []
                                title: "Move to"
                                Instantiator {
                                    model: moveMenu.folders
                                    delegate: QQC2.MenuItem { required property var modelData; text: "   ".repeat(modelData.depth) + modelData.title; onTriggered: Browser.moveBookmark(row.modelData.id, modelData.id, -1) }
                                    onObjectAdded: (i, o) => moveMenu.insertItem(i, o)
                                    onObjectRemoved: (i, o) => moveMenu.removeItem(o)
                                }
                            }
                        }
                        QQC2.ToolButton { icon.name: "edit-delete"; onClicked: Browser.removeBookmark(row.modelData.id) }
                    }
                    onClicked: {
                        if (modelData.type === "folder") bp.enter(modelData.id, modelData.title, false)
                        else if (modelData.type === "url") w.openUrl(modelData.url, false)
                    }
                    TapHandler { acceptedButtons: Qt.MiddleButton; onTapped: if (row.modelData.url) w.openUrl(row.modelData.url, true) }
                }
            }
        }
    }
}
