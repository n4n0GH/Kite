import QtCore
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite

// Falkon's Preferences dialog.
ManagerWindow {
    id: w
    property string page: "general"
    title: "Settings — Kite"
    width: Kirigami.Units.gridUnit * 50
    height: Kirigami.Units.gridUnit * 36

    // ---- changes wait for Apply (the bar at the bottom), as in System Settings ----
    // Settings changed on the page: key → value. Pages read through get(), so
    // they show what's waiting (and controls that depend on it follow).
    property var pending: ({})
    // Changes that aren't settings (the search engines, a plugin turned on or
    // off): name → what Apply does.
    property var pendingActions: ({})
    readonly property bool changed: Object.keys(pending).length > 0 || Object.keys(pendingActions).length > 0
    function get(k) { return k in pending ? pending[k] : JSON.parse(Browser.setting(k)) }
    function set(k, v) {
        const p = Object.assign({}, pending)
        if (JSON.stringify(v) === JSON.stringify(JSON.parse(Browser.setting(k)))) delete p[k]
        else p[k] = v
        pending = p
    }
    function stage(name, action) { const a = Object.assign({}, pendingActions); a[name] = action; pendingActions = a }
    function unstage(name) { const a = Object.assign({}, pendingActions); delete a[name]; pendingActions = a }
    // Right away, for what acts at once anyway (a start page file being made).
    function setNow(k, v) { Browser.setSetting(k, JSON.stringify(v)) }
    function applyChanges() {
        const p = pending, actions = pendingActions
        // Saved before the list is emptied: emptying it makes the page read the
        // settings again at once, and it must find the new ones (read before
        // they're saved, a control would show the old value, saved or not).
        for (const k in p) Browser.setSetting(k, JSON.stringify(p[k]))
        for (const name in actions) actions[name]()
        pending = ({})
        pendingActions = ({})
        // (The filters are only rebuilt on request.)
        if ("adblock.subscriptions" in p || "adblock.customRules" in p) Browser.reloadAdblock(false)
    }
    function discardChanges() {
        pending = ({})
        pendingActions = ({})
        // The page starts over from the saved settings.
        pageLoader.active = false
        pageLoader.active = true
    }

    // ---- what every page is made of (KDE HIG: forms with text section headings) ----
    // A page: one column, centred, of the same width on every page.
    readonly property real columnWidth: Kirigami.Units.gridUnit * 36
    // Its width and place on a page, the same whether the page scrolls or not:
    // from the page's width, with room for a scrollbar always kept (a page
    // makes room for one only when it shows it). A ScrollablePage stretches
    // what it holds to its room, so the column sits in an item of its own.
    readonly property real scrollbarRoom: Kirigami.Units.gridUnit * 1.5
    function columnWidthIn(page) { return Math.min(page.width - page.leftPadding * 2 - scrollbarRoom, columnWidth) }
    function columnXIn(page) { return (page.width - page.leftPadding * 2 - scrollbarRoom - columnWidthIn(page)) / 2 }
    // (Shown once its forms have lined up their labels: Kirigami does that a turn
    // of the event loop after they're made, so the first frame could show each
    // section's form on its own, and the rows jump into place.)
    component PageColumn: ColumnLayout {
        id: pageColumn
        property bool settled: false
        spacing: Kirigami.Units.largeSpacing
        opacity: settled ? 1 : 0
        Component.onCompleted: Qt.callLater(() => Qt.callLater(() => pageColumn.settled = true))
    }
    // A section's title, as System Settings has them: on the left, a line after it.
    // (Without the padding it has as a list's item: it lines up with the rest of the column.)
    component SectionHeader: Kirigami.ListSectionHeader { Layout.fillWidth: true; leftPadding: 0; rightPadding: 0 }
    // A section's form. The forms of a page line up their labels with each other.
    component Form: Kirigami.FormLayout {
        id: form
        readonly property bool kiteForm: true
        Layout.fillWidth: true
        Component.onCompleted: twinFormLayouts = form.parent ? form.parent.children.filter(c => c !== form && c.kiteForm === true) : []
    }
    // What a setting does, under it.
    component Hint: QQC2.Label {
        Layout.fillWidth: true
        Layout.maximumWidth: Kirigami.Units.gridUnit * 24
        wrapMode: Text.Wrap
        opacity: 0.7
        font: Kirigami.Theme.smallFont
    }
    // A user agent to choose: common browsers' (Browser.userAgentPresets), or one
    // typed in a field shown for it. `value` is what's stored ("" for the first
    // option, kite:qtwebengine for Kite naming QtWebEngine).
    component UserAgentField: ColumnLayout {
        id: uaf
        property string firstOption: "Like other sites"
        readonly property var presets: JSON.parse(Browser.userAgentPresets())
        readonly property int customIndex: presets.length + 1
        readonly property string value: uaCombo.currentIndex === 0 ? "" : uaCombo.currentIndex === customIndex ? uaText.text.trim() : presets[uaCombo.currentIndex - 1].userAgent
        // The user changed it.
        signal edited()
        function load(ua) {
            const preset = presets.findIndex(p => p.userAgent === ua)
            uaCombo.currentIndex = !ua ? 0 : preset >= 0 ? preset + 1 : customIndex
            uaText.text = uaCombo.currentIndex === customIndex ? ua : ""
        }
        spacing: Kirigami.Units.smallSpacing
        QQC2.ComboBox {
            id: uaCombo
            model: [uaf.firstOption].concat(uaf.presets.map(p => p.name), ["Custom…"])
            onActivated: { if (currentIndex === uaf.customIndex) uaText.forceActiveFocus(); uaf.edited() }
        }
        QQC2.TextField {
            id: uaText
            visible: uaCombo.currentIndex === uaf.customIndex
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            placeholderText: "Type or paste a user agent"
            onTextEdited: uaf.edited()
        }
    }
    // A row of a list on a page: highlighted under the pointer (KDE's list row),
    // so it's clear which row a button belongs to. Rows aren't clicked themselves.
    component ListRow: QQC2.ItemDelegate {
        id: listRow
        default property alias content: rowBox.data
        Layout.fillWidth: true
        hoverEnabled: true
        down: false
        highlighted: false
        topPadding: Kirigami.Units.smallSpacing
        bottomPadding: Kirigami.Units.smallSpacing
        contentItem: Item {
            id: rowBox
            implicitHeight: children.length ? children[0].implicitHeight : 0
            implicitWidth: children.length ? children[0].implicitWidth : 0
            // (Its one child, a RowLayout, fills it.)
            Component.onCompleted: if (children.length) { children[0].anchors.fill = rowBox }
        }
    }
    // A row's icon button: its name as a tooltip.
    component RowButton: QQC2.ToolButton {
        display: QQC2.AbstractButton.IconOnly
        QQC2.ToolTip.text: text
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }
    // A setting's checkbox.
    component Check: QQC2.CheckBox {
        property string key
        checked: !!w.get(key)
        onToggled: w.set(key, checked)
    }
    // Apply and Cancel, at the bottom of a page with changes to apply.
    component ApplyBar: QQC2.ToolBar {
        id: applyBar
        property bool changed
        property alias leading: leadingRow.data
        signal apply()
        signal cancel()
        position: QQC2.ToolBar.Footer
        contentItem: RowLayout {
            RowLayout { id: leadingRow }
            Item { Layout.fillWidth: true }
            QQC2.DialogButtonBox {
                // (The order of the buttons is the platform's.)
                QQC2.Button { text: "Apply"; icon.name: "dialog-ok-apply"; enabled: applyBar.changed; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ApplyRole; onClicked: applyBar.apply() }
                QQC2.Button { text: "Cancel"; icon.name: "dialog-cancel"; enabled: applyBar.changed; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole; onClicked: applyBar.cancel() }
            }
        }
    }

    readonly property var pages: [
        {key: "general", title: "General", icon: "go-home"},
        {key: "appearance", title: "Appearance", icon: "preferences-desktop-theme"},
        {key: "browsing", title: "Browsing", icon: "internet-web-browser"},
        {key: "search", title: "Search Engines", icon: "search"},
        {key: "privacy", title: "Privacy", icon: "preferences-system-privacy"},
        {key: "passwords", title: "Passwords", icon: "dialog-password"},
        {key: "adblock", title: "Content Blocking", icon: "security-high"},
        {key: "downloads", title: "Downloads", icon: "download"},
        {key: "sites", title: "Site Settings", icon: "preferences-web-browser-cookies"},
        {key: "extensions", title: "Plugins & Extensions", icon: "kite-extension"},
        {key: "shortcuts", title: "Shortcuts", icon: "preferences-desktop-keyboard-shortcut"},
    ]

    globalDrawer: Kirigami.GlobalDrawer {
        modal: false
        width: Kirigami.Units.gridUnit * 12
        actions: w.pages.map(p => pageAction.createObject(w, {text: p.title, iconName: p.icon, key: p.key}))
    }
    // (Content Blocking is listed while blocking is on, as set on the Browsing page,
    // or while it's the page shown.)
    Component { id: pageAction; Kirigami.Action { property string key; property string iconName; icon.name: iconName; checked: w.page === key; visible: key !== "adblock" || !!w.get("adblock.enabled") || w.page === key; onTriggered: w.page = key } }

    // Submenus (a start page file, a site's settings) are pages pushed on
    // the stack: they slide in, one at a time, with a back arrow in the
    // header (like Discover).
    pageStack.columnView.columnResizeMode: Kirigami.ColumnView.SingleColumn
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.ToolBar
    pageStack.globalToolBar.showNavigationButtons: Kirigami.ApplicationHeaderStyle.ShowBackButton

    pageStack.initialPage: Kirigami.ScrollablePage {
        id: sp
        title: (w.pages.find(p => p.key === w.page) || {}).title || ""
        // Leaving with changes not applied asks first (leavePage), like a site's page.
        readonly property bool unsaved: w.changed
        function applyChanges() { w.applyChanges() }
        function discardChanges() { w.discardChanges() }
        footer: ApplyBar {
            changed: w.changed
            onApply: w.applyChanges()
            onCancel: w.discardChanges()
            // Plugins & Extensions' own buttons.
            leading: [
                QQC2.Button {
                    visible: w.page === "extensions"
                    text: "Install Plugin…"
                    icon.name: "document-open"
                    onClicked: if (pageLoader.item && pageLoader.item.installPlugin) pageLoader.item.installPlugin()
                },
                QQC2.Button {
                    visible: w.page === "extensions"
                    // (As "Enable WebExtensions" is set, applied or not.)
                    enabled: !!w.get("extensions.enabled")
                    text: "Manage Extensions…"
                    icon.name: "kite-extension"
                    onClicked: w.app.showAddons()
                }
            ]
        }
        Item {
          implicitHeight: pageLoader.height
          Loader {
            id: pageLoader
            width: w.columnWidthIn(sp)
            x: w.columnXIn(sp)
            sourceComponent: {
                switch (w.page) {
                case "appearance": return appearance
                case "browsing": return browsing
                case "search": return search
                case "privacy": return privacy
                case "passwords": return passwords
                case "adblock": return adblock
                case "downloads": return downloads
                case "sites": return sites
                case "extensions": return extensions
                case "shortcuts": return shortcuts
                default: return general
                }
            }
          }
        }
    }

    // Dangerous Sites' lists, for `kite --debug` (Browsing → Dangerous Sites).
    Component {
        id: safeBrowsingListsPage
        Kirigami.ScrollablePage {
            id: sbPage
            title: "Dangerous Sites Lists"
            Item {
            implicitHeight: sbl.implicitHeight
            PageColumn {
                id: sbl
                width: w.columnWidthIn(sbPage)
                x: w.columnXIn(sbPage)
                readonly property int pageSize: 500
                property var entries: []
                property int total: 0
                function load(more) {
                    const r = JSON.parse(Browser.safeBrowsingEntries(search.text, more ? entries.length : 0, pageSize))
                    total = r.total || 0
                    entries = (more ? entries : []).concat(r.entries || [])
                }
                Component.onCompleted: load(false)
                // The check, written out.
                function explain(r) {
                    if (r.error) return r.error
                    if (r.mode === "off") return "Dangerous Sites is off."
                    const lines = []
                    if (r.allowed) lines.push("Allowed for this session (the user chose to visit it anyway): not checked.")
                    if (r.mode === "open") {
                        lines.push(r.entry ? "Listed: " + r.entry + " (" + r.threat + ")" : "Not listed: loads normally.")
                        return lines.join("\n")
                    }
                    let listed = false
                    for (const e of r.expressions || []) {
                        lines.push(e.expression)
                        lines.push("  sha256 " + e.hash)
                        for (const m of e.matches) {
                            listed = true
                            lines.push("  matches " + m.list + " prefix " + m.prefix + (m.knownClean ? " (Google said clean, cached)" : " (would be confirmed with Google)"))
                        }
                        if (e.confirmed) lines.push("  confirmed dangerous by Google (cached): " + e.confirmed)
                    }
                    lines.push("")
                    lines.push(listed ? "A hash prefix matches: Kite asks Google for the full hashes before warning." : "No hash prefix matches: loads normally, Google isn't asked.")
                    return lines.join("\n")
                }

                SectionHeader { text: "Check an Address" }
                RowLayout {
                    Layout.fillWidth: true
                    QQC2.TextField {
                        id: probe
                        Layout.fillWidth: true
                        placeholderText: "https://example.com/page"
                        onAccepted: checkButton.clicked()
                    }
                    QQC2.Button {
                        id: checkButton
                        text: "Check"
                        icon.name: "search"
                        enabled: probe.text.trim() !== ""
                        onClicked: verdict.text = sbl.explain(JSON.parse(Browser.safeBrowsingExplain(probe.text)))
                    }
                }
                QQC2.TextArea {
                    id: verdict
                    visible: text !== ""
                    Layout.fillWidth: true
                    readOnly: true
                    wrapMode: TextEdit.WrapAnywhere
                    font.family: "monospace"
                }
                Hint { Layout.maximumWidth: -1; text: "From the lists stored here; nothing is sent to Google for this." }

                SectionHeader { text: "Entries" }
                RowLayout {
                    Layout.fillWidth: true
                    Kirigami.SearchField {
                        id: search
                        Layout.fillWidth: true
                        Layout.minimumWidth: Kirigami.Units.gridUnit * 6
                        placeholderText: "Search entries…"
                        // (After typing stops for a moment: lists can be long.)
                        onTextChanged: searchDelay.restart()
                        Timer { id: searchDelay; interval: 300; onTriggered: sbl.load(false) }
                    }
                    QQC2.Label { text: sbl.total.toLocaleString(Qt.locale(), "f", 0) + (sbl.total === 1 ? " entry" : " entries"); opacity: 0.7 }
                }
                Hint {
                    Layout.maximumWidth: -1
                    text: "Open lists: the hosts and addresses as listed. Google: the hash prefixes (hex) of each list; addresses are matched by the SHA-256 of their host/path forms."
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Repeater {
                        model: sbl.entries
                        delegate: ListRow {
                            required property var modelData
                            RowLayout {
                                QQC2.Label { text: modelData.entry; textFormat: Text.PlainText; font.family: "monospace"; Layout.fillWidth: true; elide: Text.ElideMiddle }
                                QQC2.Label { text: modelData.threat; opacity: 0.7 }
                                QQC2.Label { visible: modelData.list !== "open lists"; text: modelData.list; opacity: 0.7; font: Kirigami.Theme.smallFont }
                            }
                        }
                    }
                }
                QQC2.Button {
                    visible: sbl.entries.length < sbl.total
                    text: "Show More (" + (sbl.total - sbl.entries.length).toLocaleString(Qt.locale(), "f", 0) + " left)"
                    icon.name: "go-down"
                    onClicked: sbl.load(true)
                }
            }
            }
        }
    }

    // Submenus of Privacy.
    Component { id: cookiesPage; CookiesPage { prefs: w } }
    Component { id: certificatesPage; CertificatesPage { } }

    // ---- the settings file (General) ----
    FileDialog {
        id: settingsExport
        title: "Export Settings"
        fileMode: FileDialog.SaveFile
        nameFilters: ["Kite settings (*.json)"]
        currentFile: "file://" + (w.get("downloads.directory") || StandardPaths.writableLocation(StandardPaths.HomeLocation).toString().replace("file://", "")) + "/kite-settings.json"
        onAccepted: importResult.show(Browser.exportSettings(selectedFile.toString()))
    }
    FileDialog {
        id: settingsImport
        title: "Import Settings"
        nameFilters: ["Kite settings (*.json)"]
        onAccepted: {
            const preview = JSON.parse(Browser.previewSettings(selectedFile.toString()))
            if (preview.error) importResult.show(preview.error)
            else importConfirm.ask(selectedFile.toString(), preview)
        }
    }
    QQC2.Dialog {
        id: importConfirm
        property string file: ""
        property var preview: ({})
        function ask(f, p) { file = f; preview = p; open() }
        function count(n, what) { return (n || 0) + " " + what + (n === 1 ? "" : "s") }
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: "Replace Your Settings?"
        width: Kirigami.Units.gridUnit * 26
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "This file has " + importConfirm.count(importConfirm.preview.settings, "changed setting") + ", " + importConfirm.count(importConfirm.preview.engines, "search engine") + " and settings for " + importConfirm.count(importConfirm.preview.sites, "site") + ". They replace yours; settings it doesn't mention go back to their defaults. A copy of the current ones is kept in your profile."
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!importConfirm.preview.scripts && importConfirm.preview.scripts.length > 0
                type: Kirigami.MessageType.Warning
                text: "It adds scripts that will run on: " + (importConfirm.preview.scripts || []).join(", ") + ". Those can read and change what you do there, so only go on if the file is your own or you trust who made it."
            }
        }
        footer: QQC2.DialogButtonBox {
            QQC2.Button { text: "Cancel"; icon.name: "dialog-cancel"; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole }
            QQC2.Button { text: "Replace Settings"; icon.name: "document-import"; QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole }
        }
        onAccepted: {
            const message = Browser.importSettings(file)
            w.app.installUserScripts()
            importResult.show(message + " A few, like the proxy, take effect after restarting Kite.")
        }
    }
    Kirigami.PromptDialog {
        id: importResult
        function show(text) { subtitle = text; open() }
        title: "Settings File"
        standardButtons: Kirigami.Dialog.Close
    }

    // A plugin's settings: a submenu each, so the list stays a list.
    function openPluginSettings(plugin) {
        leavePage(() => {
            if (pageStack.depth > 1) pageStack.pop(pageStack.initialPage)
            pageStack.push(pluginSettingsPage, {plugin: plugin})
        })
    }
    Component {
        id: pluginSettingsPage
        Kirigami.ScrollablePage {
            property var plugin
            title: plugin ? plugin.name : ""
            ColumnLayout {
                Loader {
                    Layout.fillWidth: true
                    Component.onCompleted: setSource(w.app.pluginUrl(plugin, plugin.settingsPage), {app: w.app, plugin: plugin})
                }
            }
        }
    }

    // ---- keyboard shortcuts ("shortcuts"; the actions are app.shortcutActions,
    // Kite's own: extensions configure their shortcuts themselves) ----
    Component {
        id: shortcuts
        PageColumn {
            id: sc
            readonly property var overrides: w.get("shortcuts") || {}
            readonly property var actions: w.app.shortcutActions
            readonly property var groups: actions.reduce((g, a) => g.includes(a.group) ? g : g.concat([a.group]), [])
            property string filter: ""
            function keys(a) { return sc.overrides[a.id] !== undefined ? sc.overrides[a.id] : a.keys }
            function same(x, y) { return x.length === y.length && x.every((k, i) => k === y[i]) }
            function setKeys(a, list) {
                const o = Object.assign({}, sc.overrides)
                if (sc.same(list, a.keys)) delete o[a.id]; else o[a.id] = list
                w.set("shortcuts", o)
            }
            function reset(a) { const o = Object.assign({}, sc.overrides); delete o[a.id]; w.set("shortcuts", o) }
            function matches(a) {
                const f = sc.filter.trim().toLowerCase()
                return f === "" || a.name.toLowerCase().includes(f) || a.group.toLowerCase().includes(f)
                    || sc.keys(a).some(k => k.toLowerCase().includes(f) || Browser.keySequenceDisplay(k).toLowerCase().includes(f))
            }
            // The action already using `seq`, other than `except`.
            function owner(seq, except) { return sc.actions.find(a => a.id !== except.id && sc.keys(a).includes(seq)) }

            RowLayout {
                Layout.fillWidth: true
                Kirigami.SearchField { Layout.fillWidth: true; placeholderText: "Search actions or keys…"; onTextChanged: sc.filter = text }
                QQC2.Button {
                    text: "Reset All"
                    icon.name: "edit-undo"
                    enabled: Object.keys(sc.overrides).length > 0
                    onClicked: resetAllDlg.open()
                }
            }
            Hint {
                Layout.maximumWidth: -1
                text: "Click a shortcut to change it, or + to add another one. Shortcuts need Ctrl, Alt or Meta (or a function key), so typing on web pages keeps working."
            }
            Repeater {
                model: sc.groups
                delegate: ColumnLayout {
                    id: grp
                    required property string modelData
                    readonly property var items: sc.actions.filter(a => a.group === grp.modelData && sc.matches(a))
                    Layout.fillWidth: true
                    visible: items.length > 0
                    spacing: 0
                    SectionHeader { text: grp.modelData }
                    Repeater {
                        model: grp.items
                        delegate: ListRow {
                            id: row
                            required property var modelData
                            readonly property var current: sc.keys(modelData)
                            readonly property bool changed: sc.overrides[modelData.id] !== undefined
                            RowLayout {
                                spacing: Kirigami.Units.smallSpacing
                                QQC2.Label { text: row.modelData.name; Layout.fillWidth: true; elide: Text.ElideRight; font.italic: row.changed }
                                QQC2.Label { visible: row.current.length === 0; text: "None"; opacity: 0.5 }
                                Repeater {
                                    model: row.current
                                    delegate: QQC2.Button {
                                        required property string modelData
                                        required property int index
                                        text: Browser.keySequenceDisplay(modelData)
                                        font.family: "monospace"
                                        onClicked: recorder.record(row.modelData, index)
                                        QQC2.ToolTip.text: "Change, or remove with the button in the dialog"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                                    }
                                }
                                RowButton {
                                    icon.name: "list-add"
                                    text: "Add a shortcut"
                                    onClicked: recorder.record(row.modelData, -1)
                                }
                                RowButton {
                                    icon.name: "edit-undo"
                                    // (Kept in its place when there's nothing to reset, so the keys line up.)
                                    enabled: row.changed
                                    opacity: row.changed ? 1 : 0
                                    text: "Reset to " + (row.modelData.keys.length ? row.modelData.keys.map(k => Browser.keySequenceDisplay(k)).join(", ") : "none")
                                    onClicked: sc.reset(row.modelData)
                                }
                            }
                        }
                    }
                }
            }

            Kirigami.PromptDialog {
                id: resetAllDlg
                parent: QQC2.Overlay.overlay; anchors.centerIn: parent
                title: "Reset All Shortcuts?"
                subtitle: "Every action gets its default keys back."
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                onAccepted: w.set("shortcuts", {})
            }

            // Records a key combination for an action: replaces shortcut
            // `index`, or adds one (index -1).
            Kirigami.Dialog {
                id: recorder
                parent: QQC2.Overlay.overlay; anchors.centerIn: parent
                property var action: null
                property int index: -1
                property string seq: ""
                property string error: ""
                property var conflict: null
                title: action ? action.name : ""
                padding: Kirigami.Units.largeSpacing
                standardButtons: Kirigami.Dialog.NoButton
                function record(a, i) { action = a; index = i; seq = ""; error = ""; conflict = null; open(); grab.forceActiveFocus() }
                function assign() {
                    if (conflict) { const other = conflict; sc.setKeys(other, sc.keys(other).filter(k => k !== seq)) }
                    const list = sc.keys(action).slice()
                    if (index >= 0) list[index] = seq; else list.push(seq)
                    sc.setKeys(action, list.filter((k, i) => list.indexOf(k) === i))
                    close()
                }
                function removeCurrent() {
                    const list = sc.keys(action).slice()
                    list.splice(index, 1)
                    sc.setKeys(action, list)
                    close()
                }
                onOpened: grab.forceActiveFocus()
                customFooterActions: [
                    Kirigami.Action { text: "Remove Shortcut"; icon.name: "edit-delete-remove"; visible: recorder.index >= 0; onTriggered: recorder.removeCurrent() },
                    Kirigami.Action { text: recorder.conflict ? "Reassign" : "Assign"; icon.name: "dialog-ok-apply"; enabled: recorder.seq !== "" && recorder.error === ""; onTriggered: recorder.assign() },
                    Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: recorder.close() }
                ]
                ColumnLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 22
                    QQC2.Label { text: recorder.index >= 0 ? "Press the new keys for this action:" : "Press the keys to add:" }
                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                        radius: Kirigami.Units.cornerRadius
                        color: Kirigami.Theme.alternateBackgroundColor
                        border.width: grab.activeFocus ? 2 : 1
                        border.color: grab.activeFocus ? Kirigami.Theme.highlightColor : Qt.alpha(Kirigami.Theme.textColor, 0.2)
                        QQC2.Label {
                            anchors.centerIn: parent
                            text: recorder.seq !== "" ? Browser.keySequenceDisplay(recorder.seq) : "…"
                            font.family: "monospace"
                            font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.4
                        }
                        Item {
                            id: grab
                            anchors.fill: parent
                            focus: true
                            // Take every key, also ones that are shortcuts elsewhere.
                            Keys.onShortcutOverride: event => event.accepted = true
                            Keys.onPressed: event => {
                                event.accepted = true
                                const k = event.key
                                if ([Qt.Key_Shift, Qt.Key_Control, Qt.Key_Alt, Qt.Key_Meta, Qt.Key_AltGr, Qt.Key_Super_L, Qt.Key_Super_R, Qt.Key_Hyper_L, Qt.Key_Hyper_R, Qt.Key_CapsLock, Qt.Key_NumLock].includes(k)) return
                                if (k === Qt.Key_Escape && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) { recorder.close(); return }
                                const key = k === Qt.Key_Backtab ? Qt.Key_Tab : k
                                const mods = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
                                const seq = Browser.keySequenceText(key, mods)
                                recorder.seq = seq
                                const fkey = key >= Qt.Key_F1 && key <= Qt.Key_F35
                                const media = [Qt.Key_Back, Qt.Key_Forward, Qt.Key_Refresh, Qt.Key_HomePage, Qt.Key_Search, Qt.Key_Favorites, Qt.Key_Print].includes(key)
                                recorder.error = (mods & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) || fkey || media ? ""
                                    : "Add Ctrl, Alt or Meta: keys without them are left to web pages."
                                const other = recorder.error === "" ? sc.owner(seq, recorder.action) : null
                                recorder.conflict = other || null
                            }
                        }
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        visible: text !== ""
                        color: recorder.error !== "" ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.neutralTextColor
                        text: recorder.error !== "" ? recorder.error
                            : recorder.conflict ? "Already used for “" + recorder.conflict.name + "” (" + recorder.conflict.group + "). Reassign moves it here."
                            : recorder.action && recorder.seq !== "" && sc.keys(recorder.action).includes(recorder.seq) && sc.keys(recorder.action).indexOf(recorder.seq) !== recorder.index ? "This action already has these keys."
                            : ""
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                        text: "Escape cancels. Default: " + (recorder.action && recorder.action.keys.length ? recorder.action.keys.map(k => Browser.keySequenceDisplay(k)).join(", ") : "none") + "."
                    }
                }
            }
        }
    }

    // Open the page of the home or new tab page's file ("home" | "newtab").
    function openStartPageFile(target) { pageStack.push(startPageFilePage, {target: target}) }

    // A home / new tab page file (General → "Manage…").
    Component {
        id: startPageFilePage
        Kirigami.ScrollablePage {
            id: page
            property string target   // "home" | "newtab"
            title: target === "home" ? "Home Page File" : "New Tab Page File"
            // ---- a home / new tab page file -------------------------------------
            // (Saved right away: its buttons make and copy files at once.)
            Item {
            implicitHeight: spm.implicitHeight
            PageColumn {
                id: spm
                width: w.columnWidthIn(page)
                x: w.columnXIn(page)
                readonly property string target: page.target
                readonly property string fileKey: "startpage." + target + ".file"
                readonly property string copyKey: "startpage." + target + ".copy"
                Component.onCompleted: {
                    spFile.text = w.get(fileKey)
                    spCopy.checked = w.get(copyKey)
                }
                function edit() { Qt.openUrlExternally("file://" + encodeURI(w.app.startPageFile(target))) }
                Form {
                    RowLayout {
                        Kirigami.FormData.label: "HTML file:"
                        Layout.fillWidth: true
                        QQC2.TextField {
                            id: spFile
                            Layout.fillWidth: true
                            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
                            placeholderText: "No file chosen"
                            onEditingFinished: if (text.trim() !== w.get(spm.fileKey)) w.setNow(spm.fileKey, text.trim())
                        }
                        QQC2.Button { text: "Choose…"; icon.name: "document-open"; onClicked: spDialog.open() }
                        FileDialog {
                            id: spDialog
                            nameFilters: ["Web pages (*.html *.htm *.xhtml)", "All files (*)"]
                            onAccepted: {
                                const path = decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, ""))
                                spFile.text = path
                                w.setNow(spm.fileKey, path)
                                // A new file replaces the copy too.
                                if (w.get(spm.copyKey)) Browser.startPageImport(spm.target, path)
                            }
                        }
                    }
                    QQC2.CheckBox {
                        id: spCopy
                        text: "Keep a copy inside Kite"
                        onToggled: {
                            // The chosen file is copied, or a starter page made.
                            if (checked && !(spFile.text && Browser.startPageImport(spm.target, spFile.text))) spFile.text = Browser.startPageCreate(spm.target)
                            if (checked && !w.get(spm.fileKey)) w.setNow(spm.fileKey, spFile.text)
                            w.setNow(spm.copyKey, checked)
                        }
                    }
                    Hint {
                        text: spCopy.checked
                            ? "Kite uses its own copy (kite:startpage/" + spm.target + "/), so the original file can move or go. Images and style sheets can go in its folder."
                            : "Kite loads the file where it is, with the images and style sheets next to it."
                    }
                    RowLayout {
                        QQC2.Button {
                            text: "Create New Page"
                            icon.name: "document-new"
                            onClicked: {
                                // A starter page kept inside Kite; it becomes the file.
                                const path = Browser.startPageCreate(spm.target)
                                if (!path) return
                                spFile.text = path
                                w.setNow(spm.fileKey, path)
                                w.setNow(spm.copyKey, true)
                                spCopy.checked = true
                            }
                            QQC2.ToolTip.text: "A simple start page with a clock, a search box and links, kept inside Kite"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        QQC2.Button {
                            text: "Edit"
                            icon.name: "document-edit"
                            enabled: w.app.startPageUrl(spm.target) !== ""
                            onClicked: spm.edit()
                            QQC2.ToolTip.text: "Open the page with the app your system uses for HTML files"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        QQC2.Button {
                            text: "Open Folder"
                            icon.name: "document-open-folder"
                            enabled: w.app.startPageUrl(spm.target) !== ""
                            onClicked: Qt.openUrlExternally("file://" + encodeURI(w.app.startPageFile(spm.target).replace(/\/[^/]*$/, "")))
                        }
                        QQC2.Button {
                            text: "Update Copy"
                            icon.name: "view-refresh"
                            visible: spCopy.checked && spFile.text !== "" && spFile.text !== Browser.startPageCopyPath(spm.target)
                            onClicked: { if (Browser.startPageImport(spm.target, spFile.text)) w.app.toast("Copied again from the file", "view-refresh") }
                            QQC2.ToolTip.text: "Copy the chosen file into Kite again (replaces the copy's edits)"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }
                    Hint { text: "Saving the file reloads the page in Kite." }
                }
            }
            }
        }
    }

    Component {
        id: general
        PageColumn {
            // Home page / new tab page: an address or a file (managed on a page of its own).
            component PageChoice: ColumnLayout {
                id: pc
                property string target        // "home" | "newtab"
                property string modeKey
                property string urlKey
                property string placeholder: ""
                readonly property bool file: w.get(modeKey) === "file"
                spacing: Kirigami.Units.smallSpacing
                RowLayout {
                    QQC2.RadioButton { text: "Address"; checked: !pc.file; onToggled: if (checked) w.set(pc.modeKey, "url") }
                    QQC2.RadioButton { text: "File"; checked: pc.file; onToggled: if (checked) w.set(pc.modeKey, "file") }
                }
                QQC2.TextField {
                    visible: !pc.file
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 18
                    text: w.get(pc.urlKey)
                    placeholderText: pc.placeholder
                    onEditingFinished: w.set(pc.urlKey, text)
                }
                RowLayout {
                    visible: pc.file
                    QQC2.Button { text: "Manage…"; icon.name: "document-edit"; onClicked: w.openStartPageFile(pc.target) }
                    QQC2.Label {
                        Layout.fillWidth: true
                        elide: Text.ElideMiddle
                        opacity: 0.7
                        readonly property string f: w.app.startPageFile(pc.target)
                        text: w.app.startPageUrl(pc.target) === "" ? "No file chosen yet" : (w.app.liveSettings["startpage." + pc.target + ".copy"] ? "Kept inside Kite" : f.replace(/^.*\//, ""))
                    }
                }
            }

            SectionHeader { text: "Startup" }
            Form {
                QQC2.ComboBox {
                    Kirigami.FormData.label: "When Kite starts:"
                    model: [{k: "restore", t: "Restore previous session"}, {k: "homepage", t: "Open home page"}, {k: "speeddial", t: "Open start page"}, {k: "blank", t: "Open blank page"}]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("general.startup")))
                    onActivated: i => w.set("general.startup", model[i].k)
                }
                PageChoice { id: homeChoice; Kirigami.FormData.label: "Home page:"; Kirigami.FormData.labelAlignment: Qt.AlignTop; target: "home"; modeKey: "general.homepageMode"; urlKey: "general.homepage" }
                QQC2.Button {
                    visible: !homeChoice.file
                    text: "Use Current Page as Home Page"
                    enabled: !!w.app.activeWindow && !!w.app.activeWindow.currentTab
                    onClicked: w.set("general.homepage", w.app.activeWindow.currentTab.url)
                }
                PageChoice { Kirigami.FormData.label: "New tab page:"; Kirigami.FormData.labelAlignment: Qt.AlignTop; target: "newtab"; modeKey: "general.newTabPageMode"; urlKey: "general.newTabPage"; placeholder: "kite:start" }
                Check { Kirigami.FormData.label: "Start page:"; text: "Show site tiles (sites you visit often and ones you add)"; key: "speeddial.showTiles" }
            }
            SectionHeader { text: "Search" }
            Form {
                QQC2.ComboBox {
                    Kirigami.FormData.label: "Default search engine:"
                    model: JSON.parse(Browser.engines()).map(e => e.name)
                    currentIndex: Math.max(0, model.indexOf(w.get("general.searchEngine")))
                    onActivated: i => w.set("general.searchEngine", model[i])
                }
                Check { Kirigami.FormData.label: "Location bar:"; text: "Search when input isn't an address"; key: "general.searchFromLocationBar" }
                Check { text: "Show search suggestions"; key: "general.showSearchSuggestions" }
            }
            SectionHeader { text: "Profile and Diagnostics" }
            Form {
                QQC2.Label { Kirigami.FormData.label: "Profile:"; text: Browser.profileName() + "  (" + Browser.webEnginePath().replace(/\/webengine$/, "") + ")"; wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.maximumWidth: Kirigami.Units.gridUnit * 24 }
                RowLayout {
                    Kirigami.FormData.label: "Settings file:"
                    QQC2.Button { text: "Export…"; icon.name: "document-export"; onClicked: settingsExport.open() }
                    QQC2.Button { text: "Import…"; icon.name: "document-import"; onClicked: settingsImport.open() }
                }
                Hint { text: "To carry your settings, search engines and site settings to another computer. Not your window size, download folder, start page files or Safe Browsing key; bookmarks and logins have exports of their own." }
                Check { Kirigami.FormData.label: "Crash reports:"; text: "Show the report after Kite crashes"; key: "general.crashReportNotice" }
                RowLayout {
                    QQC2.Button { text: "View Last Report"; icon.name: "tools-report-bug"; enabled: Browser.crashReport() !== ""; onClicked: if (w.app.activeWindow) w.app.activeWindow.showCrashReport(true) }
                    QQC2.Button { text: "Open Log Folder"; icon.name: "folder-open"; onClicked: Qt.openUrlExternally(Browser.logFolderUrl()) }
                }
                Hint { text: "Kite keeps a log of its warnings and errors, and a report of its last crash. They stay on this computer; addresses from private windows are scrambled in them." }
            }
        }
    }

    Component {
        id: appearance
        PageColumn {
            SectionHeader { text: "Window" }
            Form {
                Check { Kirigami.FormData.label: "Toolbars:"; text: "Show bookmarks toolbar"; key: "appearance.showBookmarksToolbar" }
                QQC2.CheckBox {
                    text: "Show home button"
                    readonly property var items: w.get("toolbar.items") || []
                    checked: items.includes("home")
                    onToggled: {
                        const list = items.filter(i => i !== "home")
                        if (checked) list.splice(Math.max(0, list.indexOf("urlbar")), 0, "home")
                        w.set("toolbar.items", list)
                    }
                }
                Check { text: "Show menu bar (or tap Alt to show it briefly)"; key: "appearance.showMenuBar" }
                QQC2.Button { text: "Customize Toolbar…"; icon.name: "configure-toolbars"; enabled: !!w.app.activeWindow; onClicked: w.app.activeWindow.customizeToolbar() }
                Check { Kirigami.FormData.label: "Status:"; text: "Show link targets when hovering links"; key: "appearance.showStatusBar" }
            }
            SectionHeader { text: "Web Content" }
            Form {
                QQC2.SpinBox { Kirigami.FormData.label: "Default zoom:"; from: 30; to: 300; stepSize: 10; value: w.get("appearance.defaultZoom"); onValueModified: w.set("appearance.defaultZoom", value); textFromValue: v => v + "%" }
                Check { Kirigami.FormData.label: "Colours:"; text: "Force dark mode on websites"; key: "appearance.forceDarkMode" }
                Check { Kirigami.FormData.label: "Scrollbars:"; text: "Overlay scrollbars (thin, widen on hover, hide when idle)"; key: "appearance.overlayScrollbars" }
                Hint { text: "Some changes apply to new tabs and windows; scrollbar changes after restarting Kite." }
            }
            SectionHeader { text: "Tabs" }
            Form {
                QQC2.ComboBox {
                    id: tabPos
                    Kirigami.FormData.label: "Position:"
                    model: [{k: "top", t: "Above the address bar"}, {k: "belowToolbar", t: "Below the address bar"}, {k: "bottom", t: "Below the page"}, {k: "vertical", t: "Vertical list on the left"}, {k: "verticalRight", t: "Vertical list on the right"}]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("tabs.position")))
                    onActivated: i => w.set("tabs.position", model[i].k)
                }
                Check { text: "Hide the tab bar when only one tab is open"; key: "tabs.hideWhenSingle" }
                Hint { text: "Also available by right-clicking the tab bar." }
                Check { enabled: tabPos.currentIndex < 3; text: "Stretch tabs across the whole tab bar"; key: "tabs.fillWidth" }
                Hint { text: "Tabs share the full width and get narrower as more are opened (horizontal tab bars)." }
                Check { text: "Tree-style tabs in vertical lists"; key: "tabs.tree" }
                Hint { text: "Links open as children of their tab. Drag a tab sideways to change its level; its children move with it." }
                Check { Kirigami.FormData.label: "Opening:"; text: "Open new tabs after the current tab"; key: "tabs.newTabAfterActive" }
                Check { text: "Switch to tabs opened from links"; key: "tabs.switchToNewTabFromLink" }
                Check { text: "Open popup windows in tabs"; key: "tabs.openPopupsInTabs" }
                Check { Kirigami.FormData.label: "Closing:"; text: "Close window when the last tab is closed"; key: "tabs.closeWindowWithLastTab" }
            }
        }
    }

    Component {
        id: browsing
        PageColumn {
          SectionHeader { text: "Web Content" }
          Form {
            Check { Kirigami.FormData.label: "Pages:"; text: "Enable JavaScript"; key: "web.javascript" }
            Check { text: "Load images automatically"; key: "web.autoloadImages" }
            Check { text: "Allow local storage"; key: "web.localStorage" }
            Check { text: "Let pages lock the mouse pointer when you click in them (games)"; key: "web.pointerLock" }
            Check { text: "Don't let pages block the right-click menu"; key: "web.unblockContextMenu" }
            Hint { text: "When a page stops the menu without showing one of its own, Kite opens its menu anyway, and finds pictures under see-through covers. Shift+right-click always opens Kite's menu." }
            Check { text: "Don't let pages block selecting and copying text"; key: "web.unblockSelection" }
            Hint { text: "Dragging across a page's text or double-clicking a word selects it, and copying it copies what you selected, even where the page tries to stop that or to change what's copied. Fields, editors and apps' own controls are left as they are." }
            // Where a page's file picker opens (upload buttons; Main.fileFolder).
            QQC2.ComboBox {
                id: filePicker
                Kirigami.FormData.label: "File picker:"
                model: [{k: "home", t: "Home directory"}, {k: "last", t: "Last directory used"}, {k: "custom", t: "Custom"}]
                textRole: "t"
                currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("browsing.fileFolder")))
                onActivated: i => { w.set("browsing.fileFolder", model[i].k); if (model[i].k === "custom" && !w.get("browsing.fileFolderPath")) pickerFolderDialog.open() }
            }
            RowLayout {
                visible: filePicker.currentIndex === 2
                QQC2.TextField {
                    id: pickerFolder
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 18
                    placeholderText: "Choose a folder"
                    text: w.get("browsing.fileFolderPath")
                    onEditingFinished: w.set("browsing.fileFolderPath", text.trim())
                }
                QQC2.Button {
                    icon.name: "document-open-folder"
                    text: "Choose Folder…"
                    display: QQC2.AbstractButton.IconOnly
                    onClicked: pickerFolderDialog.open()
                    QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                FolderDialog {
                    id: pickerFolderDialog
                    onAccepted: { pickerFolder.text = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, "")); w.set("browsing.fileFolderPath", pickerFolder.text) }
                }
            }
            Hint { text: filePicker.currentIndex === 1 ? "Where a page's file picker, as for uploading a file, opens: the folder last used in one, until Kite closes. Private windows don't change it." : "Where a page's file picker, as for uploading a file, opens." }
            Check { Kirigami.FormData.label: "Printing:"; text: "Print backgrounds"; key: "web.printBackgrounds" }
            // (Its lists and rules are on the Content Blocking page, listed while this is on.)
            Check { Kirigami.FormData.label: "Content blocking:"; text: "Block ads and trackers"; key: "adblock.enabled" }
            UserAgentField {
                Kirigami.FormData.label: "User agent:"
                firstOption: "Kite's default (reads like Chrome)"
                Component.onCompleted: load(w.get("web.userAgent"))
                onEdited: w.set("web.userAgent", value)
            }
            Hint { text: "What Kite tells sites it is. Kite's default leaves out “QtWebEngine”, which few browsers send and so would make you easier to recognise; some sites need it to work, though. Sites can also have one of their own, in Site Settings." }
          }
          SectionHeader { text: "Scrolling" }
          Form {
            Check { Kirigami.FormData.label: "Movement:"; text: "Smooth scrolling"; key: "web.smoothScrolling" }
            Check { text: "Spatial navigation (arrow keys between links)"; key: "web.spatialNavigation" }
            // ---- "web.wheelScrollPixels" (bootstrap.cpp scales wheel notches) ----
            QQC2.CheckBox {
                id: wheelSystem
                Kirigami.FormData.label: "Mouse wheel:"
                readonly property int systemPx: Browser.systemWheelPixels()
                text: "Scroll like other apps (" + systemPx + " px per notch)"
                checked: w.get("web.wheelScrollPixels") <= 0
                onToggled: w.set("web.wheelScrollPixels", checked ? 0 : wheelPx.value)
            }
            RowLayout {
                enabled: !wheelSystem.checked
                QQC2.SpinBox {
                    id: wheelPx
                    from: 10; to: 1000; stepSize: 10; editable: true
                    value: w.get("web.wheelScrollPixels") > 0 ? w.get("web.wheelScrollPixels") : wheelSystem.systemPx
                    onValueModified: w.set("web.wheelScrollPixels", value)
                }
                QQC2.Label { text: "px per wheel notch" }
            }
            Hint { text: "Scroll distance on web pages. Touchpads and Ctrl+wheel (zoom) are not affected." }
          }
          SectionHeader { text: "Spelling" }
          Form {
            Check { id: spellOn; Kirigami.FormData.label: "Spelling:"; text: "Check spelling while typing"; key: "web.spellcheck" }
            // One per installed dictionary (a language without one would do nothing).
            Flow {
                id: spellLangs
                Kirigami.FormData.label: "Languages:"
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                spacing: Kirigami.Units.largeSpacing
                visible: available.length > 0
                enabled: spellOn.checked
                readonly property var available: JSON.parse(Browser.spellDictionaries())
                // What's chosen, as the dictionaries are named ("en-US" finds en_US).
                readonly property var chosen: JSON.parse(Browser.resolveSpellLanguages(JSON.stringify(w.get("web.spellcheckLanguages"))))
                Repeater {
                    model: spellLangs.available
                    delegate: QQC2.CheckBox {
                        required property string modelData
                        text: modelData
                        checked: spellLangs.chosen.includes(modelData)
                        onToggled: w.set("web.spellcheckLanguages", checked ? spellLangs.chosen.concat([modelData]) : spellLangs.chosen.filter(l => l !== modelData))
                    }
                }
            }
            Hint {
                text: spellLangs.available.length > 0
                    ? "The languages of the dictionaries installed on this computer. More come with your distribution's QtWebEngine dictionary packages."
                    : "No spelling dictionaries were found, so nothing is checked. Your distribution's QtWebEngine dictionary packages add them."
            }

            // ---- memory ("tabs.unloadInactive", WebTab.unload()) ----
          }
          SectionHeader { text: "Memory" }
          Form {
            Check { id: unloadOn; Kirigami.FormData.label: "Inactive tabs:"; text: "Unload tabs that haven't been shown for a while"; key: "tabs.unloadInactive" }
            RowLayout {
                enabled: unloadOn.checked
                QQC2.Label { text: "Unload after" }
                QQC2.SpinBox { from: 1; to: 1440; editable: true; value: w.get("tabs.unloadAfterMinutes"); onValueModified: w.set("tabs.unloadAfterMinutes", value) }
                QQC2.Label { text: "minutes" }
            }
            Check { enabled: unloadOn.checked; text: "Keep pinned tabs loaded"; key: "tabs.unloadKeepPinned" }
            Hint {
                text: "Unloaded tabs keep their place, title and history, and reload when you switch to them (shown faded in the tab bar). Tabs playing sound, keeping the screen on, loading, with text typed into a form or with developer tools open stay loaded. Any tab can also be unloaded from its context menu."
            }

            // ---- form autofill ("browsing.formFill", formfill.rs) ----
          }
          SectionHeader { text: "Forms" }
          Form {
            Check { id: ffOn; Kirigami.FormData.label: "Autofill:"; text: "Fill in forms with saved addresses"; key: "browsing.formFill"; onToggled: if (checked && FormFill.count > 0) FormFill.unlock() }
            Check { enabled: ffOn.checked; text: "Offer to save addresses typed into forms"; key: "browsing.formFillSave" }
            Hint {
                text: "Suggestions appear when you click into an address, name, email or phone field; nothing is filled in until you pick one. Saved entries are encrypted"
                    + (FormFill.keyStore === "kwallet" ? ", with the key kept in KDE Wallet."
                       : FormFill.keyStore === "file" ? ", with the key kept in a file in your Kite profile (KDE Wallet was not available), which protects them when the database alone is copied."
                       : "; the key goes into KDE Wallet when it is available.")
            }
            ColumnLayout {
                id: ff
                Kirigami.FormData.label: "Saved:"
                Kirigami.FormData.labelAlignment: Qt.AlignTop
                Layout.fillWidth: true
                property var entries: []
                function refresh() { entries = FormFill.unlocked ? JSON.parse(FormFill.list()) : [] }
                Component.onCompleted: { if (FormFill.count > 0 && !FormFill.unlocked) FormFill.unlock(); refresh() }
                Connections { target: FormFill; function onChanged() { ff.refresh() } function onUnlockedChanged() { ff.refresh() } }
                QQC2.Label {
                    visible: text !== ""
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                    wrapMode: Text.Wrap
                    text: FormFill.unlocking ? "Opening KDE Wallet…"
                        : FormFill.error !== "" ? "Saved entries can't be read: " + FormFill.error + "."
                        : !FormFill.unlocked && FormFill.count > 0 ? FormFill.count + " saved, locked."
                        : FormFill.count === 0 ? "No saved addresses." : ""
                    opacity: 0.8
                }
                RowLayout {
                    visible: FormFill.error !== "" || (!FormFill.unlocked && FormFill.count > 0 && !FormFill.unlocking)
                    QQC2.Button { text: "Try Again"; icon.name: "view-refresh"; onClicked: FormFill.unlock() }
                    QQC2.Button { visible: FormFill.error !== ""; text: "Remove Saved Entries"; icon.name: "edit-delete"; onClicked: ffRemoveAll.open() }
                }
                Repeater {
                    model: ff.entries
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        Kirigami.Icon { source: "user-identity"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth }
                        QQC2.Label { text: modelData.label; Layout.fillWidth: true; Layout.maximumWidth: Kirigami.Units.gridUnit * 18; elide: Text.ElideRight }
                        QQC2.ToolButton { icon.name: "document-edit"; onClicked: { ffEdit.entry = modelData; ffEdit.open() } QQC2.ToolTip.text: "Edit"; QQC2.ToolTip.visible: hovered }
                        QQC2.ToolButton { icon.name: "edit-delete"; onClicked: FormFill.remove(modelData.id); QQC2.ToolTip.text: "Remove"; QQC2.ToolTip.visible: hovered }
                    }
                }
                RowLayout {
                    QQC2.Button { text: "Add Address…"; icon.name: "list-add"; enabled: FormFill.error === "" && !FormFill.unlocking; onClicked: { ffEdit.entry = null; ffEdit.open() } }
                    QQC2.Button { visible: ff.entries.length > 1; text: "Remove All…"; icon.name: "edit-delete"; onClicked: ffRemoveAll.open() }
                }
                Kirigami.PromptDialog {
                    id: ffRemoveAll
                    parent: QQC2.Overlay.overlay; anchors.centerIn: parent
                    title: "Remove All Saved Addresses?"
                    subtitle: "This cannot be undone."
                    standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                    onAccepted: FormFill.removeAll()
                }
                Kirigami.Dialog {
                    id: ffEdit
                    parent: QQC2.Overlay.overlay; anchors.centerIn: parent
                    property var entry: null
                    readonly property var keys: ["givenName", "familyName", "organization", "street", "city", "region", "postalCode", "country", "email", "tel"]
                    title: entry ? "Edit Address" : "Add Address"
                    standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
                    padding: Kirigami.Units.largeSpacing
                    onOpened: { for (const k of keys) ffFields[k].text = entry ? (entry[k] || "") : ""; ffFields.givenName.forceActiveFocus() }
                    property var ffFields: ({givenName: fGiven, familyName: fFamily, organization: fOrg, street: fStreet, city: fCity, region: fRegion, postalCode: fPostal, country: fCountry, email: fEmail, tel: fTel})
                    Kirigami.FormLayout {
                        QQC2.TextField { id: fGiven; Kirigami.FormData.label: "First name:"; Layout.preferredWidth: Kirigami.Units.gridUnit * 18 }
                        QQC2.TextField { id: fFamily; Kirigami.FormData.label: "Last name:" }
                        QQC2.TextField { id: fOrg; Kirigami.FormData.label: "Organization:" }
                        QQC2.TextArea { id: fStreet; Kirigami.FormData.label: "Street address:"; Layout.preferredWidth: Kirigami.Units.gridUnit * 18; Layout.preferredHeight: Kirigami.Units.gridUnit * 3.5; placeholderText: "One line per address line"; wrapMode: TextEdit.Wrap }
                        QQC2.TextField { id: fCity; Kirigami.FormData.label: "City:" }
                        QQC2.TextField { id: fRegion; Kirigami.FormData.label: "State / region:" }
                        QQC2.TextField { id: fPostal; Kirigami.FormData.label: "Postal code:" }
                        QQC2.TextField { id: fCountry; Kirigami.FormData.label: "Country:" }
                        QQC2.TextField { id: fEmail; Kirigami.FormData.label: "Email:"; inputMethodHints: Qt.ImhEmailCharactersOnly }
                        QQC2.TextField { id: fTel; Kirigami.FormData.label: "Phone:"; inputMethodHints: Qt.ImhDialableCharactersOnly; placeholderText: "+1 555 0100" }
                    }
                    onAccepted: {
                        const d = {}
                        for (const k of keys) d[k] = ffFields[k].text.trim()
                        if (entry) d.id = entry.id
                        const err = FormFill.save(JSON.stringify(d))
                        if (err) w.app.toast(err, "dialog-error")
                    }
                }
            }

            // ---- dangerous sites ("browsing.safeBrowsing", safebrowsing.rs) ----
          }
          SectionHeader { text: "Dangerous Sites" }
          Form {
            QQC2.ComboBox {
                id: sbMode
                Kirigami.FormData.label: "Protection:"
                model: [{k: "off", t: "Disabled"}, {k: "google", t: "Use Google Safe Browsing"}, {k: "open", t: "Use open lists"}]
                textRole: "t"
                currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("browsing.safeBrowsing")))
                onActivated: i => { w.set("browsing.safeBrowsing", model[i].k); sbStatus.refresh() }
            }
            Hint {
                text: [
                    "Known phishing and malware sites load like any other. No extra network traffic.",
                    "Pages and downloads are checked against Google's lists of dangerous sites, with a warning before they open.\n\nTraffic: Kite downloads Google's lists of address fingerprints (shortened hashes) about every 30 minutes, a few megabytes at first, then small updates, and compares addresses on your computer. Only when an address matches a fingerprint, which is rare, does Kite ask Google about that fingerprint to confirm.\n\nPrivacy: Google never receives the addresses you visit, but sees your IP address with each update and confirmation, and a confirmation tells it you may be visiting one of the few sites sharing that fingerprint. The API key ties these requests to your Google account.",
                    "Pages and downloads are checked against two public lists of phishing and malware sites (malware-filter on GitLab, compiled from PhishTank, OpenPhish, URLhaus and others), with a warning before they open.\n\nTraffic: Kite downloads the lists once a day, about 15 MB, from GitLab's servers.\n\nPrivacy: all checks happen on your computer; nothing about the sites you visit is sent. GitLab sees your IP address once a day. The lists are smaller and slower to catch new sites than Google's."
                ][sbMode.currentIndex]
            }
            RowLayout {
                visible: sbMode.currentIndex === 1
                Kirigami.FormData.label: "API key:"
                QQC2.TextField {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 18
                    placeholderText: "Google Cloud API key"
                    echoMode: TextInput.PasswordEchoOnEdit
                    text: w.get("browsing.safeBrowsingKey")
                    onEditingFinished: { w.set("browsing.safeBrowsingKey", text.trim()); sbStatus.refresh() }
                }
                QQC2.Button {
                    text: "Get a Key…"
                    icon.name: "internet-services"
                    onClicked: Qt.openUrlExternally("https://developers.google.com/safe-browsing/v4/get-started")
                    QQC2.ToolTip.text: "Create a free key for the Safe Browsing API in the Google Cloud console (free for non-commercial use)"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
            QQC2.Label {
                id: sbStatus
                visible: sbMode.currentIndex > 0 && text !== ""
                Kirigami.FormData.label: "Lists:"
                opacity: 0.8
                function refresh() { text = Browser.safeBrowsingStatus() }
                Component.onCompleted: refresh()
                // (Updates run in the background.)
                Timer { interval: 2000; repeat: true; running: sbStatus.visible || sbMode.currentIndex > 0; onTriggered: sbStatus.refresh() }
            }
            // Only with `kite --debug`: what the lists hold, and how an address is checked.
            QQC2.Button {
                Kirigami.FormData.label: "Debugging:"
                visible: Browser.debugMode() && sbMode.currentIndex > 0
                text: "Browse Lists…"
                icon.name: "tools-report-bug"
                onClicked: w.pageStack.push(safeBrowsingListsPage)
                QQC2.ToolTip.text: "The entries checked against, as stored now (of the mode as applied)"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
          }
        }
    }

    Component {
        id: search
        PageColumn {
            id: se
            property var engines: JSON.parse(Browser.engines())
            // (Saved with Apply.)
            function edited(list) {
                engines = list
                w.stage("engines", () => Browser.saveEngines(JSON.stringify(list)))
            }
            function change(index, field, value) {
                if (engines[index][field] === value) return
                const list = engines.slice()
                list[index] = Object.assign({}, list[index], {[field]: value})
                edited(list)
            }
            Hint {
                Layout.maximumWidth: -1
                text: "Type a shortcut followed by a space and your search in the location bar, e.g. “w KDE”. Use %s where the search terms go."
            }
            Repeater {
                model: se.engines
                delegate: RowLayout {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    QQC2.TextField { text: modelData.name; Layout.preferredWidth: Kirigami.Units.gridUnit * 8; onEditingFinished: se.change(index, "name", text) }
                    QQC2.TextField { text: modelData.url; Layout.fillWidth: true; onEditingFinished: se.change(index, "url", text) }
                    QQC2.TextField { text: modelData.shortcut; Layout.preferredWidth: Kirigami.Units.gridUnit * 3; placeholderText: "key"; onEditingFinished: se.change(index, "shortcut", text) }
                    QQC2.ToolButton {
                        icon.name: "edit-delete"
                        text: "Remove"
                        display: QQC2.AbstractButton.IconOnly
                        onClicked: se.edited(se.engines.filter((_, i) => i !== index))
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
            }
            QQC2.Button { text: "Add Search Engine"; icon.name: "list-add"; onClicked: se.edited(se.engines.concat([{name: "New engine", url: "https://example.com/search?q=%s", shortcut: "", suggestions_url: "", post_data: ""}])) }
        }
    }

    Component {
        id: privacy
        PageColumn {
            SectionHeader { text: "History" }
            Form {
                Check { Kirigami.FormData.label: "History:"; text: "Remember browsing history"; key: "privacy.saveHistory" }
                Check { text: "Clear history, cookies and cache when Kite closes"; key: "privacy.clearOnExit" }
            }
            SectionHeader { text: "Private Windows" }
            Form {
                Check { Kirigami.FormData.label: "From your browsing:"; text: "Show often visited sites on the start page"; key: "privacy.privateHistoryTiles" }
                Check { text: "Suggest pages from the history in the address bar"; key: "privacy.privateHistorySuggestions" }
                Check { text: "Show notifications from pages and downloads"; key: "privacy.privateNotifications" }
                Hint { text: "These show pages from your regular browsing in private windows, where others might see them. What you added yourself, tiles and bookmarks, always shows." }
            }
            SectionHeader { text: "Cookies" }
            Form {
                QQC2.ComboBox {
                    Kirigami.FormData.label: "Cookies:"
                    model: [{k: "all", t: "Allow all cookies"}, {k: "noThirdParty", t: "Block third-party cookies"}, {k: "none", t: "Block all cookies (many sites won't work)"}]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("privacy.cookies")))
                    onActivated: i => w.set("privacy.cookies", model[i].k)
                }
                Check { id: siteCookies; text: "Use per-site cookie rules"; key: "privacy.siteCookieRules" }
                Hint { text: "Allow, block, or keep until Kite closes the cookies of single sites, in Site Settings." }
                QQC2.Button { text: "Manage Cookies…"; icon.name: "preferences-web-browser-cookies"; onClicked: w.pageStack.push(cookiesPage) }
            }
            SectionHeader { text: "Tracking" }
            Form {
                Check { Kirigami.FormData.label: "Ask sites:"; text: "Send Global Privacy Control"; key: "privacy.globalPrivacyControl" }
                Hint { text: "Asks sites not to sell or share your data. Legally binding in some places, such as California." }
                Check { text: "Send “Do Not Track”"; key: "web.doNotTrack" }
                Hint { text: "Few sites honour it, and as few people send it, it makes you easier to recognise." }
                Check { Kirigami.FormData.label: "Links:"; text: "Remove tracking parameters from addresses"; key: "privacy.stripTrackingParams" }
                Hint { text: "Click and campaign identifiers such as fbclid, gclid and utm_… are removed before a page loads." }
                Check { text: "Block link click pings"; key: "privacy.blockPings" }
                Check { text: "Tell sites which page linked to them (referrer)"; key: "privacy.sendReferer" }
                Check { text: "Let sites ask to open links like mailto: and magnet:"; key: "privacy.protocolHandlers" }
            }
            SectionHeader { text: "Fingerprinting" }
            Form {
                Check { Kirigami.FormData.label: "Identity:"; text: "Appear as Firefox when signing in to Google"; key: "web.googleSignIn" }
                Hint { text: "Google refuses to sign you in with browsers it doesn't know. This only applies on accounts.google.com, and only tells that site Kite is Firefox. What Kite tells other sites it is, the user agent, is chosen in Browsing settings." }
                Check { text: "Hide local network addresses from WebRTC"; key: "privacy.webrtcPublicOnly" }
                Check { text: "Allow fingerprint protection"; key: "privacy.antiFingerprinting" }
                Hint { text: "Turn it on for single sites in Site Settings: canvas, WebGL and audio readouts are blurred and the hardware reported is generic. Some sites may misbehave." }
            }
            SectionHeader { text: "Connections" }
            Form {
                Check { Kirigami.FormData.label: "Encryption:"; text: "HTTPS-only mode"; key: "privacy.httpsOnly" }
                Hint { text: "Pages load over a secure connection. Sites without one show a warning first. Local addresses are left alone." }
                QQC2.ComboBox {
                    id: doh
                    Kirigami.FormData.label: "Secure DNS:"
                    model: [{k: "off", t: "Off (system DNS)"}, {k: "auto", t: "On, with system DNS as fallback"}, {k: "secure", t: "On, secure DNS only"}]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("privacy.dnsOverHttps")))
                    onActivated: i => w.set("privacy.dnsOverHttps", model[i].k)
                }
                QQC2.ComboBox {
                    Kirigami.FormData.label: "DNS server:"
                    enabled: doh.currentIndex > 0
                    editable: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 20
                    model: ["https://cloudflare-dns.com/dns-query", "https://dns.quad9.net/dns-query", "https://dns.mullvad.net/dns-query", "https://dns.google/dns-query"]
                    Component.onCompleted: editText = w.get("privacy.dnsServer")
                    onAccepted: w.set("privacy.dnsServer", editText.trim())
                    onActivated: i => w.set("privacy.dnsServer", model[i])
                }
                Check { text: "Look up the addresses of links in advance (DNS prefetching)"; key: "web.dnsPrefetch" }
                QQC2.Button { Kirigami.FormData.label: "Certificates:"; text: "Manage Certificate Exceptions…"; icon.name: "security-medium"; onClicked: w.pageStack.push(certificatesPage) }
            }
            SectionHeader { text: "Proxy" }
            Form {
                QQC2.ComboBox {
                    id: proxyMode
                    Kirigami.FormData.label: "Proxy:"
                    model: [{k: "system", t: "Use system settings"}, {k: "none", t: "No proxy"}, {k: "manual", t: "Manual"}]
                    textRole: "t"
                    currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("privacy.proxy")))
                    onActivated: i => w.set("privacy.proxy", model[i].k)
                }
                RowLayout {
                    Kirigami.FormData.label: "Server:"
                    enabled: proxyMode.currentIndex === 2
                    QQC2.ComboBox {
                        model: [{k: "socks5", t: "SOCKS5"}, {k: "http", t: "HTTP"}]
                        textRole: "t"
                        currentIndex: w.get("privacy.proxyType") === "http" ? 1 : 0
                        onActivated: i => w.set("privacy.proxyType", model[i].k)
                    }
                    QQC2.TextField { placeholderText: "host"; text: w.get("privacy.proxyHost"); Layout.preferredWidth: Kirigami.Units.gridUnit * 11; onEditingFinished: w.set("privacy.proxyHost", text.trim()) }
                    QQC2.SpinBox { from: 1; to: 65535; editable: true; value: w.get("privacy.proxyPort"); textFromValue: v => String(v); valueFromText: t => parseInt(t) || 1; onValueModified: w.set("privacy.proxyPort", value) }
                }
                Hint { text: "Proxy changes apply after restarting Kite. With SOCKS5 (e.g. Tor on port 9050), host names are looked up through the proxy too." }
            }
            SectionHeader { text: "Container Tabs" }
            Form {
                Check { id: containersOn; Kirigami.FormData.label: "Containers:"; text: "Use container tabs"; key: "privacy.containers" }
                Hint { text: "Each container keeps its own cookies, site data and logins: sites in one container can't see what you do in another. Open them from the menu, the tab bar or a link's context menu." }
                // ---- containers ------------------------------------------------
                ColumnLayout {
                    id: containerEditor
                    visible: containersOn.checked
                    property var list: w.get("privacy.containerList")
                    readonly property var swatches: ["#3daee9", "#f67400", "#27ae60", "#da4453", "#9b59b6", "#fdbc4b", "#1abc9c", "#7f8c8d"]
                    function save(l) { list = l; w.set("privacy.containerList", l) }
                    Repeater {
                        model: containerEditor.list
                        delegate: RowLayout {
                            required property var modelData
                            required property int index
                            QQC2.ToolButton {
                                text: "Colour"
                                display: QQC2.AbstractButton.IconOnly
                                contentItem: Rectangle { implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: implicitWidth; radius: width / 2; color: modelData.color }
                                onClicked: {
                                    const p = containerEditor.swatches
                                    const l = containerEditor.list.slice()
                                    l[index] = Object.assign({}, modelData, {color: p[(p.indexOf(modelData.color) + 1) % p.length]})
                                    containerEditor.save(l)
                                }
                                QQC2.ToolTip.text: "Change colour"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            }
                            QQC2.TextField {
                                text: modelData.name
                                Layout.preferredWidth: Kirigami.Units.gridUnit * 14
                                onEditingFinished: {
                                    if (text.trim() === "" || text.trim() === modelData.name) return
                                    const l = containerEditor.list.slice()
                                    l[index] = Object.assign({}, modelData, {name: text.trim()})
                                    containerEditor.save(l)
                                }
                            }
                            QQC2.ToolButton {
                                icon.name: "edit-delete"
                                text: "Remove container"
                                display: QQC2.AbstractButton.IconOnly
                                onClicked: containerEditor.save(containerEditor.list.filter((_, i) => i !== index))
                                QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            }
                        }
                    }
                    QQC2.Button {
                        text: "Add Container"
                        icon.name: "list-add"
                        onClicked: {
                            const l = containerEditor.list.slice()
                            const id = "c" + Date.now().toString(36)
                            l.push({id: id, name: "Container " + (l.length + 1), color: containerEditor.swatches[l.length % containerEditor.swatches.length]})
                            containerEditor.save(l)
                        }
                    }
                    Hint { text: "Removing a container closes nothing: its open tabs act as normal tabs after Kite restarts." }
                }
            }
            SectionHeader { text: "Clear Recent History" }
            Form {
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
        PageColumn {
            id: pw
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

            SectionHeader { text: "Saving and Filling" }
            Form {
                Check { Kirigami.FormData.label: "Logins:"; text: "Offer to save passwords"; key: "passwords.save" }
                Check { text: "Fill logins automatically over https"; key: "passwords.autofill" }
                Hint { text: "On plain http pages, and with this off, a login is filled when you pick it from the password icon in the location bar." }
            }
            SectionHeader { text: "Generator" }
            Form {
                Check { id: genOn; Kirigami.FormData.label: "Sign-up forms:"; text: "Suggest strong passwords"; key: "passwords.generator" }
                RowLayout {
                    Kirigami.FormData.label: "Length:"
                    enabled: genOn.checked
                    QQC2.SpinBox { from: 8; to: 64; value: w.get("passwords.generatorLength"); onValueModified: w.set("passwords.generatorLength", value) }
                    QQC2.Label { text: "characters" }
                }
                Check { enabled: genOn.checked; text: "Include symbols"; key: "passwords.generatorSymbols" }
            }
            SectionHeader { text: "Storage" }
            Form {
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
                Hint {
                    visible: Passwords.backend === "kwallet" || pw.switching || Passwords.walletOpening
                    text: Passwords.walletOpening ? "Opening KDE Wallet…" : pw.switching ? "Moving saved logins…"
                        : "Logins are kept in your network wallet, in the same folder Falkon uses, so both browsers share them."
                }
                RowLayout {
                    visible: Passwords.backend === "encrypted"
                    QQC2.Button { text: "Change Master Password…"; icon.name: "document-encrypt"; onClicked: { masterDlg.mode = "change"; masterDlg.open() } }
                    QQC2.Button { visible: !Passwords.locked; text: "Lock Now"; icon.name: "object-locked"; onClicked: Passwords.lock() }
                }
            }

            SectionHeader { text: "Saved Logins" }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: Passwords.locked
                text: Passwords.backend === "kwallet" ? (Passwords.walletOpening ? "Opening KDE Wallet…" : "KDE Wallet is closed.") : "Saved logins are protected by your master password."
                actions: [Kirigami.Action { text: "Unlock"; icon.name: "object-unlocked"; onTriggered: w.app.unlockPasswords(ok => pw.reload()) }]
            }
            RowLayout {
                visible: !Passwords.locked
                // (It may shrink: its own minimum would widen the page's column.)
                Kirigami.SearchField { id: search; Layout.fillWidth: true; Layout.minimumWidth: Kirigami.Units.gridUnit * 6 }
                QQC2.Button { text: "Add"; icon.name: "list-add"; onClicked: { editDlg.entry = null; editDlg.open() } }
                QQC2.Button { text: "Import…"; icon.name: "document-import"; onClicked: importDlg.open() }
                QQC2.Button { text: "Export…"; icon.name: "document-export"; enabled: pw.entries.length > 0; onClicked: exportWarn.open() }
            }
            QQC2.Label { visible: !Passwords.locked && pw.entries.length === 0; text: "No saved logins."; opacity: 0.7 }
            // (Rows in a column of their own: back to back, as a list.)
            ColumnLayout {
              Layout.fillWidth: true
              spacing: 0
              Repeater {
                model: pw.shown
                delegate: ListRow {
                    required property var modelData
                    RowLayout {
                        QQC2.Label { text: modelData.host; Layout.preferredWidth: Kirigami.Units.gridUnit * 11; elide: Text.ElideMiddle; font.bold: true }
                        QQC2.Label { text: modelData.username || "(no username)"; Layout.preferredWidth: Kirigami.Units.gridUnit * 9; elide: Text.ElideRight }
                        QQC2.Label { text: pw.revealed[modelData.id] ? modelData.password : "••••••••"; Layout.fillWidth: true; elide: Text.ElideRight; font.family: "monospace" }
                        RowButton { icon.name: pw.revealed[modelData.id] ? "password-show-off" : "password-show-on"; text: pw.revealed[modelData.id] ? "Hide password" : "Show password"; onClicked: { const r = Object.assign({}, pw.revealed); r[modelData.id] = !r[modelData.id]; pw.revealed = r } }
                        RowButton { icon.name: "edit-copy"; text: "Copy password"; onClicked: { clip.text = modelData.password; clip.selectAll(); clip.copy(); clip.text = ""; w.app.toast("Password copied", "edit-copy") } }
                        RowButton { icon.name: "document-edit"; text: "Edit login"; onClicked: { editDlg.entry = modelData; editDlg.open() } }
                        RowButton { icon.name: "edit-delete"; text: "Remove login"; onClicked: Passwords.remove(modelData.id) }
                    }
                }
              }
            }
            TextEdit { id: clip; visible: false }
            QQC2.Button { visible: pw.entries.length > 0; text: "Remove All Logins…"; icon.name: "edit-delete"; onClicked: removeAllDlg.open() }

            SectionHeader { text: "Never Saved For" }
            QQC2.Label { visible: pw.exceptions.length === 0; text: "No exceptions."; opacity: 0.7 }
            ColumnLayout {
              Layout.fillWidth: true
              spacing: 0
              Repeater {
                model: pw.exceptions
                delegate: ListRow {
                    required property string modelData
                    RowLayout {
                        QQC2.Label { text: modelData; Layout.fillWidth: true; elide: Text.ElideMiddle }
                        RowButton { icon.name: "edit-delete"; text: "Remove exception (logins can be saved there again)"; onClicked: Passwords.removeException(modelData) }
                    }
                }
              }
            }
            QQC2.Button { visible: pw.exceptions.length > 0; text: "Remove All Exceptions…"; icon.name: "edit-delete"; onClicked: removeAllExceptionsDlg.open() }
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
                id: removeAllExceptionsDlg
                title: "Remove All Exceptions?"
                subtitle: "Kite will offer to save logins on these sites again."
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                onAccepted: { for (const host of pw.exceptions.slice()) Passwords.removeException(host) }
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
                    RowLayout {
                        Kirigami.FormData.label: "Password:"
                        Kirigami.PasswordField { id: ePass; Layout.fillWidth: true }
                        QQC2.ToolButton {
                            icon.name: "password-generate"
                            onClicked: { ePass.text = FormFill.generatePassword(); ePass.showPassword = true }
                            QQC2.ToolTip.text: "Generate a strong password"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }
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
        PageColumn {
            id: ab
            readonly property var subs: w.get("adblock.subscriptions")
            function setSubs(list) { w.set("adblock.subscriptions", list) }
            property string status: ""
            Connections { target: Browser; function onAdblockStatus(m) { ab.status = m } }
            readonly property string extBlocker: Browser.contentBlocker(false)
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: ab.extBlocker !== ""
                type: Kirigami.MessageType.Information
                text: ab.extBlocker + " is blocking content, so Kite's built-in blocker is paused. These settings apply again when it is disabled or removed."
            }
            SectionHeader { text: "Filter Subscriptions" }
            RowLayout {
                Layout.fillWidth: true
                QQC2.TextField { id: subTitle; placeholderText: "Title"; Layout.preferredWidth: Kirigami.Units.gridUnit * 8 }
                QQC2.TextField { id: subUrl; placeholderText: "https://… list URL"; Layout.fillWidth: true }
                QQC2.Button { text: "Add"; icon.name: "list-add"; enabled: subUrl.text.startsWith("http"); onClicked: { ab.setSubs(ab.subs.concat([{title: subTitle.text || subUrl.text, url: subUrl.text, enabled: true}])); subUrl.text = ""; subTitle.text = "" } }
            }
            QQC2.Menu {
                id: presets
                QQC2.MenuItem { text: "uBlock filters"; onTriggered: { subTitle.text = text; subUrl.text = "https://ublockorigin.github.io/uAssets/filters/filters.min.txt" } }
                QQC2.MenuItem { text: "Peter Lowe's list"; onTriggered: { subTitle.text = text; subUrl.text = "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&mimetype=plaintext" } }
                QQC2.MenuItem { text: "Fanboy's Annoyances"; onTriggered: { subTitle.text = text; subUrl.text = "https://easylist.to/easylist/fanboy-annoyance.txt" } }
                QQC2.MenuItem { text: "EasyList Germany"; onTriggered: { subTitle.text = text; subUrl.text = "https://easylist.to/easylistgermany/easylistgermany.txt" } }
            }
            RowLayout {
                QQC2.Button { text: "Suggested Lists…"; icon.name: "view-list-details"; onClicked: presets.popup() }
                QQC2.Button {
                    text: "Update Lists Now"
                    icon.name: "view-refresh"
                    onClicked: { Browser.reloadAdblock(true); ab.status = "Downloading lists…" }
                    QQC2.ToolTip.text: "Download the lists again now (of the subscriptions as applied)"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
                QQC2.Label { text: ab.status; opacity: 0.7; Layout.fillWidth: true; elide: Text.ElideRight }
            }
            // (Rows back to back, as a list.)
            ColumnLayout {
              Layout.fillWidth: true
              spacing: 0
              Repeater {
                model: ab.subs
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    RowLayout {
                        QQC2.CheckBox { Accessible.name: modelData.title; checked: modelData.enabled; onToggled: { const s = ab.subs.slice(); s[index] = Object.assign({}, s[index], {enabled: checked}); ab.setSubs(s) } }
                        ColumnLayout { spacing: 0; Layout.fillWidth: true
                            QQC2.Label { text: modelData.title; Layout.fillWidth: true; elide: Text.ElideRight }
                            QQC2.Label { text: modelData.url; font: Kirigami.Theme.smallFont; opacity: 0.7; elide: Text.ElideMiddle; Layout.fillWidth: true }
                        }
                        RowButton {
                            icon.name: "edit-delete"
                            text: "Remove " + modelData.title
                            onClicked: ab.setSubs(ab.subs.filter((_, i) => i !== index))
                        }
                    }
                }
              }
            }

            SectionHeader { text: "Custom Rules" }
            QQC2.TextArea {
                id: rules
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 8
                font.family: "monospace"
                placeholderText: "||ads.example.com^\nexample.org##.banner"
                // (Not bound: setting it again while typing would move the cursor.)
                Component.onCompleted: text = w.get("adblock.customRules")
                onTextChanged: if (text !== w.get("adblock.customRules")) w.set("adblock.customRules", text)
                // (A rule added from a page's Block Element while this is open.)
                Connections { target: w.app; function onCustomFiltersChanged() { rules.text = w.get("adblock.customRules") } }
            }
            Hint { Layout.maximumWidth: -1; text: "Adblock Plus syntax, one rule per line. The filters are rebuilt when you apply." }

            SectionHeader { text: "Disabled on These Sites" }
            QQC2.Label { text: w.get("adblock.allowlist").join(", ") || "None"; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }

    Component {
        id: downloads
        PageColumn {
            SectionHeader { text: "Saving" }
            Form {
                RowLayout {
                    Kirigami.FormData.label: "Save files to:"
                    QQC2.TextField { id: dir; text: w.get("downloads.directory"); Layout.preferredWidth: Kirigami.Units.gridUnit * 18; onEditingFinished: w.set("downloads.directory", text) }
                    QQC2.Button {
                        icon.name: "document-open-folder"
                        text: "Choose Folder…"
                        display: QQC2.AbstractButton.IconOnly
                        onClicked: folderDialog.open()
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    FolderDialog { id: folderDialog; onAccepted: { dir.text = selectedFolder.toString().replace("file://", ""); w.set("downloads.directory", dir.text) } }
                }
                Check { text: "Always ask where to save files"; key: "downloads.askWhereToSave" }
            }

            // ---- notifications ("downloads.notifications", Main.notifyDownloadComplete) ----
            SectionHeader { text: "Notifications" }
            Form {
                Check { id: dlNotify; Kirigami.FormData.label: "Finished downloads:"; text: "Show a notification when a download finishes"; key: "downloads.notifications" }
                RowLayout {
                    Kirigami.FormData.label: "Close after:"
                    enabled: dlNotify.checked
                    QQC2.SpinBox {
                        from: 0; to: 300; stepSize: 5; editable: true
                        value: w.get("downloads.notificationTimeout")
                        textFromValue: v => v === 0 ? "Desktop default" : v + " s"
                        valueFromText: t => /default/i.test(t) ? 0 : (parseInt(t) || 0)
                        onValueModified: w.set("downloads.notificationTimeout", value)
                    }
                }
                Hint { text: "Seconds the notification stays; 0 leaves it to the desktop's notification settings. With Plasma Integration showing downloads in the notification area, Plasma shows them as jobs and decides how long they stay." }
            }

            // ---- an external download manager ("downloads.external", external.rs) ----
            SectionHeader { text: "External Download Manager" }
            Form {
                Check { id: externalOn; Kirigami.FormData.label: "Downloads:"; text: "Use an external download manager"; key: "downloads.external" }
                RowLayout {
                    Kirigami.FormData.label: "Program:"
                    enabled: externalOn.checked
                    QQC2.TextField {
                        id: externalProgram
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
                        placeholderText: "e.g. kget, uget-gtk or /path/to/program"
                        text: w.get("downloads.externalProgram")
                        onEditingFinished: w.set("downloads.externalProgram", text.trim())
                    }
                    QQC2.Button {
                        icon.name: "document-open"
                        text: "Choose Program…"
                        display: QQC2.AbstractButton.IconOnly
                        onClicked: programDialog.open()
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    FileDialog {
                        id: programDialog
                        title: "Choose a Download Manager"
                        currentFolder: "file:///usr/bin"
                        onAccepted: {
                            externalProgram.text = decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, ""))
                            w.set("downloads.externalProgram", externalProgram.text)
                        }
                    }
                }
                QQC2.TextField {
                    Kirigami.FormData.label: "Arguments:"
                    enabled: externalOn.checked
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 18
                    font.family: "monospace"
                    text: w.get("downloads.externalArguments")
                    onEditingFinished: w.set("downloads.externalArguments", text)
                }
                Hint { text: "%d is replaced with the file's address and %f with its name; without %d, the address goes last. Quotes group words, as in a terminal." }
                Hint { text: "The program gets only the address, not your sign-in to the site, so downloads that need you to be signed in may fail there. Private windows, saved pages and files made by a page still download in Kite." }
            }
        }
    }

    // Leaving a page closes its submenus (a site, a start page file, a plugin).
    property string settledPage: ""
    Component.onCompleted: settledPage = page
    onPageChanged: {
        if (page === settledPage) return
        leavePage(() => {
            settledPage = page
            if (pageStack.depth > 1) pageStack.pop(pageStack.initialPage)
        })
    }

    // A submenu with changes not applied (one with "unsaved", so a site's) asks
    // before it's left, like System Settings. Whatever was on its way waits
    // for the answer, in order; a page opened from elsewhere may come with
    // both a change of page and a submenu.
    property var waitingToLeave: []
    readonly property bool askingAboutUnsaved: unsavedDialog.visible
    function hasUnsaved() {
        const current = pageStack.currentItem
        return !!current && !!current.unsaved
    }
    function leavePage(then) {
        if (!hasUnsaved()) { then(); return }
        waitingToLeave.push(then)
        if (!askingAboutUnsaved) {
            unsavedDialog.answer = ""
            unsavedDialog.open()
        }
    }
    onClosing: close => {
        if (!hasUnsaved()) return
        close.accepted = false
        if (!askingAboutUnsaved) leavePage(() => w.close())
    }
    Kirigami.PromptDialog {
        id: unsavedDialog
        property string answer
        title: "Apply Changes?"
        subtitle: "The settings of " + (w.pageStack.currentItem ? w.pageStack.currentItem.title : "this site") + " have changed. Apply them or discard them?"
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action { text: "Apply"; icon.name: "dialog-ok-apply"; onTriggered: { unsavedDialog.answer = "apply"; unsavedDialog.close() } },
            Kirigami.Action { text: "Discard"; icon.name: "edit-delete"; onTriggered: { unsavedDialog.answer = "discard"; unsavedDialog.close() } },
            Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: unsavedDialog.close() }
        ]
        // (Esc or a click outside is Cancel too.)
        onClosed: {
            const waiting = w.waitingToLeave
            w.waitingToLeave = []
            if (answer === "") {
                w.page = w.settledPage
                return
            }
            const current = w.pageStack.currentItem
            if (answer === "apply") current.applyChanges()
            else current.discardChanges()
            for (const then of waiting) then()
        }
    }

    // Site Settings: sites with their own zoom, dark mode, CSS or JavaScript
    // (stored per host; see sitesettings.rs). The list; a site's settings are
    // a page of their own (also opened from the address bar's site info popup).
    function openSite(host) {
        leavePage(() => {
            if (pageStack.depth > 1) pageStack.pop(pageStack.initialPage)
            pageStack.push(siteEditorPage, {host: host})
        })
    }
    Component {
        id: sites
        PageColumn {
            id: ss
            property var list: []
            function reload() { list = JSON.parse(Browser.allSiteSettings()) }
            Component.onCompleted: { reload(); suggest() }
            // The current tab's site, if it has no settings yet, ready to add.
            // (Replaces an earlier suggestion, not what the user typed.)
            property string suggested: ""
            function suggest() {
                if (newHost.text !== suggested) return
                const win = w.app.activeWindow
                const url = win && win.currentTab ? win.currentTab.url : ""
                const host = /^https?:/i.test(url) ? Browser.hostOf(url) : ""
                suggested = host && !list.some(e => e.host === host) ? host : ""
                newHost.text = suggested
            }
            Connections { target: w; function onActiveChanged() { if (w.active) ss.suggest() } }
            // A short description of what is set for a site.
            function summary(s) {
                const parts = []
                if (s.zoom > 0) parts.push("Zoom " + s.zoom + "%")
                if (s.dark_mode === 1) parts.push("Always dark")
                if (s.dark_mode === 2) parts.push("Always light")
                if (s.cookies === 1) parts.push("Cookies allowed")
                if (s.cookies === 2) parts.push("Cookies blocked")
                if (s.cookies === 3) parts.push("Cookies until Kite closes")
                if (s.fingerprint === 1) parts.push("Fingerprint protection")
                if (s.user_agent) parts.push("Own user agent")
                if (s.css) parts.push("Custom CSS")
                if (s.js) parts.push("Custom JavaScript")
                if (!parts.length && (s.javascript || s.images || s.notifications || s.geolocation || s.midi || s.midi_sysex)) parts.push("Permissions")
                return parts.join(" · ") || "Nothing set"
            }

            // The list follows changes (a site page, the site info popup).
            Connections { target: w.app; function onSiteSettingsEdited() { ss.reload() } }

            ColumnLayout {
                Layout.fillWidth: true
                Hint {
                    Layout.maximumWidth: -1
                    text: "Sites can have their own zoom level, dark mode, cookie rules, user agent, style sheet and script. Zoom levels you set while browsing are remembered here too."
                }
                RowLayout {
                    QQC2.TextField {
                        id: newHost
                        placeholderText: "example.com"
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 16
                        onAccepted: addButton.clicked()
                    }
                    QQC2.Button {
                        id: addButton
                        text: "Add Site"
                        icon.name: "list-add"
                        enabled: newHost.text.trim() !== ""
                        onClicked: {
                            // A URL or a bare host name.
                            const t = newHost.text.trim()
                            const host = Browser.hostOf(t.includes("://") ? t : "https://" + t)
                            if (!host) return
                            newHost.text = ""
                            w.openSite(host)
                        }
                    }
                }
                QQC2.Label { visible: ss.list.length === 0; text: "No site-specific settings yet."; opacity: 0.7 }
                Repeater {
                    model: ss.list
                    delegate: QQC2.ItemDelegate {
                        required property var modelData
                        Layout.fillWidth: true
                        onClicked: w.openSite(modelData.host)
                        contentItem: RowLayout {
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                QQC2.Label { text: modelData.host; font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
                                QQC2.Label { text: ss.summary(modelData.settings); opacity: 0.7; font: Kirigami.Theme.smallFont; Layout.fillWidth: true; elide: Text.ElideRight }
                            }
                            QQC2.ToolButton {
                                icon.name: "edit-delete"
                                text: "Remove all settings of this site"
                                display: QQC2.AbstractButton.IconOnly
                                onClicked: { Browser.clearSiteSettings(modelData.host); w.app.siteSettingsChanged(modelData.host); ss.reload() }
                                QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            }
                        }
                    }
                }
            }
        }
    }

    // A site's settings (Site Settings → a site).
    Component {
        id: siteEditorPage
        Kirigami.ScrollablePage {
            id: sitePage
            property string host
            title: host
            readonly property bool unsaved: editor.changed
            function applyChanges() { editor.save() }
            function discardChanges() { editor.load() }
            onBackRequested: event => {
                if (!unsaved) return
                event.accepted = true
                // (Kirigami goes back anyway, as it gets a copy of the event. The
                // page only goes once that's over, so it can be brought back first.)
                const index = Kirigami.ColumnView.index
                Qt.callLater(() => {
                    w.pageStack.currentIndex = index
                    w.leavePage(() => w.pageStack.pop())
                })
            }
            // Always in view, however long the page gets (like System Settings' bar).
            footer: ApplyBar {
                changed: editor.changed
                onApply: editor.save()
                onCancel: editor.load()
                leading: QQC2.Button {
                    text: "Remove Site"
                    icon.name: "edit-delete"
                    onClicked: { Browser.clearSiteSettings(sitePage.host); w.app.siteSettingsChanged(sitePage.host); w.pageStack.pop() }
                }
            }
            Item {
            implicitHeight: editor.implicitHeight
            PageColumn {
                id: editor
                width: w.columnWidthIn(sitePage)
                x: w.columnXIn(sitePage)
                property var site: ({})
                function load() {
                    site = JSON.parse(Browser.siteSettings(sitePage.host))
                    zoomCustom.checked = site.zoom > 0
                    zoomValue.value = site.zoom > 0 ? site.zoom : w.get("appearance.defaultZoom")
                    darkMode.currentIndex = site.dark_mode || 0
                    cookieRule.currentIndex = site.cookies || 0
                    fingerprint.checked = site.fingerprint === 1
                    uaChoice.load(site.user_agent || "")
                    css.text = site.css || ""
                    js.text = site.js || ""
                }
                readonly property bool changed: (zoomCustom.checked ? zoomValue.value : -1) !== (site.zoom > 0 ? site.zoom : -1)
                    || darkMode.currentIndex !== (site.dark_mode || 0) || cookieRule.currentIndex !== (site.cookies || 0) || fingerprint.checked !== (site.fingerprint === 1) || uaChoice.value !== (site.user_agent || "") || css.text !== (site.css || "") || js.text !== (site.js || "")
                function save() {
                    const h = sitePage.host
                    Browser.setSiteSetting(h, "zoom", zoomCustom.checked ? zoomValue.value : -1)
                    Browser.setSiteSetting(h, "darkMode", darkMode.currentIndex)
                    Browser.setSiteSetting(h, "cookies", cookieRule.currentIndex)
                    Browser.setSiteSetting(h, "fingerprint", fingerprint.checked ? 1 : 0)
                    Browser.setSiteText(h, "css", css.text)
                    Browser.setSiteText(h, "js", js.text)
                    Browser.setSiteText(h, "userAgent", uaChoice.value)
                    w.app.siteSettingsChanged(h)
                    load()
                }
                Component.onCompleted: load()

                SectionHeader { text: "Appearance" }
                Form {
                    id: siteForm
                    RowLayout {
                        Kirigami.FormData.label: "Zoom:"
                        QQC2.CheckBox { id: zoomCustom; text: "Own zoom level" }
                        QQC2.SpinBox { id: zoomValue; enabled: zoomCustom.checked; from: 30; to: 300; stepSize: 10; textFromValue: v => v + "%"; valueFromText: t => parseInt(t) }
                    }
                    QQC2.ComboBox {
                        id: darkMode
                        Kirigami.FormData.label: "Dark mode:"
                        model: [(w.get("appearance.forceDarkMode") ? "Like other sites (forced dark)" : "Like other sites (as designed)"), "Always force dark", "Always force light"]
                    }
                }
                SectionHeader { text: "Privacy and Identity" }
                Form {
                    // Per-site cookie rules ("privacy.siteCookieRules").
                    QQC2.ComboBox {
                        id: cookieRule
                        Kirigami.FormData.label: "Cookies:"
                        enabled: w.get("privacy.siteCookieRules")
                        model: ["Like other sites", "Always allow", "Block", "Allow until Kite closes"]
                        QQC2.ToolTip.text: enabled ? "Cookies of this site and its subdomains, also when embedded in other sites." : "Turn on per-site cookie rules in Settings → Privacy to use this."
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        // (A disabled control gets no hover: the hint is shown below.)
                    }
                    Hint {
                        visible: !cookieRule.enabled
                        text: "Turn on per-site cookie rules in Privacy settings to use this."
                    }
                    // Fingerprint protection ("privacy.antiFingerprinting").
                    QQC2.CheckBox {
                        id: fingerprint
                        Kirigami.FormData.label: "Fingerprinting:"
                        enabled: w.get("privacy.antiFingerprinting")
                        text: "Protect against fingerprinting"
                    }
                    Hint {
                        text: fingerprint.enabled
                            ? "Blurs canvas, WebGL and audio readouts and reports generic hardware, so this site can't recognise you by them. Some sites (drawing tools, maps, captchas) may misbehave."
                            : "Turn on fingerprint protection in Privacy settings to use this."
                    }
                    // The site's own user agent: sent to its server and seen by its pages.
                    UserAgentField { id: uaChoice; Kirigami.FormData.label: "User agent:" }
                    Hint {
                        text: "For sites that refuse Kite or show it the wrong page. Some refuse a browser claiming to be Chrome that isn't: \"Kite, naming QtWebEngine\" tells them what it is."
                    }
                }
                SectionHeader { text: "Custom CSS" }
                Hint { Layout.maximumWidth: -1; text: "Added to every page of this site. Use !important to override the site's own styles." }
                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 9
                    QQC2.TextArea { id: css; font.family: "monospace"; wrapMode: TextEdit.NoWrap; placeholderText: "body { font-size: 110% !important; }" }
                }
                SectionHeader { text: "Custom JavaScript" }
                Hint { Layout.maximumWidth: -1; text: "Runs in every page of this site once it has loaded, with the same access as the site's own scripts. Only add code you trust." }
                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 9
                    QQC2.TextArea { id: js; font.family: "monospace"; wrapMode: TextEdit.NoWrap; placeholderText: "document.querySelector('#banner')?.remove()" }
                }
                Hint { Layout.maximumWidth: -1; text: "Zoom and forced dark apply to open tabs right away; forced light, the user agent, CSS and JavaScript when the site's pages are next loaded." }
            }
            }
        }
    }

    Component {
        id: extensions
        PageColumn {
            // (The bar's "Install Plugin…".)
            function installPlugin() { pluginFileDialog.open() }
            // ---- Kite's own optional features ----
            SectionHeader { text: "Plugins" }
            Hint { Layout.maximumWidth: -1; text: "Kite's native plugins for powerusers." }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: w.app.pluginsDisabled
                type: Kirigami.MessageType.Information
                text: "Kite was started with --no-plugins: no plugin is running, and they can't be turned on. Those you installed can still be removed. Start Kite normally to get them back."
            }
            // (Rows back to back, as a list.)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: w.app.plugins
                    delegate: ListRow {
                        id: entry
                        required property var modelData
                        // On or off as shown; what's applied is pluginEnabled().
                        readonly property bool applied: w.app.pluginEnabled(modelData)
                        property bool on: applied
                        readonly property string change: "plugin:" + modelData.id
                        RowLayout {
                            spacing: Kirigami.Units.largeSpacing
                            // (The size they had: themes draw some icons differently when larger.)
                            Kirigami.Icon { source: entry.modelData.icon; implicitWidth: Kirigami.Units.iconSizes.smallMedium; implicitHeight: implicitWidth; Layout.alignment: Qt.AlignTop }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                // (Installed ones come from anyone: their text is only text.)
                                QQC2.Label { text: entry.modelData.name; textFormat: Text.PlainText; font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    opacity: 0.7
                                    font: Kirigami.Theme.smallFont
                                    text: (entry.modelData.installed ? "Installed from a file, not part of Kite. " : "") + entry.modelData.description
                                }
                            }
                            QQC2.Switch {
                                Accessible.name: entry.modelData.name
                                enabled: !w.app.pluginsDisabled
                                checked: entry.on
                                onToggled: {
                                    entry.on = checked
                                    if (checked === entry.applied) w.unstage(entry.change)
                                    else { const plugin = entry.modelData, on = checked; w.stage(entry.change, () => w.app.setPluginEnabled(plugin, on)) }
                                }
                            }
                            RowButton {
                                visible: !!entry.modelData.settingsPage
                                // (Once it's on for real.)
                                enabled: entry.on && !(entry.change in w.pendingActions)
                                text: "Configure " + entry.modelData.name + "…"
                                icon.name: "configure"
                                onClicked: w.openPluginSettings(entry.modelData)
                            }
                            RowButton {
                                visible: !!entry.modelData.installed
                                text: "Remove " + entry.modelData.name + "…"
                                icon.name: "edit-delete"
                                onClicked: { removePluginPrompt.plugin = entry.modelData; removePluginPrompt.open() }
                            }
                        }
                    }
                }
            }
            FileDialog {
                id: pluginFileDialog
                title: "Install Plugin from File"
                nameFilters: ["Kite plugins (*.zip)", "All files (*)"]
                onAccepted: {
                    const found = JSON.parse(Browser.inspectPlugin(selectedFile.toString()))
                    if (found.error) { pluginError.show(found.error); return }
                    installPluginPrompt.found = found
                    installPluginPrompt.open()
                }
            }
            // A plugin isn't an extension: nothing keeps it from anything.
            Kirigami.PromptDialog {
                id: installPluginPrompt
                // (Wide enough for its warning, however short the description.)
                preferredWidth: Kirigami.Units.gridUnit * 28
                property var found: null
                readonly property var plugin: found ? found.plugin : null
                title: plugin ? (found.update ? "Update " : "Install ") + plugin.name + "?" : ""
                standardButtons: Kirigami.Dialog.Cancel
                customFooterActions: Kirigami.Action {
                    text: installPluginPrompt.found && installPluginPrompt.found.update ? "Update" : "Install"
                    icon.name: "dialog-warning"
                    onTriggered: {
                        const update = installPluginPrompt.found.update, name = installPluginPrompt.plugin.name
                        installPluginPrompt.close()
                        const error = Browser.installInspectedPlugin()
                        if (error) { pluginError.show(error); return }
                        w.app.reloadPlugins()
                        pluginError.show(update ? name + " is updated. Restart Kite to use the new version." : name + " is installed. It stays off until you turn it on.", "Done")
                    }
                }
                ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing
                    QQC2.Label {
                        Layout.fillWidth: true
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: installPluginPrompt.plugin ? (installPluginPrompt.plugin.version ? "Version " + installPluginPrompt.plugin.version + "\n" : "") + installPluginPrompt.plugin.description : ""
                    }
                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                        visible: true
                        type: Kirigami.MessageType.Warning
                        text: "Only install plugins from people you trust. Nothing has checked this one. Unlike an extension, a plugin has no restrictions: when turned on, it runs as part of Kite and can read your saved logins, cookies and history, and see and change every page you visit, in private windows too."
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                        wrapMode: Text.Wrap
                        visible: installPluginPrompt.found && !installPluginPrompt.found.update
                        text: "It stays off until you turn it on."
                    }
                }
            }
            Kirigami.PromptDialog {
                id: removePluginPrompt
                property var plugin: null
                title: plugin ? "Remove " + plugin.name + "?" : ""
                subtitle: "It's deleted for all profiles, along with its settings in this one."
                standardButtons: Kirigami.Dialog.Cancel
                customFooterActions: Kirigami.Action {
                    text: "Remove"
                    icon.name: "edit-delete"
                    onTriggered: {
                        const error = w.app.removePlugin(removePluginPrompt.plugin)
                        removePluginPrompt.close()
                        if (error) pluginError.show(error)
                    }
                }
            }
            Kirigami.PromptDialog {
                id: pluginError
                function show(message, heading) { title = heading || "Couldn't install the plugin"; subtitle = message; open() }
                standardButtons: Kirigami.Dialog.Ok
            }
            // ---- WebExtensions ----
            SectionHeader { text: "WebExtensions" }
            Form {
                Check { id: extensionsOn; Kirigami.FormData.label: "Extensions:"; text: "Enable WebExtensions"; key: "extensions.enabled" }
                // (What installing does, which needs extensions.)
                Check { Kirigami.FormData.label: "Installing:"; enabled: extensionsOn.checked; text: "Identify as Firefox on addons.mozilla.org"; key: "extensions.amoSpoofFirefox" }
                Hint { text: "With this enabled, the add-on site shows “Add to Firefox” buttons; clicking one installs the add-on into Kite." }
                Check { enabled: extensionsOn.checked; text: "Let other sites offer add-ons to install"; key: "extensions.siteInstalls" }
                Hint { text: "Kite always asks first, and says which site offered it. With this off, add-on files from sites other than addons.mozilla.org download like any other file." }
            }
        }
    }
}
