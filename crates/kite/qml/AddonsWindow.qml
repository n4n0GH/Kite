import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import org.kde.kite

// Managing extensions (Firefox's about:addons): installed extensions and a
// browser for addons.mozilla.org.
ManagerWindow {
    id: w
    property string page: "installed"
    title: "Extensions — Kite"
    width: Kirigami.Units.gridUnit * 50
    height: Kirigami.Units.gridUnit * 38

    globalDrawer: Kirigami.GlobalDrawer {
        modal: false
        collapsible: true
        collapsed: false
        width: Kirigami.Units.gridUnit * 11
        actions: [
            Kirigami.Action { text: "Extensions"; icon.name: "kite-extension"; checked: w.page === "installed"; onTriggered: w.page = "installed" },
            Kirigami.Action { text: "Find Extensions"; icon.name: "get-hot-new-stuff"; checked: w.page === "browse"; onTriggered: w.page = "browse" }
        ]
    }
    // Show `page`. The page is created with the page stack as its visual
    // parent: created parentless (an initialPage binding or pushing the
    // Component) it triggers "Created graphical object was not placed in the
    // graphics scene", and the binding created an extra page on each change.
    property bool _completed: false
    property Item _shown: null
    function showPage() {
        const old = _shown
        pageStack.clear()
        // Pages we create ourselves are ours to destroy.
        if (old) old.destroy()
        _shown = (page === "browse" ? browsePage : installedPage).createObject(pageStack)
        pageStack.push(_shown)
    }
    Component.onCompleted: { _completed = true; showPage() }
    onPageChanged: if (_completed) showPage()

    function formatUsers(n) { return n >= 1e6 ? (n / 1e6).toFixed(1) + "M" : n >= 1e3 ? Math.round(n / 1e3) + "K" : String(n) }

    property var updates: []
    Connections { target: Extensions; function onUpdatesFound(json) { w.updates = JSON.parse(json); w.app.toast(w.updates.length ? w.updates.length + " update(s) available" : "All extensions are up to date", "update-none") } }

    Component {
        id: installedPage
        Kirigami.ScrollablePage {
            id: ip
            title: "Manage Your Extensions"
            property var exts: JSON.parse(Extensions.list())
            // Which cards show their details (by id: a change rebuilds the cards).
            property var expandedIds: ({})
            Connections { target: Extensions; function onChanged(what) { if (what === "extensions") ip.exts = JSON.parse(Extensions.list()) } }
            actions: [
                Kirigami.Action { text: "Check for Updates"; icon.name: "view-refresh"; onTriggered: Extensions.checkUpdates() },
                Kirigami.Action { text: "Install Extension…"; icon.name: "document-open"; onTriggered: xpiDialog.open() },
                Kirigami.Action { text: "Load Temporary Extension…"; icon.name: "run-build"; onTriggered: tempDialog.open() }
            ]
            FileDialog { id: xpiDialog; title: "Install Extension From File"; nameFilters: ["Extension packages (*.xpi *.zip)"]; onAccepted: Extensions.installFromFile(selectedFile.toString()) }
            FileDialog { id: tempDialog; title: "Select manifest.json of an unpacked extension"; nameFilters: ["manifest.json (manifest.json)"]; onAccepted: w.app.toast(Extensions.loadTemporary(selectedFile.toString())) }

            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: w.updates.length > 0
                    type: Kirigami.MessageType.Positive
                    text: w.updates.map(u => u.name + " " + u.available).join(", ") + (w.updates.length === 1 ? " is available." : " are available.")
                    actions: [Kirigami.Action { text: "Update All"; icon.name: "update-none"; onTriggered: { for (const u of w.updates) Extensions.installFromAmo(u.slug); w.updates = [] } }]
                }
                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.gridUnit * 4
                    visible: ip.exts.length === 0
                    icon.name: "kite-extension"
                    text: "No extensions installed"
                    explanation: "Kite runs Firefox extensions. Find some on addons.mozilla.org."
                    helpfulAction: Kirigami.Action { text: "Find Extensions"; icon.name: "get-hot-new-stuff"; onTriggered: w.page = "browse" }
                }
                Repeater {
                    model: ip.exts
                    delegate: Kirigami.AbstractCard {
                        id: card
                        required property var modelData
                        Layout.fillWidth: true
                        readonly property bool expanded: !!ip.expandedIds[modelData.id]
                        function toggleExpanded() { const o = Object.assign({}, ip.expandedIds); o[modelData.id] = !expanded; ip.expandedIds = o }
                        contentItem: ColumnLayout {
                            RowLayout {
                                spacing: Kirigami.Units.largeSpacing
                                Kirigami.Icon {
                                    source: card.modelData.icon || "kite-extension"
                                    fallback: "kite-extension"
                                    Layout.preferredWidth: Kirigami.Units.iconSizes.large
                                    Layout.preferredHeight: Layout.preferredWidth
                                    opacity: card.modelData.enabled ? 1 : 0.5
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    RowLayout {
                                        Kirigami.Heading { level: 4; text: card.modelData.name; elide: Text.ElideRight; Layout.fillWidth: true; Layout.maximumWidth: implicitWidth }
                                        QQC2.Label { text: card.modelData.version; opacity: 0.6 }
                                        QQC2.Label { visible: card.modelData.temporary; text: "Temporary"; color: Kirigami.Theme.neutralTextColor }
                                        Kirigami.Icon {
                                            visible: !!card.modelData.allowPrivate
                                            source: "view-private"
                                            Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                            Layout.preferredHeight: Layout.preferredWidth
                                            HoverHandler { id: privateBadge }
                                            QQC2.ToolTip.text: "Runs in private windows"
                                            QQC2.ToolTip.visible: privateBadge.hovered
                                        }
                                        QQC2.Label { visible: !!card.modelData.broken; text: "Failed to load"; color: Kirigami.Theme.negativeTextColor }
                                        Item { Layout.fillWidth: true }
                                    }
                                    QQC2.Label { text: card.modelData.broken ? card.modelData.error : (card.modelData.description || ""); wrapMode: Text.Wrap; Layout.fillWidth: true; maximumLineCount: card.expanded ? 20 : 2; elide: Text.ElideRight }
                                }
                                QQC2.Switch {
                                    enabled: !card.modelData.broken
                                    checked: card.modelData.enabled
                                    onToggled: Extensions.setEnabled(card.modelData.id, checked)
                                    QQC2.ToolTip.text: checked ? "Disable" : "Enable"; QQC2.ToolTip.visible: hovered
                                }
                                QQC2.ToolButton {
                                    icon.name: "settings-configure"
                                    visible: !!card.modelData.optionsUrl
                                    enabled: card.modelData.enabled
                                    onClicked: card.modelData.optionsInTab ? w.app.openUrl(card.modelData.optionsUrl, "tab") : w.app.optionsDialog.open(card.modelData.optionsUrl, card.modelData.name)
                                    QQC2.ToolTip.text: "Options"; QQC2.ToolTip.visible: hovered
                                }
                                QQC2.ToolButton { icon.name: card.expanded ? "arrow-up" : "arrow-down"; onClicked: card.toggleExpanded(); QQC2.ToolTip.text: "Details"; QQC2.ToolTip.visible: hovered }
                                QQC2.ToolButton { icon.name: "edit-delete"; onClicked: removeDialog.open(); QQC2.ToolTip.text: "Remove"; QQC2.ToolTip.visible: hovered }
                            }
                            Kirigami.FormLayout {
                                visible: card.expanded
                                Layout.fillWidth: true
                                ColumnLayout {
                                    Kirigami.FormData.label: "Private windows:"
                                    visible: !card.modelData.broken
                                    spacing: 0
                                    QQC2.Switch {
                                        text: "Allow"
                                        enabled: !card.modelData.privateNotAllowed
                                        checked: !!card.modelData.allowPrivate
                                        onToggled: Extensions.setAllowPrivate(card.modelData.id, checked)
                                    }
                                    QQC2.Label {
                                        text: card.modelData.privateNotAllowed ? "This extension doesn't run in private windows." : "When allowed, it can see and change what you do in private windows."
                                        opacity: 0.7
                                        font: Kirigami.Theme.smallFont
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                                QQC2.Label { Kirigami.FormData.label: "Author:"; text: card.modelData.author || "—"; visible: !card.modelData.broken }
                                QQC2.Label { Kirigami.FormData.label: "ID:"; text: card.modelData.id; textFormat: Text.PlainText }
                                QQC2.Label { Kirigami.FormData.label: "Manifest:"; text: "Version " + card.modelData.manifestVersion; visible: !card.modelData.broken }
                                QQC2.Label { Kirigami.FormData.label: "Source:"; text: card.modelData.source === "amo" ? "addons.mozilla.org" : card.modelData.source === "temporary" ? "Temporary (" + card.modelData.path + ")" : "File" }
                                QQC2.Label { Kirigami.FormData.label: "Homepage:"; visible: !!card.modelData.homepage; text: "<a href=\"" + card.modelData.homepage + "\">" + card.modelData.homepage + "</a>"; onLinkActivated: l => w.app.openUrl(l, "tab") }
                                QQC2.Label { Kirigami.FormData.label: "Extension page:"; visible: !!card.modelData.amoSlug; text: "<a href=\"https://addons.mozilla.org/firefox/addon/" + card.modelData.amoSlug + "/\">addons.mozilla.org</a>"; onLinkActivated: l => w.app.openUrl(l, "tab") }
                                ColumnLayout {
                                    Kirigami.FormData.label: "Permissions:"
                                    visible: !card.modelData.broken
                                    Repeater {
                                        model: (card.modelData.warnings || []).length ? card.modelData.warnings : ["No special permissions"]
                                        delegate: QQC2.Label { required property string modelData; text: "• " + modelData }
                                    }
                                }
                                QQC2.Label {
                                    Kirigami.FormData.label: "Not supported:"
                                    visible: (card.modelData.unsupported || []).length > 0
                                    text: (card.modelData.unsupported || []).join(", ")
                                    color: Kirigami.Theme.neutralTextColor
                                    wrapMode: Text.Wrap
                                    Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                                }
                            }
                        }
                        Kirigami.PromptDialog {
                            id: removeDialog
                            title: "Remove " + card.modelData.name + "?"
                            subtitle: "The extension and its data will be removed."
                            standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                            onAccepted: {
                                const u = Extensions.uninstall(card.modelData.id)
                                if (u !== "") w.app.openUrl(u, "tab")
                            }
                        }
                    }
                }
            }
        }
    }

    Component {
        id: browsePage
        Kirigami.ScrollablePage {
            id: bp
            title: "Firefox Add-ons"
            property var results: []
            property int count: 0
            property int pageNo: 1
            property bool busy: false
            property string error: ""
            property var installedIds: JSON.parse(Extensions.list()).map(e => e.id)
            property var detail: null
            function run(p) { pageNo = p || 1; busy = true; error = ""; Extensions.searchAmo(query.text, pageNo, sort.model[sort.currentIndex].k) }
            Component.onCompleted: run(1)
            Connections {
                target: Extensions
                function onAmoResults(json) {
                    const r = JSON.parse(json)
                    bp.busy = false
                    bp.error = r.error || ""
                    bp.results = bp.pageNo > 1 ? bp.results.concat(r.results) : r.results
                    bp.count = r.count || 0
                }
                function onAmoDetail(json) { bp.detail = JSON.parse(json) }
                function onChanged(what) { if (what === "extensions") bp.installedIds = JSON.parse(Extensions.list()).map(e => e.id) }
            }
            actions: [
                Kirigami.Action { text: "Open addons.mozilla.org"; icon.name: "internet-web-browser"; onTriggered: w.app.openUrl("https://addons.mozilla.org/firefox/extensions/", "tab") }
            ]
            header: QQC2.ToolBar {
                RowLayout {
                    anchors.fill: parent
                    Kirigami.SearchField { id: query; Layout.fillWidth: true; placeholderText: "Search addons.mozilla.org, or paste an extension URL"; onAccepted: {
                        if (text.startsWith("https://addons.mozilla.org/")) Extensions.installFromAmo(text); else bp.run(1) } }
                    QQC2.ComboBox {
                        id: sort
                        model: [{k: "relevance", t: "Recommended"}, {k: "users", t: "Most users"}, {k: "rating", t: "Top rated"}, {k: "updated", t: "Recently updated"}, {k: "created", t: "Newest"}]
                        textRole: "t"
                        onActivated: bp.run(1)
                    }
                }
            }
            Kirigami.CardsListView {
                model: bp.results
                Kirigami.PlaceholderMessage {
                    anchors.centerIn: parent
                    visible: !bp.busy && bp.results.length === 0
                    icon.name: bp.error ? "network-disconnect" : "search"
                    text: bp.error ? "Could not reach addons.mozilla.org" : "No extensions found"
                    explanation: bp.error
                }
                QQC2.BusyIndicator { anchors.centerIn: parent; running: bp.busy && bp.results.length === 0; visible: running }
                delegate: Kirigami.AbstractCard {
                    id: rc
                    required property var modelData
                    readonly property bool installed: bp.installedIds.indexOf(modelData.guid) >= 0
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Image {
                            source: rc.modelData.icon_url
                            Layout.preferredWidth: Kirigami.Units.iconSizes.large
                            Layout.preferredHeight: Layout.preferredWidth
                            sourceSize.width: 64; sourceSize.height: 64
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            RowLayout {
                                Kirigami.Heading { level: 4; text: rc.modelData.name; elide: Text.ElideRight; Layout.fillWidth: true; Layout.maximumWidth: implicitWidth }
                                Kirigami.Icon { visible: rc.modelData.recommended; source: "emblem-favorite"; Layout.preferredWidth: Kirigami.Units.iconSizes.small; Layout.preferredHeight: Layout.preferredWidth; QQC2.ToolTip.text: "Recommended by Mozilla"; QQC2.ToolTip.visible: hh.hovered; HoverHandler { id: hh } }
                                Item { Layout.fillWidth: true }
                            }
                            QQC2.Label { text: rc.modelData.summary; wrapMode: Text.Wrap; Layout.fillWidth: true; maximumLineCount: 2; elide: Text.ElideRight }
                            QQC2.Label {
                                font: Kirigami.Theme.smallFont; opacity: 0.7
                                text: (rc.modelData.author ? rc.modelData.author + "  ·  " : "") + "★ " + rc.modelData.rating.toFixed(1) + " (" + rc.modelData.rating_count + ")  ·  " + w.formatUsers(rc.modelData.users) + " users  ·  v" + rc.modelData.version
                            }
                        }
                        ColumnLayout {
                            QQC2.Button {
                                text: rc.installed ? "Installed" : "Add to Kite"
                                icon.name: rc.installed ? "checkmark" : "list-add"
                                enabled: !rc.installed
                                highlighted: !rc.installed
                                onClicked: Extensions.installFromAmo(rc.modelData.slug)
                            }
                            QQC2.Button { text: "Details"; flat: true; onClicked: { bp.detail = null; Extensions.amoDetails(rc.modelData.slug); detailSheet.open() } }
                        }
                    }
                }
                footer: QQC2.Button {
                    visible: bp.results.length > 0 && bp.results.length < bp.count
                    width: parent ? parent.width : 0
                    text: bp.busy ? "Loading…" : "Load More"
                    enabled: !bp.busy
                    onClicked: bp.run(bp.pageNo + 1)
                }
            }
            Kirigami.Dialog {
                id: detailSheet
                title: bp.detail ? (bp.detail.name || "Extension") : "Loading…"
                width: Math.min(w.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 36)
                height: Math.min(w.height - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 30)
                padding: Kirigami.Units.largeSpacing
                standardButtons: Kirigami.Dialog.Close
                customFooterActions: [
                    Kirigami.Action { text: "Add to Kite"; icon.name: "list-add"; enabled: !!bp.detail && !bp.detail.error && bp.installedIds.indexOf(bp.detail.guid) < 0; onTriggered: { Extensions.installFromAmo(bp.detail.slug); detailSheet.close() } },
                    Kirigami.Action { text: "Open on AMO"; icon.name: "internet-web-browser"; enabled: !!bp.detail && !!bp.detail.url; onTriggered: w.app.openUrl(bp.detail.url, "tab") }
                ]
                QQC2.ScrollView {
                    QQC2.Label {
                        width: detailSheet.width - Kirigami.Units.gridUnit * 3
                        wrapMode: Text.Wrap
                        textFormat: Text.StyledText
                        text: !bp.detail ? "" : bp.detail.error ? bp.detail.error
                            : ("<p><b>" + bp.detail.summary + "</b></p>" + (bp.detail.description || "").replace(/\n/g, "<br>")
                               + "<p><b>Permissions:</b> " + (bp.detail.permissions.concat(bp.detail.host_permissions).join(", ") || "none") + "</p>"
                               + "<p>Version " + bp.detail.version + " · " + w.formatUsers(bp.detail.users) + " users · ★ " + bp.detail.rating.toFixed(1) + "</p>")
                        onLinkActivated: l => w.app.openUrl(l, "tab")
                    }
                }
            }
        }
    }
}
