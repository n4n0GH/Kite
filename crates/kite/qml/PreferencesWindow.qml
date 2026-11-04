import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's Preferences dialog.
Kirigami.ApplicationWindow {
    id: w
    property var app
    property string page: "general"
    title: "Settings — Kite"
    width: Kirigami.Units.gridUnit * 50
    height: Kirigami.Units.gridUnit * 36

    function get(k) { return JSON.parse(Browser.setting(k)) }
    function set(k, v) { Browser.setSetting(k, JSON.stringify(v)) }

    readonly property var pages: [
        {key: "general", title: "General", icon: "go-home"},
        {key: "appearance", title: "Appearance", icon: "preferences-desktop-theme"},
        {key: "tabs", title: "Tabs", icon: "tab-new"},
        {key: "browsing", title: "Browsing", icon: "internet-web-browser"},
        {key: "search", title: "Search Engines", icon: "search"},
        {key: "privacy", title: "Privacy", icon: "preferences-system-privacy"},
        {key: "passwords", title: "Passwords", icon: "dialog-password"},
        {key: "adblock", title: "Content Blocking", icon: "security-high"},
        {key: "downloads", title: "Downloads", icon: "download"},
        {key: "sites", title: "Site Settings", icon: "preferences-web-browser-cookies"},
        {key: "extensions", title: "Extensions", icon: "preferences-plugin"},
    ]

    globalDrawer: Kirigami.GlobalDrawer {
        modal: false
        width: Kirigami.Units.gridUnit * 12
        actions: w.pages.map(p => pageAction.createObject(w, {text: p.title, iconName: p.icon, key: p.key}))
    }
    Component { id: pageAction; Kirigami.Action { property string key; property string iconName; icon.name: iconName; checked: w.page === key; onTriggered: w.page = key } }

    pageStack.initialPage: Kirigami.ScrollablePage {
        id: sp
        title: (w.pages.find(p => p.key === w.page) || {}).title || ""
        Loader {
            width: sp.width - Kirigami.Units.gridUnit * 2
            sourceComponent: {
                switch (w.page) {
                case "appearance": return appearance
                case "tabs": return tabs
                case "browsing": return browsing
                case "search": return search
                case "privacy": return privacy
                case "passwords": return passwords
                case "adblock": return adblock
                case "downloads": return downloads
                case "sites": return sites
                case "extensions": return extensions
                default: return general
                }
            }
        }
    }

    Component {
        id: general
        Kirigami.FormLayout {
            QQC2.ComboBox {
                Kirigami.FormData.label: "When Kite starts:"
                model: [{k: "restore", t: "Restore previous session"}, {k: "homepage", t: "Open home page"}, {k: "speeddial", t: "Open start page"}, {k: "blank", t: "Open blank page"}]
                textRole: "t"
                currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("general.startup")))
                onActivated: i => w.set("general.startup", model[i].k)
            }
            QQC2.TextField { Kirigami.FormData.label: "Home page:"; Layout.fillWidth: true; text: w.get("general.homepage"); onEditingFinished: w.set("general.homepage", text) }
            QQC2.TextField { Kirigami.FormData.label: "New tab page:"; Layout.fillWidth: true; text: w.get("general.newTabPage"); onEditingFinished: w.set("general.newTabPage", text); placeholderText: "kite:start" }
            QQC2.Button { text: "Use Current Page as Home Page"; enabled: !!w.app.activeWindow && !!w.app.activeWindow.currentTab; onClicked: w.set("general.homepage", w.app.activeWindow.currentTab.url) }
            Item { Kirigami.FormData.isSection: true }
            QQC2.ComboBox {
                Kirigami.FormData.label: "Default search engine:"
                model: JSON.parse(Browser.engines()).map(e => e.name)
                currentIndex: Math.max(0, model.indexOf(w.get("general.searchEngine")))
                onActivated: i => w.set("general.searchEngine", model[i])
            }
            QQC2.CheckBox { Kirigami.FormData.label: "Location bar:"; text: "Search when input isn't an address"; checked: w.get("general.searchFromLocationBar"); onToggled: w.set("general.searchFromLocationBar", checked) }
            QQC2.CheckBox { text: "Show search suggestions"; checked: w.get("general.showSearchSuggestions"); onToggled: w.set("general.showSearchSuggestions", checked) }
            Item { Kirigami.FormData.isSection: true }
            QQC2.Label { Kirigami.FormData.label: "Profile:"; text: Browser.profileName() + "  (" + Browser.webEnginePath().replace(/\/webengine$/, "") + ")"; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }

    Component {
        id: appearance
        Kirigami.FormLayout {
            QQC2.CheckBox { Kirigami.FormData.label: "Toolbars:"; text: "Show bookmarks toolbar"; checked: w.get("appearance.showBookmarksToolbar"); onToggled: w.set("appearance.showBookmarksToolbar", checked) }
            QQC2.CheckBox {
                text: "Show home button"
                readonly property var items: w.app.liveSettings["toolbar.items"] || []
                checked: items.includes("home")
                onToggled: {
                    const list = items.filter(i => i !== "home")
                    if (checked) list.splice(Math.max(0, list.indexOf("urlbar")), 0, "home")
                    w.set("toolbar.items", list)
                }
            }
            QQC2.CheckBox { text: "Show menu bar (or tap Alt to show it briefly)"; checked: w.get("appearance.showMenuBar"); onToggled: w.set("appearance.showMenuBar", checked) }
            QQC2.Button { text: "Customize Toolbar…"; icon.name: "configure-toolbars"; enabled: !!w.app.activeWindow; onClicked: w.app.activeWindow.customizeToolbar() }
            QQC2.CheckBox { text: "Show link targets when hovering links"; checked: w.get("appearance.showStatusBar"); onToggled: w.set("appearance.showStatusBar", checked) }
            QQC2.SpinBox { Kirigami.FormData.label: "Default zoom:"; from: 30; to: 300; stepSize: 10; value: w.get("appearance.defaultZoom"); onValueModified: w.set("appearance.defaultZoom", value); textFromValue: v => v + "%" }
            QQC2.CheckBox { Kirigami.FormData.label: "Web content:"; text: "Force dark mode on websites"; checked: w.get("appearance.forceDarkMode"); onToggled: w.set("appearance.forceDarkMode", checked) }
            QQC2.CheckBox { text: "Overlay scrollbars (thin, widen on hover, hide when idle)"; checked: w.get("appearance.overlayScrollbars"); onToggled: w.set("appearance.overlayScrollbars", checked) }
            QQC2.Label { text: "Some changes apply to new tabs and windows; scrollbar changes after restarting Kite."; opacity: 0.7; font: Kirigami.Theme.smallFont }
        }
    }

    Component {
        id: tabs
        Kirigami.FormLayout {
            QQC2.ComboBox {
                Kirigami.FormData.label: "Tab bar:"
                model: [{k: "top", t: "Above the address bar"}, {k: "belowToolbar", t: "Below the address bar"}, {k: "bottom", t: "Below the page"}, {k: "vertical", t: "Vertical list on the side"}]
                textRole: "t"
                currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("tabs.position")))
                onActivated: i => w.set("tabs.position", model[i].k)
            }
            QQC2.CheckBox { text: "Hide the tab bar when only one tab is open"; checked: w.get("tabs.hideWhenSingle"); onToggled: w.set("tabs.hideWhenSingle", checked) }
            QQC2.Label { text: "Also available by right-clicking the tab bar."; opacity: 0.7; font: Kirigami.Theme.smallFont }
            QQC2.CheckBox { Kirigami.FormData.label: "Opening:"; text: "Open new tabs after the current tab"; checked: w.get("tabs.newTabAfterActive"); onToggled: w.set("tabs.newTabAfterActive", checked) }
            QQC2.CheckBox { text: "Open links in background tabs"; checked: w.get("tabs.openNewTabsInBackground"); onToggled: w.set("tabs.openNewTabsInBackground", checked) }
            QQC2.CheckBox { text: "Switch to tabs opened from links"; checked: w.get("tabs.switchToNewTabFromLink"); onToggled: w.set("tabs.switchToNewTabFromLink", checked) }
            QQC2.CheckBox { text: "Open popup windows in tabs"; checked: w.get("tabs.openPopupsInTabs"); onToggled: w.set("tabs.openPopupsInTabs", checked) }
            QQC2.CheckBox { Kirigami.FormData.label: "Closing:"; text: "Close window when the last tab is closed"; checked: w.get("tabs.closeWindowWithLastTab"); onToggled: w.set("tabs.closeWindowWithLastTab", checked) }
        }
    }

    Component {
        id: browsing
        Kirigami.FormLayout {
            QQC2.CheckBox { Kirigami.FormData.label: "Web content:"; text: "Enable JavaScript"; checked: w.get("web.javascript"); onToggled: w.set("web.javascript", checked) }
            QQC2.CheckBox { text: "Load images automatically"; checked: w.get("web.autoloadImages"); onToggled: w.set("web.autoloadImages", checked) }
            QQC2.CheckBox { text: "Allow local storage"; checked: w.get("web.localStorage"); onToggled: w.set("web.localStorage", checked) }
            QQC2.CheckBox { text: "Enable plugins (PDF viewer is always on)"; checked: w.get("web.pluginsEnabled"); onToggled: w.set("web.pluginsEnabled", checked) }
            QQC2.CheckBox { text: "Smooth scrolling"; checked: w.get("web.smoothScrolling"); onToggled: w.set("web.smoothScrolling", checked) }
            QQC2.CheckBox { text: "Spatial navigation (arrow keys between links)"; checked: w.get("web.spatialNavigation"); onToggled: w.set("web.spatialNavigation", checked) }
            QQC2.CheckBox { text: "Prefetch DNS"; checked: w.get("web.dnsPrefetch"); onToggled: w.set("web.dnsPrefetch", checked) }
            QQC2.CheckBox { text: "Print backgrounds"; checked: w.get("web.printBackgrounds"); onToggled: w.set("web.printBackgrounds", checked) }
            QQC2.CheckBox { Kirigami.FormData.label: "Spelling:"; text: "Check spelling while typing"; checked: w.get("web.spellcheck"); onToggled: w.set("web.spellcheck", checked) }
            QQC2.TextField { Kirigami.FormData.label: "Languages:"; text: w.get("web.spellcheckLanguages").join(", "); onEditingFinished: w.set("web.spellcheckLanguages", text.split(",").map(s => s.trim()).filter(s => s)) }
            QQC2.TextField { Kirigami.FormData.label: "User agent:"; Layout.fillWidth: true; placeholderText: "Default"; text: w.get("web.userAgent"); onEditingFinished: w.set("web.userAgent", text) }
            QQC2.Label { text: "User agent and spelling changes apply after restarting Kite."; opacity: 0.7; font: Kirigami.Theme.smallFont }
        }
    }

    Component {
        id: search
        ColumnLayout {
            id: se
            property var engines: JSON.parse(Browser.engines())
            function save() { Browser.saveEngines(JSON.stringify(engines)); engines = JSON.parse(Browser.engines()) }
            QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; text: "Type a shortcut followed by a space and your search in the location bar, e.g. “w KDE”. Use %s where the search terms go." }
            Repeater {
                model: se.engines
                delegate: RowLayout {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    QQC2.TextField { text: modelData.name; Layout.preferredWidth: Kirigami.Units.gridUnit * 8; onEditingFinished: { se.engines[index].name = text; se.save() } }
                    QQC2.TextField { text: modelData.url; Layout.fillWidth: true; onEditingFinished: { se.engines[index].url = text; se.save() } }
                    QQC2.TextField { text: modelData.shortcut; Layout.preferredWidth: Kirigami.Units.gridUnit * 3; placeholderText: "key"; onEditingFinished: { se.engines[index].shortcut = text; se.save() } }
                    QQC2.ToolButton { icon.name: "edit-delete"; onClicked: { se.engines.splice(index, 1); se.save() } }
                }
            }
            QQC2.Button { text: "Add Search Engine"; icon.name: "list-add"; onClicked: { se.engines = se.engines.concat([{name: "New engine", url: "https://example.com/search?q=%s", shortcut: "", suggestions_url: "", post_data: ""}]); se.save() } }
        }
    }

    Component {
        id: privacy
        ColumnLayout {
            Kirigami.FormLayout {
                Layout.fillWidth: true
                QQC2.CheckBox { Kirigami.FormData.label: "History:"; text: "Remember browsing history"; checked: w.get("privacy.saveHistory"); onToggled: w.set("privacy.saveHistory", checked) }
                QQC2.CheckBox { text: "Clear history, cookies and cache when Kite closes"; checked: w.get("privacy.clearOnExit"); onToggled: w.set("privacy.clearOnExit", checked) }
                QQC2.ComboBox {
                    Kirigami.FormData.label: "Cookies:"
                    model: [{k: "all", t: "Allow all cookies"}, {k: "noThirdParty", t: "Block third-party cookies"}, {k: "none", t: "Block all cookies (many sites won't work)"}]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("privacy.cookies")))
                    onActivated: i => w.set("privacy.cookies", model[i].k)
                }
                QQC2.CheckBox { Kirigami.FormData.label: "Tracking:"; text: "Send “Do Not Track” requests"; checked: w.get("web.doNotTrack"); onToggled: w.set("web.doNotTrack", checked) }
                QQC2.CheckBox { text: "Tell sites which page linked to them (referrer)"; checked: w.get("privacy.sendReferer"); onToggled: w.set("privacy.sendReferer", checked) }
            }
            Kirigami.Heading { level: 3; text: "Clear Recent History" }
            Kirigami.FormLayout {
                Layout.fillWidth: true
                QQC2.ComboBox { id: since; Kirigami.FormData.label: "Time range:"; model: ["Last hour", "Today", "Last 7 days", "Everything"] }
                QQC2.CheckBox { id: cHistory; Kirigami.FormData.label: "Clear:"; text: "Browsing history"; checked: true }
                QQC2.CheckBox { id: cCookies; text: "Cookies and site data"; checked: true }
                QQC2.CheckBox { id: cCache; text: "Cache"; checked: true }
                QQC2.CheckBox { id: cDownloads; text: "Download history"; checked: false }
                QQC2.CheckBox { id: cClosed; text: "Recently closed tabs and windows"; checked: false }
                QQC2.Button {
                    text: "Clear Now"; icon.name: "edit-clear-history"
                    onClicked: {
                        const now = Date.now()
                        const s = [now - 3600000, new Date().setHours(0, 0, 0, 0), now - 7 * 86400000, 0][since.currentIndex]
                        Browser.clearPrivateData(JSON.stringify({since: s, history: cHistory.checked, cookies: cCookies.checked, cache: cCache.checked, downloads: cDownloads.checked, closedTabs: cClosed.checked}))
                        w.app.toast("Browsing data cleared", "edit-clear-history")
                    }
                }
            }
        }
    }

    Component {
        id: passwords
        ColumnLayout {
            id: pw
            spacing: Kirigami.Units.largeSpacing
            property var entries: []
            property var exceptions: []
            property var revealed: ({})
            property string message: ""
            property bool switching: false
            function switchTo(target) {
                if (target === Passwords.backend) return
                if (target === "kwallet" && !Passwords.kwalletAvailable) { message = "KDE Wallet is not running or is disabled in System Settings."; storage.currentIndex = storage.model.findIndex(m => m.k === Passwords.backend); return }
                const go = () => {
                    if (target === "encrypted") { masterDlg.mode = "set"; masterDlg.open(); return }
                    if (Passwords.backend === "encrypted" && target === "plain") { masterDlg.mode = "remove"; masterDlg.open(); return }
                    switching = true
                    Passwords.switchBackend(target, "")
                }
                // Logins must be readable before they can be moved.
                if (Passwords.locked) w.app.unlockPasswords(ok => { if (ok) go(); else storage.currentIndex = storage.model.findIndex(m => m.k === Passwords.backend) })
                else go()
            }
            Connections {
                target: Passwords
                function onBackendSwitched(error) {
                    pw.switching = false
                    pw.message = error !== "" ? "Could not change storage: " + error
                        : Passwords.backend === "kwallet" ? "Saved logins are now stored in KDE Wallet." : "Storage changed."
                    storage.currentIndex = storage.model.findIndex(m => m.k === Passwords.backend)
                    pw.reload()
                }
            }
            function reload() {
                entries = Passwords.locked ? [] : JSON.parse(Passwords.list())
                exceptions = JSON.parse(Passwords.exceptions())
            }
            Component.onCompleted: reload()
            Connections { target: Passwords; function onChanged() { pw.reload() } }
            readonly property var shown: entries.filter(e => search.text === "" || (e.host + " " + e.username).toLowerCase().includes(search.text.toLowerCase()))

            Kirigami.FormLayout {
                Layout.fillWidth: true
                QQC2.CheckBox { Kirigami.FormData.label: "Logins:"; text: "Offer to save passwords"; checked: w.get("passwords.save"); onToggled: w.set("passwords.save", checked) }
                QQC2.CheckBox { text: "Fill in saved logins automatically"; checked: w.get("passwords.autofill"); onToggled: w.set("passwords.autofill", checked) }
                QQC2.ComboBox {
                    id: storage
                    Kirigami.FormData.label: "Store logins in:"
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 18
                    enabled: !Passwords.walletOpening && !pw.switching
                    model: [
                        {k: "plain", t: "Kite database (not encrypted)"},
                        {k: "encrypted", t: "Kite database, encrypted with a master password"},
                        {k: "kwallet", t: Passwords.kwalletAvailable ? "KDE Wallet" : "KDE Wallet (not available)"}
                    ]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === Passwords.backend))
                    onActivated: i => pw.switchTo(model[i].k)
                }
                QQC2.Label {
                    visible: Passwords.backend === "kwallet" || pw.switching || Passwords.walletOpening
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: Passwords.walletOpening ? "Opening KDE Wallet…" : pw.switching ? "Moving saved logins…"
                        : "Logins are kept in your network wallet, in the same folder Falkon uses, so both browsers share them."
                }
                RowLayout {
                    visible: Passwords.backend === "encrypted"
                    QQC2.Button { text: "Change Master Password…"; icon.name: "document-encrypt"; onClicked: { masterDlg.mode = "change"; masterDlg.open() } }
                    QQC2.Button { visible: !Passwords.locked; text: "Lock Now"; icon.name: "object-locked"; onClicked: Passwords.lock() }
                }
            }

            Kirigami.Heading { level: 3; text: "Saved Logins" }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: Passwords.locked
                text: Passwords.backend === "kwallet" ? (Passwords.walletOpening ? "Opening KDE Wallet…" : "KDE Wallet is closed.") : "Saved logins are protected by your master password."
                actions: [Kirigami.Action { text: "Unlock"; icon.name: "object-unlocked"; onTriggered: w.app.unlockPasswords(ok => pw.reload()) }]
            }
            RowLayout {
                visible: !Passwords.locked
                Kirigami.SearchField { id: search; Layout.fillWidth: true }
                QQC2.Button { text: "Add"; icon.name: "list-add"; onClicked: { editDlg.entry = null; editDlg.open() } }
                QQC2.Button { text: "Import…"; icon.name: "document-import"; onClicked: importDlg.open() }
                QQC2.Button { text: "Export…"; icon.name: "document-export"; enabled: pw.entries.length > 0; onClicked: exportWarn.open() }
            }
            QQC2.Label { visible: !Passwords.locked && pw.entries.length === 0; text: "No saved logins."; opacity: 0.7 }
            Repeater {
                model: pw.shown
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Kirigami.Icon { source: "globe"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                    QQC2.Label { text: modelData.host; Layout.preferredWidth: Kirigami.Units.gridUnit * 11; elide: Text.ElideMiddle; font.bold: true }
                    QQC2.Label { text: modelData.username || "(no username)"; Layout.preferredWidth: Kirigami.Units.gridUnit * 9; elide: Text.ElideRight }
                    QQC2.Label { text: pw.revealed[modelData.id] ? modelData.password : "••••••••"; Layout.fillWidth: true; elide: Text.ElideRight; font.family: "monospace" }
                    QQC2.ToolButton { icon.name: pw.revealed[modelData.id] ? "password-show-off" : "password-show-on"; onClicked: { const r = Object.assign({}, pw.revealed); r[modelData.id] = !r[modelData.id]; pw.revealed = r } QQC2.ToolTip.text: "Show password"; QQC2.ToolTip.visible: hovered }
                    QQC2.ToolButton { icon.name: "edit-copy"; onClicked: { clip.text = modelData.password; clip.selectAll(); clip.copy(); clip.text = ""; w.app.toast("Password copied", "edit-copy") } QQC2.ToolTip.text: "Copy password"; QQC2.ToolTip.visible: hovered }
                    QQC2.ToolButton { icon.name: "document-edit"; onClicked: { editDlg.entry = modelData; editDlg.open() } }
                    QQC2.ToolButton { icon.name: "edit-delete"; onClicked: Passwords.remove(modelData.id) }
                }
            }
            TextEdit { id: clip; visible: false }
            QQC2.Button { visible: pw.entries.length > 0; text: "Remove All Logins…"; icon.name: "edit-delete"; onClicked: removeAllDlg.open() }

            Kirigami.Heading { level: 3; text: "Never Saved For" }
            QQC2.Label { visible: pw.exceptions.length === 0; text: "No exceptions."; opacity: 0.7 }
            Repeater {
                model: pw.exceptions
                delegate: RowLayout {
                    required property string modelData
                    QQC2.Label { text: modelData; Layout.preferredWidth: Kirigami.Units.gridUnit * 16 }
                    QQC2.ToolButton { icon.name: "edit-delete"; onClicked: Passwords.removeException(modelData) }
                }
            }
            QQC2.Label { visible: pw.message !== ""; text: pw.message }

            FileDialog { id: importDlg; title: "Import Passwords"; nameFilters: ["Passwords (*.csv *.xml)", "CSV from Firefox, Chrome, Bitwarden (*.csv)", "Falkon export (*.xml)"]; onAccepted: pw.message = Passwords.importFile(selectedFile.toString()) }
            FileDialog { id: exportDlg; title: "Export Passwords"; fileMode: FileDialog.SaveFile; nameFilters: ["CSV for Firefox/Chrome (*.csv)", "Falkon XML (*.xml)"]; currentFile: "file://" + w.get("downloads.directory") + "/kite-passwords.csv"; onAccepted: pw.message = Passwords.exportFile(selectedFile.toString()) }
            Kirigami.PromptDialog {
                id: exportWarn
                title: "Export Passwords?"
                subtitle: "Your passwords will be saved as readable text. Anyone who can open the file can see them."
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                onAccepted: exportDlg.open()
            }
            Kirigami.PromptDialog {
                id: removeAllDlg
                title: "Remove All Saved Logins?"
                subtitle: "This cannot be undone."
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                onAccepted: Passwords.removeAll()
            }
            Kirigami.Dialog {
                id: editDlg
                property var entry: null
                title: entry ? "Edit Login" : "Add Login"
                standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
                padding: Kirigami.Units.largeSpacing
                onOpened: { eHost.text = entry ? entry.host : ""; eUser.text = entry ? entry.username : ""; ePass.text = entry ? entry.password : ""; eHost.forceActiveFocus() }
                Kirigami.FormLayout {
                    QQC2.TextField { id: eHost; Kirigami.FormData.label: "Site:"; placeholderText: "example.com"; Layout.preferredWidth: Kirigami.Units.gridUnit * 18 }
                    QQC2.TextField { id: eUser; Kirigami.FormData.label: "Username:" }
                    Kirigami.PasswordField { id: ePass; Kirigami.FormData.label: "Password:" }
                }
                onAccepted: {
                    const d = {host: eHost.text, username: eUser.text, password: ePass.text}
                    pw.message = editDlg.entry ? Passwords.update(JSON.stringify(Object.assign({id: editDlg.entry.id}, d))) : Passwords.add(JSON.stringify(d))
                }
            }
            Kirigami.Dialog {
                id: masterDlg
                property string mode: "set"   // set | change | remove
                title: mode === "set" ? "Set Master Password" : mode === "change" ? "Change Master Password" : "Remove Master Password"
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                padding: Kirigami.Units.largeSpacing
                onOpened: { mCur.text = ""; mNew.text = ""; mNew2.text = ""; mErr.text = "" }
                onRejected: storage.currentIndex = storage.model.findIndex(m => m.k === Passwords.backend)
                ColumnLayout {
                    QQC2.Label {
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 22; wrapMode: Text.Wrap
                        text: masterDlg.mode === "remove" ? "Your saved logins will be stored without encryption."
                            : "Saved logins will be encrypted. You'll need this password once per session to use them. If you forget it, the saved logins cannot be recovered."
                    }
                    Kirigami.FormLayout {
                        Kirigami.PasswordField { id: mCur; Kirigami.FormData.label: "Current password:"; visible: masterDlg.mode !== "set" && Passwords.backend === "encrypted" }
                        Kirigami.PasswordField { id: mNew; Kirigami.FormData.label: "New password:"; visible: masterDlg.mode !== "remove" }
                        Kirigami.PasswordField { id: mNew2; Kirigami.FormData.label: "Repeat:"; visible: masterDlg.mode !== "remove" }
                    }
                    QQC2.Label { id: mErr; color: Kirigami.Theme.negativeTextColor; visible: text !== "" }
                }
                onAccepted: {
                    let err = ""
                    if (mode !== "remove" && mNew.text !== mNew2.text) err = "The new passwords do not match"
                    else if (mode !== "remove" && mNew.text.length < 6) err = "Use at least 6 characters"
                    else err = mode === "remove" ? Passwords.removeMasterPassword(mCur.text) : Passwords.setMasterPassword(mCur.text, mNew.text)
                    if (err !== "") { mErr.text = err; Qt.callLater(open) }
                    else { pw.message = mode === "remove" ? "Master password removed" : "Master password set"; pw.reload() }
                    storage.currentIndex = storage.model.findIndex(m => m.k === Passwords.backend)
                }
            }
        }
    }

    Component {
        id: adblock
        ColumnLayout {
            id: ab
            property var subs: w.get("adblock.subscriptions")
            property string status: ""
            Connections { target: Browser; function onAdblockStatus(m) { ab.status = m } }
            readonly property string extBlocker: Browser.contentBlocker()
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: ab.extBlocker !== ""
                type: Kirigami.MessageType.Information
                text: ab.extBlocker + " is blocking content, so Kite's built-in blocker is paused. These settings apply again when it is disabled or removed."
            }
            QQC2.CheckBox { text: "Block ads and trackers"; checked: w.get("adblock.enabled"); onToggled: w.set("adblock.enabled", checked) }
            Kirigami.Heading { level: 3; text: "Filter Subscriptions" }
            Repeater {
                model: ab.subs
                delegate: RowLayout {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    QQC2.CheckBox { checked: modelData.enabled; onToggled: { const s = ab.subs.slice(); s[index].enabled = checked; ab.subs = s; w.set("adblock.subscriptions", s) } }
                    ColumnLayout { spacing: 0; Layout.fillWidth: true
                        QQC2.Label { text: modelData.title }
                        QQC2.Label { text: modelData.url; font: Kirigami.Theme.smallFont; opacity: 0.7; elide: Text.ElideMiddle; Layout.fillWidth: true }
                    }
                    QQC2.ToolButton { icon.name: "edit-delete"; onClicked: { const s = ab.subs.slice(); s.splice(index, 1); ab.subs = s; w.set("adblock.subscriptions", s) } }
                }
            }
            RowLayout {
                QQC2.TextField { id: subTitle; placeholderText: "Title" }
                QQC2.TextField { id: subUrl; placeholderText: "https://… list URL"; Layout.fillWidth: true }
                QQC2.Button { text: "Add"; icon.name: "list-add"; enabled: subUrl.text.startsWith("http"); onClicked: { const s = ab.subs.concat([{title: subTitle.text || subUrl.text, url: subUrl.text, enabled: true}]); ab.subs = s; w.set("adblock.subscriptions", s); subUrl.text = ""; subTitle.text = "" } }
            }
            QQC2.Menu {
                id: presets
                QQC2.MenuItem { text: "uBlock filters"; onTriggered: { subTitle.text = text; subUrl.text = "https://ublockorigin.github.io/uAssets/filters/filters.min.txt" } }
                QQC2.MenuItem { text: "Peter Lowe's list"; onTriggered: { subTitle.text = text; subUrl.text = "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&mimetype=plaintext" } }
                QQC2.MenuItem { text: "Fanboy's Annoyances"; onTriggered: { subTitle.text = text; subUrl.text = "https://easylist.to/easylist/fanboy-annoyance.txt" } }
                QQC2.MenuItem { text: "EasyList Germany"; onTriggered: { subTitle.text = text; subUrl.text = "https://easylist.to/easylistgermany/easylistgermany.txt" } }
            }
            QQC2.Button { text: "Suggested Lists…"; icon.name: "view-list-details"; onClicked: presets.popup() }
            Kirigami.Heading { level: 3; text: "Custom Rules" }
            QQC2.TextArea {
                id: rules
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 8
                font.family: "monospace"
                placeholderText: "||ads.example.com^\nexample.org##.banner"
                text: w.get("adblock.customRules")
                onEditingFinished: w.set("adblock.customRules", text)
            }
            Kirigami.Heading { level: 3; text: "Disabled on These Sites" }
            QQC2.Label { text: w.get("adblock.allowlist").join(", ") || "None"; wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                QQC2.Button { text: "Apply"; icon.name: "dialog-ok-apply"; onClicked: { w.set("adblock.customRules", rules.text); Browser.reloadAdblock(false); ab.status = "Rebuilding…" } }
                QQC2.Button { text: "Update Lists Now"; icon.name: "view-refresh"; onClicked: { Browser.reloadAdblock(true); ab.status = "Downloading lists…" } }
                QQC2.Label { text: ab.status; opacity: 0.8 }
            }
        }
    }

    Component {
        id: downloads
        Kirigami.FormLayout {
            RowLayout {
                Kirigami.FormData.label: "Save files to:"
                QQC2.TextField { id: dir; text: w.get("downloads.directory"); Layout.preferredWidth: Kirigami.Units.gridUnit * 20; onEditingFinished: w.set("downloads.directory", text) }
                QQC2.Button { icon.name: "document-open-folder"; onClicked: folderDialog.open() }
                FolderDialog { id: folderDialog; onAccepted: { dir.text = selectedFolder.toString().replace("file://", ""); w.set("downloads.directory", dir.text) } }
            }
            QQC2.CheckBox { text: "Always ask where to save files"; checked: w.get("downloads.askWhereToSave"); onToggled: w.set("downloads.askWhereToSave", checked) }
        }
    }

    Component {
        id: sites
        ColumnLayout {
            id: ss
            property var list: JSON.parse(Browser.allSiteSettings())
            QQC2.Label { visible: ss.list.length === 0; text: "No site-specific settings yet. Zoom levels you set are remembered per site." }
            Repeater {
                model: ss.list
                delegate: RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    QQC2.Label { text: modelData.host; Layout.fillWidth: true; font.bold: true }
                    QQC2.Label { text: modelData.settings.zoom > 0 ? "Zoom " + modelData.settings.zoom + "%" : "" }
                    QQC2.ToolButton { icon.name: "edit-delete"; onClicked: { Browser.clearSiteSettings(modelData.host); ss.list = JSON.parse(Browser.allSiteSettings()) } }
                }
            }
        }
    }

    Component {
        id: extensions
        Kirigami.FormLayout {
            QQC2.CheckBox { Kirigami.FormData.label: "Extensions:"; text: "Enable extensions"; checked: w.get("extensions.enabled"); onToggled: { w.set("extensions.enabled", checked); w.app.installUserScripts(); w.app.backgroundHost.reload() } }
            QQC2.CheckBox { text: "Identify as Firefox on addons.mozilla.org"; checked: w.get("extensions.amoSpoofFirefox"); onToggled: w.set("extensions.amoSpoofFirefox", checked) }
            QQC2.Label { Layout.maximumWidth: Kirigami.Units.gridUnit * 26; wrapMode: Text.Wrap; opacity: 0.75; text: "With this enabled, the add-on site shows “Add to Firefox” buttons; clicking one installs the add-on into Kite." }
            QQC2.Button { text: "Manage Add-ons…"; icon.name: "preferences-plugin"; onClicked: w.app.showAddons() }
        }
    }
}
