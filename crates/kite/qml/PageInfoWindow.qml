import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.kde.kite
import "mediafile.js" as MediaFile

// Page Info: what a tab's page is made of (General, Media), what its site may
// do (Permissions: every one there is, where the address bar's popup has only
// the ones allowed or in use), and how it is connected and what Kite keeps of
// it (Security). As Firefox's and Falkon's windows of that name; laid out as
// the Library is, a sidebar with a page for each.
//
// What the page says of itself comes from a script run in it when the window
// asks (js/pageinfo.js). All of that is the page's to make up, so it is shown
// as plain text only: in text fields, and labels with textFormat PlainText.
ManagerWindow {
    id: w
    // The tab asked about (a WebTab). The window follows it from page to page,
    // and goes when the tab does.
    property Item tab
    property string section: "general"

    title: "Page Info — " + (info.title || url || "Kite")
    width: Kirigami.Units.gridUnit * 42
    height: Kirigami.Units.gridUnit * 32
    // (As the Library: more with the sidebar's names showing.)
    minimumWidth: Kirigami.Units.gridUnit * (globalDrawer.collapsed ? 26 : 32)
    minimumHeight: Kirigami.Units.gridUnit * 18
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.ToolBar
    function pageComponent() { return section === "media" ? mediaPage : section === "permissions" ? permissionsPage : section === "security" ? securityPage : generalPage }
    pageStack.initialPage: pageComponent()
    // (Not for the page it's opened with, which comes before there is one to replace.)
    onSectionChanged: if (pageStack.depth > 0) pageStack.replace(pageComponent())

    globalDrawer: Kirigami.GlobalDrawer {
        modal: false
        collapsible: true
        collapsed: true
        showHeaderWhenCollapsed: true
        actions: [
            Kirigami.Action { text: "General"; icon.name: "documentinfo"; checked: w.section === "general"; onTriggered: w.section = "general" },
            Kirigami.Action { text: "Media"; icon.name: "image-x-generic"; checked: w.section === "media"; onTriggered: w.section = "media" },
            Kirigami.Action { text: "Permissions"; icon.name: "dialog-object-properties"; checked: w.section === "permissions"; onTriggered: w.section = "permissions" },
            Kirigami.Action { text: "Security"; icon.name: "security-high"; checked: w.section === "security"; onTriggered: w.section = "security" }
        ]
    }

    // A button in a page's bar. While the page is too narrow for the whole bar (compact), it's
    // only its icon, with its text as the tooltip. (As in LibraryWindow.qml.)
    component BarButton: QQC2.Button {
        property bool compact: false
        display: compact ? QQC2.AbstractButton.IconOnly : QQC2.AbstractButton.TextBesideIcon
        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: compact && hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }

    // ---- what is shown ----------------------------------------------------------
    property string url: ""
    property var info: ({})
    property var media: []
    // The one chosen in the Media list (-1: none), kept while other pages show.
    property int mediaIndex: -1
    property var permissions: []
    property var facts: ({})
    property int logins: 0
    readonly property string host: Browser.hostOf(url)
    readonly property bool isSite: /^https?:/i.test(url)
    readonly property bool inPrivate: !!tab && tab.window.privateMode

    function load() {
        const t = tab
        if (!t) return
        url = t.url
        info = {title: t.title}
        media = []
        mediaIndex = -1
        loadPermissions()
        facts = isSite && !inPrivate ? JSON.parse(Browser.pageSiteFacts(url)) : {}
        logins = isSite && !inPrivate ? JSON.parse(Passwords.logins(url)).length : 0
        // (A tab that wasn't loaded yet has no page to ask.)
        if (t.pendingUrl !== "") return
        const asked = url
        t.view.runJavaScript(Browser.pageInfoScript(), 1, json => {
            if (w.tab !== t || w.url !== asked || typeof json !== "string") return
            let said
            try { said = JSON.parse(json) } catch (e) { return }
            said.title = said.title || t.title
            w.media = Array.isArray(said.media) ? said.media : []
            w.info = said
        })
    }
    function loadPermissions() { permissions = tab ? tab.sitePermissions() : [] }

    Component.onCompleted: load()
    onTabChanged: { if (!tab) close(); else load() }
    Connections {
        target: w.tab
        function onLoadingChanged() { if (!w.tab.loading) w.load() }
        function onUrlChanged() { if (!w.tab.loading) w.load() }
    }
    // (They may have been changed elsewhere meanwhile: the address bar's popup.)
    onActiveChanged: if (active) loadPermissions()

    // ---- saving media: by the page's own profile (MediaSaver.qml) ----
    readonly property var saver: app ? app.mediaSaver : null
    readonly property var savableMedia: {
        const seen = new Set(), out = []
        for (const m of media) if (MediaFile.savable(m.url) && !seen.has(m.url)) { seen.add(m.url); out.push(m) }
        return out
    }
    function saveOne(m) {
        saveFile.item = m
        saveFile.currentFile = "file://" + app.setting("downloads.directory") + "/" + MediaFile.fileName(m.url, m.kind)
        saveFile.open()
    }
    function saveAll() {
        saveFolder.currentFolder = "file://" + app.setting("downloads.directory")
        saveFolder.open()
    }
    function localPath(u) { return decodeURIComponent(u.toString().replace(/^file:\/\//, "")) }
    FileDialog {
        id: saveFile
        property var item: null
        title: "Save Media"
        fileMode: FileDialog.SaveFile
        onAccepted: if (w.tab && item) w.app.saveMedia(w.tab.view.profile, [{url: item.url, path: w.localPath(selectedFile), unique: false}])
    }
    // (Each under its own name there, or one like it if that's taken.)
    FolderDialog {
        id: saveFolder
        title: "Save All Media To"
        onAccepted: {
            if (!w.tab) return
            const folder = w.localPath(selectedFolder).replace(/\/+$/, "")
            w.app.saveMedia(w.tab.view.profile, w.savableMedia.map(m => ({url: m.url, path: folder + "/" + MediaFile.fileName(m.url, m.kind), unique: true})))
        }
    }

    // ---- how it is put ----------------------------------------------------------
    function protocolName(p) {
        return {"h2": "HTTP/2", "h3": "HTTP/3", "http/1.1": "HTTP/1.1", "http/1.0": "HTTP/1.0"}[p] || p || ""
    }
    function sizeText() {
        if (!(info.size > 0)) return ""
        const sent = info.transferred > 0 && info.transferred !== info.size ? " (" + app.formatBytes(info.transferred) + " transferred)" : ""
        return app.formatBytes(info.size) + sent
    }
    function modifiedText() {
        const d = info.modified ? new Date(info.modified) : null
        return d && !isNaN(d) ? d.toLocaleString(Qt.locale(), Locale.LongFormat) : ""
    }
    function kindName(k) { return {image: "Image", icon: "Icon", background: "Background", video: "Video", audio: "Audio", embedded: "Embedded"}[k] || "Other" }
    function kindIcon(k) { return {video: "video-x-generic", audio: "audio-x-generic", embedded: "application-x-generic"}[k] || "image-x-generic" }
    function sizeOf(m) { return m && m.width > 0 && m.height > 0 ? m.width + " × " + m.height + " px" : "" }
    function shortAddress(u) { return u.startsWith("data:") ? u.slice(0, 60) + "…" : u }
    function escapeAttribute(t) { return String(t).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }

    // One line of what is known, to read, select and copy. Nothing known, no line.
    // (Not a Kirigami.FormLayout: that one is only as wide as its fields would
    // like to be, and these should have all the window gives.)
    component FactLine: RowLayout {
        id: line
        property string label
        property string value
        property bool copyable: false
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        visible: value !== ""
        QQC2.Label {
            text: line.label
            horizontalAlignment: Text.AlignRight
            Layout.preferredWidth: Kirigami.Units.gridUnit * 8
        }
        QQC2.TextField {
            Layout.fillWidth: true
            text: line.value
            readOnly: true
            selectByMouse: true
            background: null
            leftPadding: 0
            // (Its beginning, not its end, when it's longer than the window is wide.)
            onTextChanged: cursorPosition = 0
        }
        QQC2.ToolButton {
            visible: line.copyable
            icon.name: "edit-copy"
            onClicked: Browser.copyText(line.value)
            QQC2.ToolTip.text: "Copy"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
    component PermissionRow: RowLayout {
        id: row
        required property var modelData
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Icon { source: row.modelData.icon; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
        QQC2.Label { text: row.modelData.name; textFormat: Text.PlainText; Layout.fillWidth: true; elide: Text.ElideRight }
        QQC2.Label { text: "In use"; visible: !!w.tab && w.tab.permissionInUse(row.modelData); font: Kirigami.Theme.smallFont; opacity: 0.75 }
        QQC2.ComboBox {
            model: ["Ask", "Allow", "Block"]
            currentIndex: w.tab ? w.tab.permissionState(row.modelData) : 0
            onActivated: i => { if (w.tab) w.tab.setPermissionState(row.modelData, i) }
        }
    }

    // ---- General ----
    Component {
        id: generalPage
        Kirigami.Page {
            id: gp
            title: "General"
            header: QQC2.ToolBar {
                RowLayout {
                    anchors.fill: parent
                    BarButton { compact: gp.width < Kirigami.Units.gridUnit * 20; text: "Copy Address"; icon.name: "edit-copy"; onClicked: Browser.copyText(w.url) }
                    BarButton { compact: gp.width < Kirigami.Units.gridUnit * 20; text: "Page Source"; icon.name: "text-html"; enabled: !!w.tab && !w.tab.showsSource; onClicked: w.tab.window.openTab("view-source:" + w.url, {background: false, afterCurrent: true}) }
                    Item { Layout.fillWidth: true }
                }
            }
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                FactLine { label: "Title:"; value: w.info.title || "" }
                FactLine { label: "Address:"; value: w.url; copyable: true }
                FactLine { label: "Type:"; value: w.info.type || "" }
                FactLine { label: "Text encoding:"; value: w.info.encoding || "" }
                FactLine { label: "Render mode:"; value: w.info.mode === "standards" ? "Standards compliance mode" : w.info.mode === "quirks" ? "Quirks mode" : "" }
                FactLine { label: "Language:"; value: w.info.language || "" }
                FactLine { label: "Fetched over:"; value: w.protocolName(w.info.protocol) }
                FactLine { label: "Size:"; value: w.sizeText() }
                FactLine { label: "Referring page:"; value: w.info.referrer || ""; copyable: true }
                FactLine { label: "Modified:"; value: w.modifiedText() }
                Kirigami.Heading {
                    level: 5
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                    readonly property int count: (w.info.meta || []).length
                    text: count === 1 ? "1 meta tag" : count + " meta tags"
                    visible: count > 0
                }
                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: (w.info.meta || []).length > 0
                    Component.onCompleted: if (background) background.visible = true
                    ListView {
                        clip: true
                        model: w.info.meta || []
                        delegate: QQC2.ItemDelegate {
                            id: metaRow
                            required property var modelData
                            width: ListView.view.width
                            contentItem: RowLayout {
                                spacing: Kirigami.Units.largeSpacing
                                QQC2.Label { text: metaRow.modelData.name; textFormat: Text.PlainText; elide: Text.ElideRight; font.bold: true; Layout.preferredWidth: Kirigami.Units.gridUnit * 10; Layout.maximumWidth: metaRow.width / 3 }
                                QQC2.Label { text: metaRow.modelData.content; textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true }
                            }
                            onClicked: metaMenu.popup()
                            QQC2.Menu {
                                id: metaMenu
                                QQC2.MenuItem { text: "Copy Content"; icon.name: "edit-copy"; onTriggered: Browser.copyText(metaRow.modelData.content) }
                            }
                        }
                    }
                }
                Item { Layout.fillHeight: true; visible: (w.info.meta || []).length === 0 }
            }
        }
    }

    // ---- Media ----
    Component {
        id: mediaPage
        Kirigami.Page {
            id: mp
            title: "Media"
            readonly property bool compact: width < Kirigami.Units.gridUnit * 30
            header: QQC2.ToolBar {
                RowLayout {
                    anchors.fill: parent
                    BarButton { compact: mp.compact; text: "Save…"; icon.name: "document-save"; enabled: !!mediaBody.chosen && MediaFile.savable(mediaBody.chosen.url) && !!w.tab; onClicked: w.saveOne(mediaBody.chosen) }
                    BarButton { compact: mp.compact; text: "Save All…"; icon.name: "document-save-all"; enabled: w.savableMedia.length > 0 && !!w.tab; onClicked: w.saveAll() }
                    Item { Layout.fillWidth: true }
                    // What is being saved, or how the last of it went.
                    QQC2.Label {
                        opacity: 0.75
                        elide: Text.ElideRight
                        Layout.maximumWidth: mp.width / 2
                        visible: !!w.saver && (w.saver.busy || w.saver.saved + w.saver.failed > 0)
                        text: !w.saver ? "" : w.saver.busy ? "Saving " + (w.saver.saved + w.saver.failed + 1) + " of " + w.saver.total + "…"
                            : w.saver.saved + " saved" + (w.saver.failed > 0 ? ", " + w.saver.failed + " failed" : "")
                    }
                    BarButton { compact: true; text: "Stop Saving"; icon.name: "process-stop"; visible: !!w.saver && w.saver.busy && w.saver.queue.length > 0; onClicked: w.saver.stop() }
                }
            }
            ColumnLayout {
                id: mediaBody
                anchors.fill: parent
                spacing: Kirigami.Units.smallSpacing
                readonly property var chosen: w.mediaIndex >= 0 && w.mediaIndex < w.media.length ? w.media[w.mediaIndex] : null
                // (Pictures only: sound and video would start to load in full.)
                readonly property bool shows: !!chosen && ["image", "icon", "background"].includes(chosen.kind)
                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: w.media.length === 0
                    icon.name: "image-x-generic"
                    text: "No pictures, sound or video on this page"
                }
                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: !mediaBody.chosen
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 8
                    visible: w.media.length > 0
                    Component.onCompleted: if (background) background.visible = true
                    ListView {
                        id: mediaList
                        clip: true
                        model: w.media
                        currentIndex: w.mediaIndex
                        delegate: QQC2.ItemDelegate {
                            id: mediaRow
                            required property var modelData
                            required property int index
                            width: ListView.view.width
                            highlighted: ListView.isCurrentItem
                            contentItem: RowLayout {
                                spacing: Kirigami.Units.smallSpacing
                                Kirigami.Icon { source: w.kindIcon(mediaRow.modelData.kind); Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                                QQC2.Label { text: w.shortAddress(mediaRow.modelData.url); textFormat: Text.PlainText; elide: Text.ElideMiddle; Layout.fillWidth: true }
                                QQC2.Label { text: w.kindName(mediaRow.modelData.kind); font: Kirigami.Theme.smallFont; opacity: 0.75 }
                            }
                            onClicked: w.mediaIndex = index
                            onDoubleClicked: if (MediaFile.savable(mediaRow.modelData.url)) w.saveOne(mediaRow.modelData)
                        }
                    }
                }
                FactLine { label: "Address:"; value: mediaBody.chosen ? w.shortAddress(mediaBody.chosen.url) : "" }
                FactLine { label: "Kind:"; value: mediaBody.chosen ? w.kindName(mediaBody.chosen.kind) : "" }
                FactLine { label: "Size:"; value: w.sizeOf(mediaBody.chosen) }
                FactLine { label: "Text:"; value: mediaBody.chosen ? mediaBody.chosen.text || "" : "" }
                RowLayout {
                    visible: !!mediaBody.chosen
                    QQC2.Button { text: "Copy Address"; icon.name: "edit-copy"; onClicked: Browser.copyText(mediaBody.chosen.url) }
                    QQC2.Button {
                        text: "Open in New Tab"
                        icon.name: "tab-new"
                        // (Not what only this page can open.)
                        enabled: !!mediaBody.chosen && /^https?:/i.test(mediaBody.chosen.url) && !!w.tab
                        onClicked: w.tab.window.openTab(mediaBody.chosen.url, {background: true, index: w.tab.window.linkTabIndex(w.tab), openerTabId: w.tab.tabId, container: w.tab.container})
                    }
                    Item { Layout.fillWidth: true }
                    QQC2.Label { text: (w.mediaIndex + 1) + " of " + w.media.length; opacity: 0.75 }
                }
                // The picture, as the page's own profile fetches it (from its
                // cache, with its content blocker), in a page of Kite's making
                // where nothing of the site's runs.
                Loader {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: !!mediaBody.chosen
                    active: w.section === "media" && mediaBody.shows && !!w.tab
                    sourceComponent: WebEngineView {
                        id: preview
                        profile: w.tab.view.profile
                        settings.javascriptEnabled: false
                        settings.localContentCanAccessFileUrls: false
                        backgroundColor: Kirigami.Theme.backgroundColor
                        readonly property string shown: mediaBody.shows ? mediaBody.chosen.url : ""
                        function show() {
                            if (shown === "") return
                            loadHtml('<!doctype html><meta charset="utf-8"><meta name="color-scheme" content="light dark">'
                                + '<body style="margin:0;height:100vh;display:flex;align-items:center;justify-content:center">'
                                + '<img style="max-width:100%;max-height:100vh;object-fit:contain" src="' + w.escapeAttribute(shown) + '">', w.url)
                        }
                        onShownChanged: show()
                        Component.onCompleted: show()
                        // Only the page above: nothing dropped on it or linked from it.
                        onNavigationRequested: request => { if (!request.url.toString().startsWith("data:text/html")) request.reject() }
                        onNewWindowRequested: request => {}
                        onContextMenuRequested: request => { request.accepted = true }
                    }
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: !!mediaBody.chosen && !mediaBody.shows
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    opacity: 0.75
                    text: "Not shown here. Open it in a tab to play it."
                }
            }
        }
    }

    // ---- Permissions ----
    Component {
        id: permissionsPage
        Kirigami.ScrollablePage {
            id: pp
            title: "Permissions"
            header: QQC2.ToolBar {
                RowLayout {
                    anchors.fill: parent
                    BarButton {
                        compact: pp.width < Kirigami.Units.gridUnit * 18
                        text: "Ask for All Again"
                        icon.name: "edit-undo"
                        enabled: !!w.tab && w.permissions.some(p => w.tab.permissionState(p) !== 0)
                        onClicked: { for (const p of w.permissions) w.tab.setPermissionState(p, 0); w.loadPermissions() }
                    }
                    Item { Layout.fillWidth: true }
                }
            }
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.gridUnit * 4
                    visible: w.permissions.length === 0
                    icon.name: "dialog-object-properties"
                    text: "Permissions are for websites"
                    explanation: "This page isn't one, and has nothing to ask for."
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    visible: w.permissions.length > 0
                    text: "What " + w.host + " may do" + (w.inPrivate ? ", in private windows" : "") + ". Left at Ask, it has to ask you first."
                }
                Repeater {
                    model: w.permissions
                    PermissionRow {}
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    visible: w.permissions.length > 0
                    text: "Camera, microphone and screen sharing are asked for every time a page wants them."
                }
            }
        }
    }

    // ---- Security ----
    Component {
        id: securityPage
        Kirigami.Page {
            id: sp
            title: "Security"
            readonly property bool encrypted: /^https:/i.test(w.url)
            header: QQC2.ToolBar {
                RowLayout {
                    anchors.fill: parent
                    BarButton {
                        compact: sp.width < Kirigami.Units.gridUnit * 18
                        text: "View Certificate"
                        icon.name: "certificate-server"
                        enabled: sp.encrypted && !!w.tab
                        onClicked: w.tab.window.openTab(w.app.certificatePage(w.url), {background: false, afterCurrent: true})
                    }
                    Item { Layout.fillWidth: true }
                }
            }
            ColumnLayout {
                anchors.fill: parent
                spacing: Kirigami.Units.largeSpacing
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        source: sp.encrypted ? "security-high" : w.isSite ? "security-low" : "globe"
                        Layout.preferredWidth: Kirigami.Units.iconSizes.large
                        Layout.preferredHeight: Layout.preferredWidth
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Kirigami.Heading { level: 4; text: w.host || (/^kite:/i.test(w.url) ? "A page of Kite's own" : /^file:/i.test(w.url) ? "A file on this computer" : "Not a website's page"); textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: sp.encrypted ? "The connection to this site is encrypted: what you send and receive can't be read or changed on the way."
                                : w.isSite ? "The connection to this site is not encrypted. What you send and receive could be read or changed by others on the way."
                                : "This page doesn't come from a website."
                        }
                    }
                }
                Kirigami.Separator { Layout.fillWidth: true; visible: w.isSite }
                Kirigami.Heading { level: 5; text: "Privacy and history"; visible: w.isSite }
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    visible: w.isSite && w.inPrivate
                    text: "This is a private window: the page isn't in your history, and its cookies go when the last private window closes."
                }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    visible: w.isSite && !w.inPrivate
                    QQC2.Label {
                        Kirigami.FormData.label: "Visited before:"
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: !(w.facts.visits > 0) ? "No, not in the history"
                            : "Yes, " + (w.facts.visits === 1 ? "once" : w.facts.visits + " times") + " (" + (w.facts.pages === 1 ? "1 page" : w.facts.pages + " pages") + " of this site)"
                    }
                    RowLayout {
                        Kirigami.FormData.label: "Cookies:"
                        QQC2.Label {
                            Layout.fillWidth: true
                            textFormat: Text.PlainText
                            text: !(w.facts.cookies > 0) ? "None stored" : (w.facts.cookies === 1 ? "1 stored" : w.facts.cookies + " stored") + " for " + w.facts.site
                        }
                        QQC2.Button { text: "Manage…"; onClicked: w.app.showPreferences("privacy").showCookies() }
                    }
                    RowLayout {
                        Kirigami.FormData.label: "Saved logins:"
                        QQC2.Label { Layout.fillWidth: true; text: Passwords.locked ? "Not known while the passwords are locked" : w.logins === 0 ? "None" : String(w.logins) }
                        QQC2.Button { text: "Manage…"; onClicked: w.app.showPreferences("passwords") }
                    }
                }
                Item { Layout.fillHeight: true }
            }
        }
    }
}
