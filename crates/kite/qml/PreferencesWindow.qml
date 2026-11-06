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

    function get(k) { return JSON.parse(Browser.setting(k)) }
    function set(k, v) { Browser.setSetting(k, JSON.stringify(v)) }

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
    Component { id: pageAction; Kirigami.Action { property string key; property string iconName; icon.name: iconName; checked: w.page === key; onTriggered: w.page = key } }

    // Submenus (a start page file, a site's settings) are pages pushed on
    // the stack: they slide in, one at a time, with a back arrow in the
    // header (like Discover).
    pageStack.columnView.columnResizeMode: Kirigami.ColumnView.SingleColumn
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.ToolBar
    pageStack.globalToolBar.showNavigationButtons: Kirigami.ApplicationHeaderStyle.ShowBackButton

    pageStack.initialPage: Kirigami.ScrollablePage {
        id: sp
        title: (w.pages.find(p => p.key === w.page) || {}).title || ""
        Loader {
            width: sp.width - Kirigami.Units.gridUnit * 2
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
        if (pageStack.depth > 1) pageStack.pop(pageStack.initialPage)
        pageStack.push(pluginSettingsPage, {plugin: plugin})
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
        ColumnLayout {
            id: sc
            spacing: Kirigami.Units.smallSpacing
            readonly property var overrides: w.app.liveSettings["shortcuts"] || {}
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
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.7
                font: Kirigami.Theme.smallFont
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
                    Kirigami.Heading { level: 4; text: grp.modelData; Layout.topMargin: Kirigami.Units.largeSpacing; Layout.bottomMargin: Kirigami.Units.smallSpacing }
                    Repeater {
                        model: grp.items
                        delegate: RowLayout {
                            id: row
                            required property var modelData
                            readonly property var current: sc.keys(modelData)
                            readonly property bool changed: sc.overrides[modelData.id] !== undefined
                            Layout.fillWidth: true
                            Layout.minimumHeight: Kirigami.Units.gridUnit * 1.8
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
                            QQC2.ToolButton {
                                icon.name: "list-add"
                                onClicked: recorder.record(row.modelData, -1)
                                QQC2.ToolTip.text: "Add a shortcut"; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            }
                            QQC2.ToolButton {
                                icon.name: "edit-undo"
                                enabled: row.changed
                                opacity: row.changed ? 1 : 0
                                onClicked: sc.reset(row.modelData)
                                QQC2.ToolTip.text: "Reset to " + (row.modelData.keys.length ? row.modelData.keys.map(k => Browser.keySequenceDisplay(k)).join(", ") : "none"); QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
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
            ColumnLayout {
                id: spm
                width: page.width - Kirigami.Units.gridUnit * 2
                readonly property string target: page.target
                readonly property string fileKey: "startpage." + target + ".file"
                readonly property string copyKey: "startpage." + target + ".copy"
                Component.onCompleted: {
                    spFile.text = w.get(fileKey)
                    spCopy.checked = w.get(copyKey)
                }
                function edit() { Qt.openUrlExternally("file://" + encodeURI(w.app.startPageFile(target))) }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    RowLayout {
                        Kirigami.FormData.label: "HTML file:"
                        Layout.fillWidth: true
                        QQC2.TextField {
                            id: spFile
                            Layout.fillWidth: true
                            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
                            placeholderText: "No file chosen"
                            onEditingFinished: if (text.trim() !== w.get(spm.fileKey)) w.set(spm.fileKey, text.trim())
                        }
                        QQC2.Button { text: "Choose…"; icon.name: "document-open"; onClicked: spDialog.open() }
                        FileDialog {
                            id: spDialog
                            nameFilters: ["Web pages (*.html *.htm *.xhtml)", "All files (*)"]
                            onAccepted: {
                                const path = decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, ""))
                                spFile.text = path
                                w.set(spm.fileKey, path)
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
                            if (checked && !w.get(spm.fileKey)) w.set(spm.fileKey, spFile.text)
                            w.set(spm.copyKey, checked)
                        }
                    }
                    QQC2.Label {
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
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
                                w.set(spm.fileKey, path)
                                w.set(spm.copyKey, true)
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
                    QQC2.Label {
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                        text: "Saving the file reloads the page in Kite."
                    }
                }
            }
        }
    }

    Component {
        id: general
        ColumnLayout {
            // Home page / new tab page: an address or a file (managed below).
            component PageChoice: ColumnLayout {
                id: pc
                property string target        // "home" | "newtab"
                property string modeKey
                property string urlKey
                property string placeholder: ""
                readonly property bool file: mode === "file"
                property string mode: w.get(modeKey)
                spacing: Kirigami.Units.smallSpacing
                RowLayout {
                    QQC2.RadioButton { text: "Address"; checked: !pc.file; onToggled: if (checked) { pc.mode = "url"; w.set(pc.modeKey, "url") } }
                    QQC2.RadioButton { text: "File"; checked: pc.file; onToggled: if (checked) { pc.mode = "file"; w.set(pc.modeKey, "file") } }
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

            Kirigami.FormLayout {
                Layout.fillWidth: true
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
                    onClicked: { w.set("general.homepage", w.app.activeWindow.currentTab.url); homeChoice.mode = ""; homeChoice.mode = "url" }
                }
                PageChoice { Kirigami.FormData.label: "New tab page:"; Kirigami.FormData.labelAlignment: Qt.AlignTop; target: "newtab"; modeKey: "general.newTabPageMode"; urlKey: "general.newTabPage"; placeholder: "kite:start" }
                QQC2.CheckBox { Kirigami.FormData.label: "Start page:"; text: "Show site tiles (sites you visit often and ones you add)"; checked: w.get("speeddial.showTiles"); onToggled: w.set("speeddial.showTiles", checked) }
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
                RowLayout {
                    Kirigami.FormData.label: "Settings file:"
                    QQC2.Button { text: "Export…"; icon.name: "document-export"; onClicked: settingsExport.open() }
                    QQC2.Button { text: "Import…"; icon.name: "document-import"; onClicked: settingsImport.open() }
                }
                QQC2.Label {
                    // (A set width: a form makes room for a text's whole length otherwise.)
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    font: Kirigami.Theme.smallFont
                    text: "To carry your settings, search engines and site settings to another computer. Not your window size, download folder, start page files or Safe Browsing key; bookmarks and logins have exports of their own."
                }
                QQC2.Label { Kirigami.FormData.label: "Profile:"; text: Browser.profileName() + "  (" + Browser.webEnginePath().replace(/\/webengine$/, "") + ")"; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }

        }
    }

    Component {
        id: appearance
        Kirigami.FormLayout {
            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "General" }
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

            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Tabs" }
            QQC2.ComboBox {
                id: tabPos
                Kirigami.FormData.label: "Position:"
                model: [{k: "top", t: "Above the address bar"}, {k: "belowToolbar", t: "Below the address bar"}, {k: "bottom", t: "Below the page"}, {k: "vertical", t: "Vertical list on the left"}, {k: "verticalRight", t: "Vertical list on the right"}]
                textRole: "t"
                currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("tabs.position")))
                onActivated: i => w.set("tabs.position", model[i].k)
            }
            QQC2.CheckBox { text: "Hide the tab bar when only one tab is open"; checked: w.get("tabs.hideWhenSingle"); onToggled: w.set("tabs.hideWhenSingle", checked) }
            QQC2.Label { text: "Also available by right-clicking the tab bar."; opacity: 0.7; font: Kirigami.Theme.smallFont }
            QQC2.CheckBox { enabled: tabPos.currentIndex < 3; text: "Stretch tabs across the whole tab bar"; checked: w.get("tabs.fillWidth"); onToggled: w.set("tabs.fillWidth", checked) }
            QQC2.Label { text: "Tabs share the full width and get narrower as more are opened (horizontal tab bars)."; opacity: 0.7; font: Kirigami.Theme.smallFont }
            QQC2.CheckBox { text: "Tree-style tabs in vertical lists"; checked: w.get("tabs.tree"); onToggled: w.set("tabs.tree", checked) }
            QQC2.Label { text: "Links open as children of their tab. Drag a tab sideways to change its level; its children move with it."; opacity: 0.7; font: Kirigami.Theme.smallFont; wrapMode: Text.Wrap; Layout.fillWidth: true }
            QQC2.CheckBox { Kirigami.FormData.label: "Opening:"; text: "Open new tabs after the current tab"; checked: w.get("tabs.newTabAfterActive"); onToggled: w.set("tabs.newTabAfterActive", checked) }
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
            QQC2.CheckBox { text: "Print backgrounds"; checked: w.get("web.printBackgrounds"); onToggled: w.set("web.printBackgrounds", checked) }
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
            QQC2.Label { text: "Scroll distance on web pages. Touchpads and Ctrl+wheel (zoom) are not affected."; opacity: 0.7; font: Kirigami.Theme.smallFont }
            QQC2.CheckBox { Kirigami.FormData.label: "Spelling:"; text: "Check spelling while typing"; checked: w.get("web.spellcheck"); onToggled: w.set("web.spellcheck", checked) }
            QQC2.TextField { Kirigami.FormData.label: "Languages:"; text: w.get("web.spellcheckLanguages").join(", "); onEditingFinished: w.set("web.spellcheckLanguages", text.split(",").map(s => s.trim()).filter(s => s)) }
            QQC2.TextField { Kirigami.FormData.label: "User agent:"; Layout.fillWidth: true; placeholderText: "Default"; text: w.get("web.userAgent"); onEditingFinished: w.set("web.userAgent", text) }
            QQC2.Label { text: "User agent and spelling changes apply after restarting Kite."; opacity: 0.7; font: Kirigami.Theme.smallFont }

            // ---- memory ("tabs.unloadInactive", WebTab.unload()) ----
            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Memory" }
            QQC2.CheckBox { id: unloadOn; Kirigami.FormData.label: "Inactive tabs:"; text: "Unload tabs that haven't been shown for a while"; checked: w.get("tabs.unloadInactive"); onToggled: w.set("tabs.unloadInactive", checked) }
            RowLayout {
                enabled: unloadOn.checked
                QQC2.Label { text: "Unload after" }
                QQC2.SpinBox { from: 1; to: 1440; editable: true; value: w.get("tabs.unloadAfterMinutes"); onValueModified: w.set("tabs.unloadAfterMinutes", value) }
                QQC2.Label { text: "minutes" }
            }
            QQC2.CheckBox { enabled: unloadOn.checked; text: "Keep pinned tabs loaded"; checked: w.get("tabs.unloadKeepPinned"); onToggled: w.set("tabs.unloadKeepPinned", checked) }
            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                wrapMode: Text.Wrap
                opacity: 0.8
                font: Kirigami.Theme.smallFont
                text: "Unloaded tabs keep their place, title and history, and reload when you switch to them (shown faded in the tab bar). Tabs playing sound, keeping the screen on, loading, with text typed into a form or with developer tools open stay loaded. Any tab can also be unloaded from its context menu."
            }

            // ---- form autofill ("browsing.formFill", formfill.rs) ----
            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Forms" }
            QQC2.CheckBox { id: ffOn; Kirigami.FormData.label: "Autofill:"; text: "Fill in forms with saved addresses"; checked: w.get("browsing.formFill"); onToggled: { w.set("browsing.formFill", checked); if (checked && FormFill.count > 0) FormFill.unlock() } }
            QQC2.CheckBox { enabled: ffOn.checked; text: "Offer to save addresses typed into forms"; checked: w.get("browsing.formFillSave"); onToggled: w.set("browsing.formFillSave", checked) }
            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                wrapMode: Text.Wrap
                opacity: 0.8
                font: Kirigami.Theme.smallFont
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
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 26
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
                        QQC2.Label { text: modelData.label; Layout.fillWidth: true; Layout.maximumWidth: Kirigami.Units.gridUnit * 20; elide: Text.ElideRight }
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
            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Dangerous Sites" }
            QQC2.ComboBox {
                id: sbMode
                Kirigami.FormData.label: "Protection:"
                model: [{k: "off", t: "Disabled"}, {k: "google", t: "Use Google Safe Browsing"}, {k: "open", t: "Use open lists"}]
                textRole: "t"
                currentIndex: Math.max(0, model.findIndex(m => m.k === w.get("browsing.safeBrowsing")))
                onActivated: i => { w.set("browsing.safeBrowsing", model[i].k); sbStatus.refresh() }
            }
            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                wrapMode: Text.Wrap
                opacity: 0.8
                font: Kirigami.Theme.smallFont
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
            spacing: Kirigami.Units.largeSpacing
            component Hint: QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 26
                wrapMode: Text.Wrap
                opacity: 0.7
                font: Kirigami.Theme.smallFont
            }
            component Check: QQC2.CheckBox {
                property string key
                checked: !!w.get(key)
                onToggled: w.set(key, checked)
            }

            Kirigami.FormLayout {
                Layout.fillWidth: true
                Check { Kirigami.FormData.label: "History:"; text: "Remember browsing history"; key: "privacy.saveHistory" }
                Check { text: "Clear history, cookies and cache when Kite closes"; key: "privacy.clearOnExit" }
                Check { Kirigami.FormData.label: "Private windows:"; text: "Show often visited sites on the start page"; key: "privacy.privateHistoryTiles" }
                Check { text: "Suggest pages from the history in the address bar"; key: "privacy.privateHistorySuggestions" }
                Check { text: "Show notifications from pages and downloads"; key: "privacy.privateNotifications" }
                QQC2.Label { text: "These show pages from your regular browsing in private windows, where others might see them. What you added yourself, tiles and bookmarks, always shows."; opacity: 0.7; font: Kirigami.Theme.smallFont; wrapMode: Text.Wrap; Layout.fillWidth: true }
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
                QQC2.Button { Kirigami.FormData.label: "Certificates:"; text: "Manage Certificate Exceptions…"; icon.name: "security-medium"; onClicked: w.pageStack.push(certificatesPage) }

                Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Tracking" }
                Check { Kirigami.FormData.label: "Ask sites:"; text: "Send Global Privacy Control"; key: "privacy.globalPrivacyControl" }
                Hint { text: "Asks sites not to sell or share your data. Legally binding in some places, such as California." }
                Check { text: "Send “Do Not Track”"; key: "web.doNotTrack" }
                Hint { text: "Few sites honour it, and as few people send it, it makes you easier to recognise." }
                Check { Kirigami.FormData.label: "Links:"; text: "Remove tracking parameters from addresses"; key: "privacy.stripTrackingParams" }
                Hint { text: "Click and campaign identifiers such as fbclid, gclid and utm_… are removed before a page loads." }
                Check { text: "Block link click pings"; key: "privacy.blockPings" }
                Check { text: "Tell sites which page linked to them (referrer)"; key: "privacy.sendReferer" }

                Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Fingerprinting" }
                Check { Kirigami.FormData.label: "Identity:"; text: "Use a common user agent"; key: "privacy.genericUserAgent" }
                Hint { text: "Leaves out “QtWebEngine” from what Kite tells sites about itself, so it reads like Chrome." }
                Check { text: "Appear as Firefox when signing in to Google"; key: "web.googleSignIn" }
                Hint { text: "Google refuses to sign you in with browsers it doesn't know. This only applies on accounts.google.com, and only tells that site Kite is Firefox." }
                Check { text: "Hide local network addresses from WebRTC"; key: "privacy.webrtcPublicOnly" }
                Check { text: "Allow fingerprint protection"; key: "privacy.antiFingerprinting" }
                Hint { text: "Turn it on for single sites in Site Settings: canvas, WebGL and audio readouts are blurred and the hardware reported is generic. Some sites may misbehave." }

                Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Connections" }
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

                Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Proxy" }
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

                Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Container tabs" }
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

                Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Clear Recent History" }
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
                QQC2.CheckBox { id: genOn; Kirigami.FormData.label: "Generator:"; text: "Suggest strong passwords on sign-up forms"; checked: w.get("passwords.generator"); onToggled: w.set("passwords.generator", checked) }
                RowLayout {
                    enabled: genOn.checked
                    QQC2.SpinBox { from: 8; to: 64; value: w.get("passwords.generatorLength"); onValueModified: w.set("passwords.generatorLength", value) }
                    QQC2.Label { text: "characters" }
                    QQC2.CheckBox { text: "Include symbols"; checked: w.get("passwords.generatorSymbols"); onToggled: w.set("passwords.generatorSymbols", checked) }
                }
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
        ColumnLayout {
            id: ab
            property var subs: w.get("adblock.subscriptions")
            property string status: ""
            Connections { target: Browser; function onAdblockStatus(m) { ab.status = m } }
            readonly property string extBlocker: Browser.contentBlocker(false)
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

    // Leaving a page closes its submenus (a site, a start page file, a plugin).
    onPageChanged: if (pageStack.depth > 1) pageStack.pop(pageStack.initialPage)

    // Site Settings: sites with their own zoom, dark mode, CSS or JavaScript
    // (stored per host; see sitesettings.rs). The list; a site's settings are
    // a page of their own (also opened from the address bar's site info popup).
    function openSite(host) {
        if (pageStack.depth > 1) pageStack.pop(pageStack.initialPage)
        pageStack.push(siteEditorPage, {host: host})
    }
    Component {
        id: sites
        ColumnLayout {
            id: ss
            spacing: Kirigami.Units.largeSpacing
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
                if (s.dark_mode === 2) parts.push("Never dark")
                if (s.cookies === 1) parts.push("Cookies allowed")
                if (s.cookies === 2) parts.push("Cookies blocked")
                if (s.cookies === 3) parts.push("Cookies until Kite closes")
                if (s.fingerprint === 1) parts.push("Fingerprint protection")
                if (s.css) parts.push("Custom CSS")
                if (s.js) parts.push("Custom JavaScript")
                if (!parts.length && (s.javascript || s.images || s.notifications || s.geolocation)) parts.push("Permissions")
                return parts.join(" · ") || "Nothing set"
            }

            // The list follows changes (a site page, the site info popup).
            Connections { target: w.app; function onSiteSettingsEdited() { ss.reload() } }

            ColumnLayout {
                Layout.fillWidth: true
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "Sites can have their own zoom level, dark mode, style sheet and script. Zoom levels you set while browsing are remembered here too."
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
            ColumnLayout {
                id: editor
                width: sitePage.width - Kirigami.Units.gridUnit * 2
                spacing: Kirigami.Units.largeSpacing
                property var site: ({})
                function load() {
                    site = JSON.parse(Browser.siteSettings(sitePage.host))
                    zoomCustom.checked = site.zoom > 0
                    zoomValue.value = site.zoom > 0 ? site.zoom : w.get("appearance.defaultZoom")
                    darkMode.currentIndex = site.dark_mode || 0
                    cookieRule.currentIndex = site.cookies || 0
                    fingerprint.checked = site.fingerprint === 1
                    css.text = site.css || ""
                    js.text = site.js || ""
                }
                readonly property bool changed: (zoomCustom.checked ? zoomValue.value : -1) !== (site.zoom > 0 ? site.zoom : -1)
                    || darkMode.currentIndex !== (site.dark_mode || 0) || cookieRule.currentIndex !== (site.cookies || 0) || fingerprint.checked !== (site.fingerprint === 1) || css.text !== (site.css || "") || js.text !== (site.js || "")
                function save() {
                    const h = sitePage.host
                    Browser.setSiteSetting(h, "zoom", zoomCustom.checked ? zoomValue.value : -1)
                    Browser.setSiteSetting(h, "darkMode", darkMode.currentIndex)
                    Browser.setSiteSetting(h, "cookies", cookieRule.currentIndex)
                    Browser.setSiteSetting(h, "fingerprint", fingerprint.checked ? 1 : 0)
                    Browser.setSiteText(h, "css", css.text)
                    Browser.setSiteText(h, "js", js.text)
                    w.app.siteSettingsChanged(h)
                    load()
                }
                Component.onCompleted: load()

                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    RowLayout {
                        Kirigami.FormData.label: "Zoom:"
                        QQC2.CheckBox { id: zoomCustom; text: "Own zoom level" }
                        QQC2.SpinBox { id: zoomValue; enabled: zoomCustom.checked; from: 30; to: 300; stepSize: 10; textFromValue: v => v + "%"; valueFromText: t => parseInt(t) }
                    }
                    QQC2.ComboBox {
                        id: darkMode
                        Kirigami.FormData.label: "Dark mode:"
                        model: [(w.get("appearance.forceDarkMode") ? "Like other sites (forced dark)" : "Like other sites (as designed)"), "Always force dark", "Never force dark"]
                    }
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
                    QQC2.Label {
                        visible: !cookieRule.enabled
                        text: "Turn on per-site cookie rules in Privacy settings to use this."
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                    }
                    // Fingerprint protection ("privacy.antiFingerprinting").
                    QQC2.CheckBox {
                        id: fingerprint
                        Kirigami.FormData.label: "Fingerprinting:"
                        enabled: w.get("privacy.antiFingerprinting")
                        text: "Protect against fingerprinting"
                    }
                    QQC2.Label {
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                        wrapMode: Text.Wrap
                        text: fingerprint.enabled
                            ? "Blurs canvas, WebGL and audio readouts and reports generic hardware, so this site can't recognise you by them. Some sites (drawing tools, maps, captchas) may misbehave."
                            : "Turn on fingerprint protection in Privacy settings to use this."
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                    }
                }
                Kirigami.Heading { level: 4; text: "Custom CSS" }
                QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; opacity: 0.7; font: Kirigami.Theme.smallFont; text: "Added to every page of this site. Use !important to override the site's own styles." }
                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 9
                    QQC2.TextArea { id: css; font.family: "monospace"; wrapMode: TextEdit.NoWrap; placeholderText: "body { font-size: 110% !important; }" }
                }
                Kirigami.Heading { level: 4; text: "Custom JavaScript" }
                QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; opacity: 0.7; font: Kirigami.Theme.smallFont; text: "Runs in every page of this site once it has loaded, with the same access as the site's own scripts. Only add code you trust." }
                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 9
                    QQC2.TextArea { id: js; font.family: "monospace"; wrapMode: TextEdit.NoWrap; placeholderText: "document.querySelector('#banner')?.remove()" }
                }
                QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; opacity: 0.7; font: Kirigami.Theme.smallFont; text: "Zoom and dark mode apply to open tabs right away; CSS and JavaScript when the site's pages are next loaded." }
                RowLayout {
                    QQC2.Button { text: "Save"; icon.name: "document-save"; enabled: editor.changed; onClicked: editor.save() }
                    QQC2.Button { text: "Revert"; icon.name: "edit-undo"; enabled: editor.changed; onClicked: editor.load() }
                    Item { Layout.fillWidth: true }
                    QQC2.Button {
                        text: "Remove Site"
                        icon.name: "edit-delete"
                        onClicked: { Browser.clearSiteSettings(sitePage.host); w.app.siteSettingsChanged(sitePage.host); w.pageStack.pop() }
                    }
                }
            }
        }
    }

    Component {
        id: extensions
        Kirigami.FormLayout {
            // (Left-aligned across the page, as Privacy comes out by its contents.)
            wideMode: false
            // ---- Kite's own optional features ----
            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "Plugins" }
            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 34
                wrapMode: Text.Wrap
                opacity: 0.7
                font: Kirigami.Theme.smallFont
                text: "Kite's native plugins for powerusers."
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.maximumWidth: Kirigami.Units.gridUnit * 34
                visible: w.app.pluginsDisabled
                type: Kirigami.MessageType.Information
                text: "Kite was started with --no-plugins: no plugin is running, and they can't be turned on. Those you installed can still be removed. Start Kite normally to get them back."
            }
            // (In a column of their own: the form would put rows made later at its end,
            // and installing a plugin makes them anew.)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: w.app.plugins
                    delegate: ColumnLayout {
                        id: entry
                        required property var modelData
                        property bool on: w.app.pluginEnabled(modelData)
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Kirigami.Icon { source: entry.modelData.icon; implicitWidth: Kirigami.Units.iconSizes.smallMedium; implicitHeight: implicitWidth }
                            // (Installed ones come from anyone: their text is only text.)
                            QQC2.Label { text: entry.modelData.name; textFormat: Text.PlainText; Layout.fillWidth: true }
                            QQC2.Switch {
                                Accessible.name: entry.modelData.name
                                enabled: !w.app.pluginsDisabled
                                checked: entry.on
                                onToggled: { entry.on = checked; w.app.setPluginEnabled(entry.modelData, checked) }
                            }
                            QQC2.Button {
                                visible: !!entry.modelData.settingsPage
                                enabled: entry.on
                                text: "Configure…"
                                icon.name: "configure"
                                onClicked: w.openPluginSettings(entry.modelData)
                            }
                            QQC2.Button {
                                visible: !!entry.modelData.installed
                                text: "Remove…"
                                icon.name: "edit-delete"
                                onClicked: { removePluginPrompt.plugin = entry.modelData; removePluginPrompt.open() }
                            }
                        }
                        QQC2.Label {
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 34
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            opacity: 0.7
                            font: Kirigami.Theme.smallFont
                            text: (entry.modelData.installed ? "Installed from a file, not part of Kite. " : "") + entry.modelData.description
                        }
                    }
                }
            }
            QQC2.Button {
                text: "Install Plugin from File…"
                icon.name: "document-open"
                onClicked: pluginFileDialog.open()
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
            Kirigami.Separator { Kirigami.FormData.isSection: true; Kirigami.FormData.label: "WebExtensions" }
            QQC2.CheckBox { text: "Enable WebExtensions"; checked: w.get("extensions.enabled"); onToggled: { w.set("extensions.enabled", checked); w.app.installUserScripts(); w.app.backgroundHost.reload() } }
            QQC2.CheckBox { text: "Identify as Firefox on addons.mozilla.org"; checked: w.get("extensions.amoSpoofFirefox"); onToggled: w.set("extensions.amoSpoofFirefox", checked) }
            QQC2.Label { Layout.maximumWidth: Kirigami.Units.gridUnit * 26; wrapMode: Text.Wrap; opacity: 0.75; text: "With this enabled, the add-on site shows “Add to Firefox” buttons; clicking one installs the add-on into Kite." }
            QQC2.Button { text: "Manage Extensions…"; icon.name: "kite-extension"; onClicked: w.app.showAddons() }
        }
    }
}
