import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's LocationBar: URL entry with history/bookmark completion and search
// suggestions, security indicator, bookmark star, content-blocker shield and
// extension page actions.
QQC2.TextField {
    id: field
    property var win
    property var tab
    property bool editing: false
    property var completions: []
    property var suggestions: []
    property int selected: -1
    property var bookmark: null
    property var pageActions: win.toolbarActions.filter(a => a.kind === "pageAction")
    readonly property string extName: tab ? Extensions.extensionNameForUrl(tab.url) : ""

    function displayUrl(u) {
        if (!u || u === "about:blank" || u === win.app.newTabUrl() || u === "kite:start") return ""
        return u
    }

    text: editing ? text : (tab ? displayUrl(tab.url) : "")
    placeholderText: "Search with " + win.app.setting("general.searchEngine") + " or enter address"
    leftPadding: securityButton.width + Kirigami.Units.smallSpacing * 2
    rightPadding: trailing.implicitWidth + Kirigami.Units.smallSpacing * 2
    selectByMouse: true
    inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText

    function focusAndSelect() { forceActiveFocus(); selectAll(); editing = true }

    function refreshBookmark() { bookmark = tab && tab.url ? JSON.parse(Browser.bookmarkForUrl(tab.url)) : null }
    onTabChanged: refreshBookmark()
    Connections { target: field.tab; ignoreUnknownSignals: true; function onUrlChanged() { field.refreshBookmark(); field.shieldAllowed = Browser.adblockAllowed(Browser.hostOf(field.tab.url)) } }
    Connections { target: Browser; function onBookmarksChanged() { field.refreshBookmark() } }

    function toggleBookmark() {
        if (!tab || !tab.url) return
        if (!bookmark) { Browser.addBookmark(tab.url, tab.title, JSON.parse(Browser.bookmarkRoots()).unsorted); refreshBookmark() }
        bookmarkPopup.open()
    }

    onActiveFocusChanged: {
        if (!activeFocus) { editing = false; popup.close() }
    }
    onTextEdited: {
        editing = true
        selected = -1
        const t = text.trim()
        if (t === "") { popup.close(); return }
        completions = JSON.parse(Browser.complete(t))
        suggestions = []
        Browser.requestSuggestions(t)
        popup.open()
    }
    Connections {
        target: Browser
        function onSuggestionsReady(q, json) { if (q === field.text.trim()) field.suggestions = JSON.parse(json) }
    }

    readonly property var rows: {
        const t = text.trim()
        const out = []
        if (t !== "") out.push({type: "go", title: t, url: ""})
        for (const c of completions) out.push(c)
        for (const s of suggestions) if (s !== t) out.push({type: "suggestion", title: s, url: ""})
        return out
    }

    function go(input, where) {
        const t = input.trim()
        if (t === "") return
        // Extension omnibox keywords
        const kw = JSON.parse(Extensions.omniboxKeywords())
        const first = t.split(" ")[0]
        const k = kw.find(x => x.keyword === first)
        if (k && t.length > first.length) {
            Extensions.browserEvent("omniboxInputEntered", JSON.stringify({ext: k.ext, args: [t.substring(first.length + 1), "currentTab"]}))
        } else {
            const url = t.startsWith("? ") ? Browser.searchUrl(win.app.setting("general.searchEngine"), t.substring(2)) : Browser.resolveInput(t)
            if (!url) return
            if (where === "tab") win.openTab(url, {background: false})
            else if (tab) tab.load(url)
        }
        editing = false
        popup.close()
        if (tab) tab.view.forceActiveFocus()
    }

    onAccepted: {
        const r = selected >= 0 ? rows[selected] : null
        if (r && r.url) go(r.url, "")
        else if (r && r.type === "suggestion") go("? " + r.title, "")
        else go(text, "")
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Down && popup.opened) { selected = Math.min(rows.length - 1, selected + 1); event.accepted = true }
        else if (event.key === Qt.Key_Up && popup.opened) { selected = Math.max(-1, selected - 1); event.accepted = true }
        else if (event.key === Qt.Key_Escape) { editing = false; popup.close(); if (tab) tab.view.forceActiveFocus(); event.accepted = true }
        else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.AltModifier)) { go(text, "tab"); event.accepted = true }
        else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (event.modifiers & Qt.ControlModifier)) {
            // Ctrl+Enter: add www. and .com like Falkon
            const t = text.trim(); if (!t.includes(".")) go("www." + t + ".com", ""); else go(t, ""); event.accepted = true
        }
    }

    // Security / site info indicator
    QQC2.ToolButton {
        id: securityButton
        anchors { left: parent.left; verticalCenter: parent.verticalCenter; leftMargin: 2 }
        height: parent.height - 4; width: height
        padding: 0
        flat: true
        icon.name: !field.tab ? "globe" : field.extName !== "" ? "preferences-plugin" : field.tab.isSecure ? "security-high" : field.tab.url.startsWith("http:") ? "security-low" : "globe"
        onClicked: siteInfo.open()
        QQC2.ToolTip.text: field.extName !== "" ? "Extension (" + field.extName + ")" : field.tab && field.tab.isSecure ? "Connection is secure" : "Connection is not secure"
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }

    property bool shieldAllowed: false
    RowLayout {
        id: trailing
        anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: 2 }
        spacing: 0
        height: parent.height - 4
        Repeater {
            model: field.pageActions
            delegate: QQC2.ToolButton {
                required property var modelData
                Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
                padding: 2
                icon.source: modelData.icon || ""
                icon.name: modelData.icon ? "" : "preferences-plugin"
                onClicked: modelData.popup ? field.win.openPageActionPopup(modelData, this) : Extensions.actionClicked(modelData.ext, "pageAction", field.tab.tabId)
                QQC2.ToolTip.text: modelData.title; QQC2.ToolTip.visible: hovered
            }
        }
        QQC2.ToolButton {
            id: keyButton
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            visible: field.tab && field.tab.loginForm && (field.tab.logins.length > 0 || Passwords.locked)
            icon.name: Passwords.locked ? "object-locked" : "dialog-password"
            onClicked: keyMenu.popup(keyButton, 0, keyButton.height)
            QQC2.ToolTip.text: Passwords.locked ? "Saved logins are locked" : "Saved logins for this site"; QQC2.ToolTip.visible: hovered
            QQC2.Menu {
                id: keyMenu
                Instantiator {
                    model: field.tab ? field.tab.logins : []
                    delegate: QQC2.MenuItem {
                        required property var modelData
                        text: modelData.username || "(no username)"
                        icon.name: "user-identity"
                        onTriggered: field.tab.fillLogin(modelData.id)
                    }
                    onObjectAdded: (i, o) => keyMenu.insertItem(i, o)
                    onObjectRemoved: (i, o) => keyMenu.removeItem(o)
                }
                QQC2.MenuItem {
                    visible: Passwords.locked
                    height: visible ? implicitHeight : 0
                    text: "Unlock Saved Logins…"; icon.name: "object-unlocked"
                    onTriggered: field.win.app.unlockPasswords(ok => { if (ok) field.tab.refreshLogins() })
                }
                QQC2.MenuSeparator {}
                QQC2.MenuItem { text: "Manage Passwords…"; icon.name: "configure"; onTriggered: field.win.app.showPreferences("passwords") }
            }
        }
        QQC2.ToolButton {
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            // Content-blocking extensions (uBlock Origin, ...) have their own button.
            visible: field.tab && field.tab.url.startsWith("http") && Browser.contentBlocker() === ""
            icon.name: field.shieldAllowed ? "security-low" : "security-high"
            icon.color: field.shieldAllowed ? Kirigami.Theme.disabledTextColor : Kirigami.Theme.positiveTextColor
            onClicked: shieldMenu.popup()
            QQC2.ToolTip.text: "Content blocker"; QQC2.ToolTip.visible: hovered
            QQC2.Menu {
                id: shieldMenu
                QQC2.MenuItem { enabled: false; text: Browser.adblockBlocked() + " requests blocked this session" }
                QQC2.MenuItem {
                    text: field.shieldAllowed ? "Enable blocking on this site" : "Disable blocking on this site"
                    onTriggered: { Browser.adblockAllow(Browser.hostOf(field.tab.url), !field.shieldAllowed); field.shieldAllowed = !field.shieldAllowed; field.tab.view.reload() }
                }
                QQC2.MenuItem { text: "Content Blocker Settings…"; onTriggered: field.win.app.showPreferences("adblock") }
            }
        }
        QQC2.Label {
            visible: field.tab && Math.abs(field.tab.view.zoomFactor - 1.0) > 0.01
            text: field.tab ? Math.round(field.tab.view.zoomFactor * 100) + "%" : ""
            font: Kirigami.Theme.smallFont
            TapHandler { onTapped: field.tab.setZoom(1.0) }
        }
        QQC2.ToolButton {
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            visible: field.tab && field.tab.url !== "" && !field.editing
            icon.name: field.bookmark ? "bookmarks-bookmarked" : "bookmark-new"
            icon.color: field.bookmark ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
            onClicked: field.toggleBookmark()
            QQC2.ToolTip.text: field.bookmark ? "Edit this bookmark (Ctrl+D)" : "Bookmark this page (Ctrl+D)"; QQC2.ToolTip.visible: hovered
        }
    }

    // Loading progress along the bottom edge
    Rectangle {
        anchors { left: parent.left; bottom: parent.bottom; margins: 1 }
        height: 2
        width: field.tab && field.tab.loading ? (parent.width - 2) * field.tab.view.loadProgress / 100 : 0
        color: Kirigami.Theme.highlightColor
        visible: width > 0
    }

    QQC2.Popup {
        id: popup
        y: field.height
        width: field.width
        padding: 1
        focus: false
        closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
        contentItem: ListView {
            implicitHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 22)
            model: field.rows
            clip: true
            currentIndex: field.selected
            delegate: QQC2.ItemDelegate {
                required property var modelData
                required property int index
                width: ListView.view.width
                highlighted: index === field.selected
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth
                        source: modelData.type === "bookmark" ? "bookmarks" : modelData.type === "history" ? "view-history" : "search"
                    }
                    QQC2.Label {
                        text: modelData.type === "go" ? (Browser.resolveInput(modelData.title).startsWith("http") && !modelData.title.includes(" ") && modelData.title.includes(".") ? "Go to " + modelData.title : "Search for “" + modelData.title + "”")
                            : modelData.title || modelData.url
                        elide: Text.ElideRight
                        // Takes the rest of the row unless a URL follows it (which
                        // then does). With nothing able to grow, the layout would
                        // spread the spare width over the cells and shift the
                        // text differently in each row.
                        Layout.fillWidth: modelData.url === ""
                    }
                    QQC2.Label {
                        visible: modelData.url !== ""
                        text: "— " + modelData.url
                        color: Kirigami.Theme.linkColor
                        elide: Text.ElideMiddle
                        Layout.fillWidth: true
                    }
                }
                onClicked: {
                    if (modelData.url) field.go(modelData.url, "")
                    else if (modelData.type === "suggestion") field.go("? " + modelData.title, "")
                    else field.go(modelData.title, "")
                }
            }
        }
    }

    QQC2.Popup {
        id: bookmarkPopup
        x: field.width - width
        y: field.height
        width: Kirigami.Units.gridUnit * 20
        onOpened: { nameField.text = field.bookmark ? field.bookmark.title : ""; folders.model = JSON.parse(Browser.bookmarkFolders()); folders.currentIndex = Math.max(0, folders.model.findIndex(f => f.id === (field.bookmark ? field.bookmark.parent : ""))); nameField.forceActiveFocus() }
        contentItem: ColumnLayout {
            Kirigami.Heading { level: 4; text: "Edit Bookmark" }
            Kirigami.FormLayout {
                Layout.fillWidth: true
                QQC2.TextField { id: nameField; Kirigami.FormData.label: "Name:"; Layout.fillWidth: true }
                QQC2.ComboBox {
                    id: folders
                    Kirigami.FormData.label: "Folder:"
                    Layout.fillWidth: true
                    textRole: "title"
                    delegate: QQC2.ItemDelegate { required property var modelData; width: ListView.view.width; text: "    ".repeat(modelData.depth) + modelData.title }
                }
            }
            RowLayout {
                QQC2.Button { text: "Remove"; icon.name: "edit-delete"; onClicked: { Browser.removeBookmark(field.bookmark.id); bookmarkPopup.close() } }
                Item { Layout.fillWidth: true }
                QQC2.Button {
                    text: "Done"; icon.name: "dialog-ok"
                    onClicked: {
                        Browser.updateBookmark(field.bookmark.id, JSON.stringify({title: nameField.text}))
                        const f = folders.model[folders.currentIndex]
                        if (f && f.id !== field.bookmark.parent) Browser.moveBookmark(field.bookmark.id, f.id, -1)
                        bookmarkPopup.close()
                    }
                }
            }
        }
    }

    QQC2.Popup {
        id: siteInfo
        y: field.height
        width: Kirigami.Units.gridUnit * 20
        property var site: ({})
        onOpened: site = JSON.parse(Browser.siteSettings(Browser.hostOf(field.tab.url)))
        contentItem: ColumnLayout {
            Kirigami.Heading { level: 4; text: field.extName !== "" ? field.extName : Browser.hostOf(field.tab ? field.tab.url : ""); elide: Text.ElideRight; Layout.fillWidth: true }
            QQC2.Label {
                Layout.fillWidth: true; wrapMode: Text.Wrap
                text: field.extName !== "" ? "This page is provided by an extension." : field.tab && field.tab.isSecure ? "Your connection to this site is encrypted." : "Your connection to this site is not encrypted. Information you submit could be viewed by others."
            }
            Kirigami.Separator { Layout.fillWidth: true }
            QQC2.CheckBox {
                visible: Browser.contentBlocker() === ""
                text: "Block ads and trackers on this site"
                checked: !field.shieldAllowed
                onToggled: { Browser.adblockAllow(Browser.hostOf(field.tab.url), !checked); field.shieldAllowed = !checked; field.tab.view.reload() }
            }
            QQC2.Button { text: "Clear zoom setting"; visible: siteInfo.site.zoom > 0; onClicked: { Browser.setSiteSetting(Browser.hostOf(field.tab.url), "zoom", -1); field.tab.view.zoomFactor = 1.0; siteInfo.close() } }
            QQC2.Button { text: "Clear site data and permissions…"; onClicked: { Browser.clearSiteSettings(Browser.hostOf(field.tab.url)); siteInfo.close(); field.win.toast("Site settings cleared", "edit-clear") } }
        }
    }
}
