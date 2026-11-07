import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtWebEngine
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
        if (!u || u === "about:blank" || u === win.app.newTabUrl() || u === "kite:start" || win.app.isStartPage(u)) return ""
        return u
    }

    text: editing ? text : (tab ? displayUrl(tab.url) : "")
    placeholderText: "Search with " + win.app.setting("general.searchEngine") + " or enter address"
    leftPadding: securityButton.width + (win.privateMode ? privateIcon.width + Kirigami.Units.smallSpacing : 0) + Kirigami.Units.smallSpacing * 2
    rightPadding: trailing.implicitWidth + Kirigami.Units.smallSpacing * 2
    selectByMouse: true
    inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText

    function focusAndSelect() { forceActiveFocus(); selectAll(); editing = true }

    function refreshBookmark() { bookmark = tab && tab.url ? JSON.parse(Browser.bookmarkForUrl(tab.url)) : null }
    onTabChanged: refreshBookmark()
    Connections { target: field.tab; ignoreUnknownSignals: true; function onUrlChanged() { field.refreshBookmark(); field.shieldAllowed = Browser.adblockAllowed(Browser.hostOf(field.tab.url)) } }
    Connections { target: Browser; function onBookmarksChanged() { field.refreshBookmark() } }

    // The page's site, and whether it has settings of its own (Site
    // Settings: zoom, dark mode, CSS, JavaScript).
    readonly property string siteHost: tab && /^https?:/i.test(tab.url) ? Browser.hostOf(tab.url) : ""
    property bool siteHasSettings: false
    function refreshSite() {
        const s = siteHost ? JSON.parse(Browser.siteSettings(siteHost)) : null
        siteHasSettings = !!s && (s.zoom > 0 || s.dark_mode > 0 || s.css !== "" || s.js !== "")
    }
    onSiteHostChanged: refreshSite()
    Connections { target: field.win ? field.win.app : null; ignoreUnknownSignals: true; function onSiteSettingsEdited(host) { if (host === field.siteHost) field.refreshSite() } }

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
        completions = JSON.parse(Browser.complete(t, win.privateMode))
        suggestions = []
        // (What is typed in a private window stays here.)
        // (Nor an address for another program, like smb://server/share.)
        if (!win.privateMode && !Browser.isExternalUrl(t)) Browser.requestSuggestions(t)
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
        const kw = JSON.parse(Extensions.omniboxKeywords(win.privateMode))
        const first = t.split(" ")[0]
        const k = kw.find(x => x.keyword === first)
        if (k && t.length > first.length) {
            Extensions.browserEvent("omniboxInputEntered", JSON.stringify({ext: k.ext, args: [t.substring(first.length + 1), "currentTab"]}))
        } else {
            const url = t.startsWith("? ") ? Browser.searchUrl(win.app.setting("general.searchEngine"), t.substring(2)) : Browser.resolveInput(t)
            if (!url) return
            if (Browser.isExternalUrl(url)) win.openExternal(url)
            else if (where === "tab") win.openTab(url, {background: false})
            else if (tab) tab.load(url)
        }
        editing = false
        popup.close()
        if (tab) tab.focusPage()
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
        else if (event.key === Qt.Key_Escape) { editing = false; popup.close(); if (tab) tab.focusPage(); event.accepted = true }
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
        icon.name: !field.tab ? "globe" : field.extName !== "" ? "kite-extension" : field.tab.isSecure ? "security-high" : field.tab.url.startsWith("http:") ? "security-low" : "globe"
        onClicked: siteInfo.open()
        QQC2.ToolTip.text: field.extName !== "" ? "Extension (" + field.extName + ")" : field.tab && field.tab.isSecure ? "Connection is secure" : "Connection is not secure"
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }

    // Always there in a private window, whatever the page or tab bar.
    Kirigami.Icon {
        id: privateIcon
        visible: field.win.privateMode
        anchors { left: securityButton.right; leftMargin: Kirigami.Units.smallSpacing; verticalCenter: parent.verticalCenter }
        width: Kirigami.Units.iconSizes.smallMedium
        height: width
        source: "view-private"
        Accessible.name: "Private window"
        HoverHandler { id: privateHover }
        QQC2.ToolTip.text: "Private window: pages stay out of your history"
        QQC2.ToolTip.visible: privateHover.hovered
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
                icon.name: modelData.icon ? "" : "kite-extension"
                onClicked: modelData.popup ? field.win.openPageActionPopup(modelData, this) : Extensions.actionClicked(modelData.ext, "pageAction", field.tab.tabId)
                QQC2.ToolTip.text: modelData.title; QQC2.ToolTip.visible: hovered
            }
        }
        // Reader view, for pages that look like articles (and to leave it).
        QQC2.ToolButton {
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            visible: field.tab && (field.tab.readerAvailable || field.tab.isReader) && !field.editing
            icon.name: "view-readermode"
            icon.color: field.tab && field.tab.isReader ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
            onClicked: field.tab.toggleReader()
            QQC2.ToolTip.text: (field.tab && field.tab.isReader ? "Close reader view" : "Reader view")
                + " (" + field.win.app.shortcutKeys("readerView").map(k => Browser.keySequenceDisplay(k)).join(", ") + ")"
            QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
        // Picture-in-picture, for pages with a video (Main.qml startPip).
        QQC2.ToolButton {
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            visible: field.win.app.pipAvailable && field.tab && (field.tab.hasVideo || field.tab.inPip) && !field.editing
            icon.name: field.tab && field.tab.inPip ? "kite-pip-exit" : "kite-pip"
            icon.color: field.tab && field.tab.inPip ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
            onClicked: field.win.app.startPip(field.tab)
            QQC2.ToolTip.text: (field.tab && field.tab.inPip ? "Back from picture-in-picture" : "Picture-in-picture")
                + " (" + field.win.app.shortcutKeys("pip.toggle").map(k => Browser.keySequenceDisplay(k)).join(", ") + ")"
            QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
        QQC2.ToolButton {
            id: keyButton
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            visible: field.tab && (field.tab.loginForm || field.tab.frameLoginForm) && (field.tab.logins.length > 0 || Passwords.locked)
            icon.name: Passwords.locked ? "object-locked" : "dialog-password"
            onClicked: keyMenu.popup(keyButton, 0, keyButton.height)
            QQC2.ToolTip.text: Passwords.locked ? "Saved logins are locked" : "Saved logins for this site"; QQC2.ToolTip.visible: hovered
            QQC2.Menu {
                id: keyMenu
                Instantiator {
                    model: field.tab ? field.tab.logins : []
                    delegate: QQC2.MenuItem {
                        required property var modelData
                        // (From a frame of another site: which one it is for.)
                        text: (modelData.username || "(no username)") + (modelData.frameHost ? " — " + modelData.frameHost : "")
                        icon.name: "user-identity"
                        onTriggered: field.tab.fillLogin(modelData.id, modelData.frameId)
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
            visible: field.tab && field.tab.url.startsWith("http") && Browser.contentBlocker(field.win.privateMode) === ""
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
        // Site Settings of this site (adds them if it has none yet).
        QQC2.ToolButton {
            Layout.preferredHeight: trailing.height; Layout.preferredWidth: trailing.height
            padding: 2
            visible: field.siteHost !== "" && !field.editing
            // Breeze's "configure" toggles, shipped with Kite (icons/) as a
            // plain shape painted in the colour below: highlighted like the
            // bookmark star while the site has settings.
            // (Drawn at the toolbar icon size, like the star beside it, not
            // stretched to the button.)
            contentItem: Item {
                Kirigami.Icon {
                    anchors.centerIn: parent
                    width: Kirigami.Units.iconSizes.smallMedium
                    height: width
                    source: Qt.resolvedUrl("../icons/site-settings.svg")
                    isMask: true
                    color: field.siteHasSettings ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                }
            }
            onClicked: field.win.app.showSiteSettings(field.siteHost)
            QQC2.ToolTip.text: (field.siteHasSettings ? "Edit settings for " : "Add settings for ") + field.siteHost; QQC2.ToolTip.visible: hovered
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
                    // The address first: it's what's being typed.
                    QQC2.Label {
                        text: modelData.url ? Browser.displayUrl(modelData.url) : modelData.type === "go" ? Browser.isExternalUrl(modelData.title) && modelData.title.includes("://") ? "Open " + modelData.title + " in another app" : (Browser.resolveInput(modelData.title).startsWith("http") && !modelData.title.includes(" ") && modelData.title.includes(".") ? "Go to " + modelData.title : "Search for “" + modelData.title + "”")
                            : modelData.title
                        color: modelData.url ? Kirigami.Theme.linkColor : Kirigami.Theme.textColor
                        elide: modelData.url ? Text.ElideMiddle : Text.ElideRight
                        // Takes the rest of the row unless a title follows it (which
                        // then does). With nothing able to grow, the layout would
                        // spread the spare width over the cells and shift the
                        // text differently in each row.
                        Layout.fillWidth: !titleLabel.visible
                        Layout.maximumWidth: titleLabel.visible ? field.width * 0.6 : -1
                    }
                    QQC2.Label {
                        id: titleLabel
                        visible: modelData.url !== "" && !!modelData.title
                        text: "— " + modelData.title
                        opacity: 0.7
                        elide: Text.ElideRight
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
                    // ListView.view is null for rows the ComboBox creates to measure them.
                    delegate: QQC2.ItemDelegate { required property var modelData; width: ListView.view ? ListView.view.width : implicitWidth; text: "    ".repeat(modelData.depth) + modelData.title }
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
        // Filled before the popup shows (onOpened runs after it has appeared).
        onAboutToShow: { site = JSON.parse(Browser.siteSettings(Browser.hostOf(field.tab.url))); loadPermissions() }

        // Site permissions: the kinds QtWebEngine stores per origin, and
        // Kite's own session decision for keeping the screen on (Wake Lock).
        // Camera, microphone and screen sharing are asked for every time.
        property var permissions: []
        function loadPermissions() {
            const t = field.tab
            const m = t && field.extName === "" ? t.url.match(/^https?:\/\/[^/?#]+/i) : null
            if (!m) { permissions = []; return }
            const P = WebEnginePermission.PermissionType
            const kinds = [
                {type: P.Geolocation, name: "Location", icon: "mark-location"},
                {type: P.Notifications, name: "Notifications", icon: "notifications"},
                {type: P.ClipboardReadWrite, name: "Clipboard", icon: "edit-paste"},
                {type: P.LocalFontsAccess, name: "Local fonts", icon: "preferences-desktop-font"},
            ]
            const list = kinds.map(k => ({name: k.name, icon: k.icon, perm: t.view.profile.queryPermission(m[0], k.type)}))
            list.push({name: "Keep screen on", icon: "video-display", wakeLock: true, key: (field.win.privateMode ? "private:" : "") + Browser.hostOf(t.url)})
            permissions = list
        }
        // kite:certificate for an https address.
        function certificatePage(u) {
            const m = u.match(/^https:\/\/(?:[^\/?#@]*@)?(\[[^\]]+\]|[^:\/?#]+)(?::(\d+))?/i)
            return "kite:certificate?host=" + encodeURIComponent(m ? m[1].replace(/^\[|\]$/g, "") : "") + (m && m[2] ? "&port=" + m[2] : "")
        }
        // 0 = ask, 1 = allow, 2 = block
        function stateOf(p) {
            if (p.wakeLock) { const d = field.win.app.wakeLockSites[p.key]; return d === undefined ? 0 : d ? 1 : 2 }
            const S = WebEnginePermission.State
            return p.perm.state === S.Granted ? 1 : p.perm.state === S.Denied ? 2 : 0
        }
        function setState(p, i) {
            if (p.wakeLock) { if (i === 0) field.win.app.forgetWakeLock(p.key); else field.win.app.rememberWakeLock(p.key, i === 1); return }
            if (i === 1) p.perm.grant(); else if (i === 2) p.perm.deny(); else p.perm.reset()
        }
        contentItem: ColumnLayout {
            Kirigami.Heading { level: 4; text: field.extName !== "" ? field.extName : Browser.hostOf(field.tab ? field.tab.url : ""); elide: Text.ElideRight; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                QQC2.Label {
                    Layout.fillWidth: true; wrapMode: Text.Wrap
                    text: field.extName !== "" ? "This page is provided by an extension." : field.tab && field.tab.isSecure ? "Your connection to this site is encrypted." : "Your connection to this site is not encrypted. Information you submit could be viewed by others."
                }
                // The site's certificates, in a tab of their own.
                QQC2.Button {
                    text: "View Certificate"
                    icon.name: "certificate-server"
                    visible: field.extName === "" && field.tab && /^https:/i.test(field.tab.url)
                    onClicked: { field.win.openTab(siteInfo.certificatePage(field.tab.url), {background: false, afterCurrent: true}); siteInfo.close() }
                }
            }
            Kirigami.Separator { Layout.fillWidth: true }
            QQC2.CheckBox {
                visible: Browser.contentBlocker(field.win.privateMode) === ""
                text: "Block ads and trackers on this site"
                checked: !field.shieldAllowed
                onToggled: { Browser.adblockAllow(Browser.hostOf(field.tab.url), !checked); field.shieldAllowed = !checked; field.tab.view.reload() }
            }
            Kirigami.Separator { Layout.fillWidth: true; visible: siteInfo.permissions.length > 0 }
            Kirigami.Heading { level: 5; text: "Permissions"; visible: siteInfo.permissions.length > 0 }
            Repeater {
                model: siteInfo.permissions
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon { source: modelData.icon; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                    QQC2.Label { text: modelData.name; Layout.fillWidth: true; elide: Text.ElideRight }
                    QQC2.ComboBox {
                        model: ["Ask", "Allow", "Block"]
                        currentIndex: siteInfo.stateOf(modelData)
                        onActivated: i => siteInfo.setState(modelData, i)
                    }
                }
            }
            Kirigami.Separator { Layout.fillWidth: true; visible: siteInfo.permissions.length > 0 }
            QQC2.Button { text: "Clear zoom setting"; visible: siteInfo.site.zoom > 0; onClicked: { const h = Browser.hostOf(field.tab.url); Browser.setSiteSetting(h, "zoom", -1); field.win.app.siteSettingsChanged(h); siteInfo.close() } }
            // Zoom, dark mode, CSS and JavaScript of the site (Settings).
            QQC2.Button {
                text: "Site Settings…"
                icon.name: "configure"
                visible: field.extName === "" && Browser.hostOf(field.tab ? field.tab.url : "") !== ""
                onClicked: { field.win.app.showSiteSettings(Browser.hostOf(field.tab.url)); siteInfo.close() }
            }
            // Keeps the site's custom CSS and JavaScript (written by the user).
            QQC2.Button { text: "Clear site data and permissions…"; onClicked: { const h = Browser.hostOf(field.tab.url); Browser.resetSiteSettings(h); field.win.app.siteSettingsChanged(h); for (const p of siteInfo.permissions) siteInfo.setState(p, 0); siteInfo.close(); field.win.toast("Site settings cleared", "edit-clear") } }
        }
    }
}
