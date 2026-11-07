import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// What the content blocker blocked on pages of a site this session, newest first: from the
// count in the address bar's blocker menu.
ManagerWindow {
    id: w
    property string host: ""
    title: "Blocked on " + host + " — Kite"
    width: Kirigami.Units.gridUnit * 40
    height: Kirigami.Units.gridUnit * 28
    pageStack.initialPage: Kirigami.ScrollablePage {
        id: page
        title: "Blocked on " + w.host
        property var items: []
        property string filter: ""
        readonly property var shown: filter === "" ? items : items.filter(i => i.url.toLowerCase().includes(filter.toLowerCase()))
        function reload() { items = JSON.parse(Browser.adblockBlockedList(w.host)) }
        Component.onCompleted: reload()
        Connections { target: w; function onHostChanged() { page.reload() } }
        // (The page goes on loading, and blocking.)
        Timer { interval: 1000; repeat: true; running: w.visible && w.active; onTriggered: page.reload() }
        function time(ms) { return new Date(ms).toLocaleTimeString(Qt.locale(), Locale.ShortFormat) }
        function kind(k) {
            return {script: "Script", image: "Image", stylesheet: "Style sheet", xmlhttprequest: "Data request", xhr: "Data request", subdocument: "Frame",
                    sub_frame: "Frame", document: "Page", font: "Font", media: "Media", websocket: "WebSocket", ping: "Ping", beacon: "Ping",
                    object: "Plugin content", csp_report: "Report", other: "Other"}[k] || k
        }
        header: QQC2.ToolBar {
            contentItem: RowLayout {
                Kirigami.SearchField {
                    Layout.fillWidth: true
                    placeholderText: "Filter addresses…"
                    onTextChanged: page.filter = text
                }
                QQC2.Label {
                    opacity: 0.75
                    text: page.shown.length === page.items.length ? page.items.length + " blocked" : page.shown.length + " of " + page.items.length
                }
            }
        }
        actions: [
            Kirigami.Action {
                text: "Copy Addresses"
                icon.name: "edit-copy"
                enabled: page.shown.length > 0
                onTriggered: Browser.copyText(page.shown.map(i => i.url).join("\n"))
            },
            Kirigami.Action {
                text: "Clear List"
                icon.name: "edit-clear-list"
                enabled: page.items.length > 0
                onTriggered: { Browser.adblockClearBlocked(w.host); page.reload() }
            }
        ]
        ListView {
            model: page.shown
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 4
                visible: page.shown.length === 0
                icon.name: "cards-block-symbolic"
                text: page.items.length === 0 ? "Nothing blocked on " + w.host + " yet" : "No address matches the filter"
            }
            delegate: QQC2.ItemDelegate {
                required property var modelData
                width: ListView.view.width
                contentItem: ColumnLayout {
                    spacing: 2
                    QQC2.Label { text: modelData.url; elide: Text.ElideMiddle; Layout.fillWidth: true }
                    QQC2.Label {
                        font: Kirigami.Theme.smallFont
                        opacity: 0.75
                        text: page.kind(modelData.kind) + " — " + page.time(modelData.at) + (modelData.replaced ? " — given a stand-in instead" : "")
                    }
                }
                QQC2.ToolTip.text: modelData.url
                QQC2.ToolTip.visible: hovered && modelData.url.length > 80
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: entryMenu.popup()
                QQC2.Menu {
                    id: entryMenu
                    QQC2.MenuItem { text: "Copy Address"; icon.name: "edit-copy"; onTriggered: Browser.copyText(modelData.url) }
                }
            }
        }
    }
}
