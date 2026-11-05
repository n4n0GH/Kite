import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Settings → Privacy → Manage Cookies: what sites have stored, by site.
Kirigami.ScrollablePage {
    id: page
    property var prefs
    title: "Cookies"

    property var sites: []
    property string shownJson: ""
    property var open_: ({})
    function reload() {
        const json = Browser.cookiesBySite()
        if (json !== shownJson) {
            shownJson = json
            sites = JSON.parse(json)
        }
    }
    readonly property var shown: {
        const q = search.text.trim().toLowerCase()
        if (!q) return sites
        return sites.filter(s => s.site.toLowerCase().includes(q) || s.cookies.some(c => c.name.toLowerCase().includes(q) || c.domain.toLowerCase().includes(q)))
    }
    readonly property int total: sites.reduce((n, s) => n + s.cookies.length, 0)
    Component.onCompleted: reload()
    // Sites keep setting and removing them, and removing takes a moment.
    Timer { interval: 1000; repeat: true; running: true; onTriggered: page.reload() }

    function expires(c) {
        if (c.session) return "Until Kite closes"
        return "Until " + new Date(c.expirationDate * 1000).toLocaleDateString(Qt.locale(), Locale.ShortFormat)
    }
    function flags(c) {
        const out = []
        if (c.secure) out.push("Secure")
        if (c.httpOnly) out.push("HttpOnly")
        return out.join(", ")
    }

    actions: [
        Kirigami.Action {
            text: "Remove All…"
            icon.name: "edit-delete"
            enabled: page.total > 0
            onTriggered: removeAll.open()
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Kirigami.SearchField {
            id: search
            Layout.fillWidth: true
            placeholderText: "Search sites and cookies…"
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: "Those of regular windows; private windows forget theirs on closing, and containers keep their own. To always allow or block a site's cookies, use its site settings."
        }
        QQC2.Label {
            visible: page.shown.length === 0
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.gridUnit
            horizontalAlignment: Text.AlignHCenter
            opacity: 0.7
            text: page.total === 0 ? "No cookies stored." : "Nothing found."
        }
        Repeater {
            model: page.shown
            delegate: ColumnLayout {
                id: site
                required property var modelData
                readonly property bool expanded: !!page.open_[modelData.site]
                Layout.fillWidth: true
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    QQC2.ToolButton {
                        icon.name: site.expanded ? "go-down" : "go-next"
                        display: QQC2.AbstractButton.IconOnly
                        text: site.expanded ? "Hide Cookies" : "Show Cookies"
                        onClicked: {
                            const next = Object.assign({}, page.open_)
                            next[site.modelData.site] = !site.expanded
                            page.open_ = next
                        }
                    }
                    QQC2.Label { Layout.fillWidth: true; elide: Text.ElideRight; text: site.modelData.site }
                    QQC2.Label { opacity: 0.7; text: site.modelData.cookies.length + (site.modelData.cookies.length === 1 ? " cookie" : " cookies") }
                    QQC2.ToolButton {
                        icon.name: "configure"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Site Settings"
                        onClicked: page.prefs.openSite(site.modelData.site)
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                    QQC2.ToolButton {
                        icon.name: "edit-delete-remove"
                        display: QQC2.AbstractButton.IconOnly
                        text: "Remove This Site's Cookies"
                        onClicked: Browser.removeSiteCookies(site.modelData.site)
                        QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    }
                }
                Repeater {
                    model: site.expanded ? site.modelData.cookies : []
                    delegate: RowLayout {
                        id: row
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.gridUnit * 2
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            QQC2.Label { Layout.fillWidth: true; elide: Text.ElideRight; text: row.modelData.name }
                            QQC2.Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                font: Kirigami.Theme.smallFont
                                opacity: 0.7
                                text: row.modelData.value
                            }
                            QQC2.Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                font: Kirigami.Theme.smallFont
                                opacity: 0.7
                                text: [row.modelData.domain + row.modelData.path, page.expires(row.modelData), page.flags(row.modelData)].filter(t => t).join(" · ")
                            }
                        }
                        QQC2.ToolButton {
                            icon.name: "edit-delete-remove"
                            display: QQC2.AbstractButton.IconOnly
                            text: "Remove Cookie"
                            onClicked: Browser.removeCookie(JSON.stringify(row.modelData))
                            QQC2.ToolTip.text: text; QQC2.ToolTip.visible: hovered; QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: removeAll
        title: "Remove All Cookies?"
        subtitle: "This signs you out of most sites. Those of private windows and containers stay."
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: "Remove All"
                icon.name: "edit-delete"
                onTriggered: { Browser.removeAllCookies(); removeAll.close() }
            },
            Kirigami.Action { text: "Cancel"; icon.name: "dialog-cancel"; onTriggered: removeAll.close() }
        ]
    }
}
