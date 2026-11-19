import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// Settings → Parental Controls (kite_core::parental): the parent password, which
// profiles are a child's, and what each of those may see and do.
//
// Everything here takes the parent password first (once one is set). What's changed
// about a profile waits for Apply, like the settings of the other pages.
ColumnLayout {
    id: page
    // The Settings window.
    required property var prefs
    spacing: Kirigami.Units.largeSpacing

    property var st: JSON.parse(Browser.parental())
    // The lists were asked for: their state is read again for a while.
    property int watching: 0
    property string message: ""
    // What's changed and not applied yet: profile → what it's held to (null: not a child's).
    property var edits: ({})
    // The child's profile whose limits are shown.
    property string editing: ""
    signal reloaded()

    function refresh() { st = JSON.parse(Browser.parental()) }
    function profileOf(name) { return st.profiles.find(p => p.name === name) || {} }
    function held(name) { return name in edits ? edits[name] : (profileOf(name).restrictions || null) }
    function isChild(name) { return held(name) !== null }
    function change(name, r) {
        const e = Object.assign({}, edits); e[name] = r; edits = e
        prefs.stage("parental:" + name, () => {
            const error = Browser.parentalSetChild(name, r ? JSON.stringify(r) : "")
            if (error !== "") prefs.app.toast(error, "dialog-error")
        })
    }
    function patch(p) { if (editing !== "" && isChild(editing)) change(editing, Object.assign({}, held(editing), p)) }
    function sites(text) { return text.split("\n").map(s => s.trim()).filter(s => s !== "") }
    function when(ms) { return ms > 0 ? new Date(ms).toLocaleDateString(Qt.locale(), Locale.ShortFormat) : "" }

    readonly property var cur: (editing !== "" && held(editing)) || ({})
    readonly property bool unlocked: !!st.hasPassword && !!st.unlocked

    Component.onCompleted: editing = st.child ? st.profile : ((st.profiles.find(p => p.child) || {}).name || "")
    Connections {
        target: Browser
        function onParentalChanged() {
            page.refresh()
            // (Applied, or changed from a page: what's shown starts over from what's kept.)
            if (!page.prefs.changed) { page.edits = ({}); page.reloaded() }
        }
    }
    Timer { interval: 1500; repeat: true; running: page.watching > 0; onTriggered: { page.watching--; page.refresh() } }

    SettingsHint {
        Layout.maximumWidth: -1
        text: "A child's profile keeps adult sites out, holds search engines to their safe search, and can't be left for another profile without the parent password."
    }

    // ---- no password yet ----
    SettingsSectionHeader { visible: !page.st.hasPassword; text: "Parent Password" }
    SettingsForm {
        visible: !page.st.hasPassword
        Kirigami.PasswordField { id: firstPass; Kirigami.FormData.label: "New password:" }
        Kirigami.PasswordField { id: firstPass2; Kirigami.FormData.label: "Repeat it:"; onAccepted: if (setFirst.enabled) setFirst.clicked() }
        QQC2.Button {
            id: setFirst
            text: "Set Password"
            icon.name: "lock"
            enabled: firstPass.text.length >= page.st.minPassword && firstPass.text === firstPass2.text
            onClicked: { page.message = Browser.parentalSetPassword("", firstPass.text); firstPass.text = ""; firstPass2.text = "" }
        }
        SettingsHint { text: "Parental controls start with a password only the parent knows, of at least " + page.st.minPassword + " characters. Profiles can then be made a child's, here or as they're created. If it's ever forgotten, removing the files parental.json and profiles/.parental in Kite's data folder removes it, with all parental controls." }
    }

    // ---- locked ----
    RowLayout {
        visible: !!page.st.hasPassword && !page.st.unlocked
        spacing: Kirigami.Units.largeSpacing
        Kirigami.Icon { source: "object-locked"; Layout.preferredWidth: Kirigami.Units.iconSizes.medium; Layout.preferredHeight: Layout.preferredWidth }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: page.st.child ? "This is a child's profile. What it may see and do is the parent's to change." : "Parental controls are changed with the parent password."
        }
    }
    SettingsForm {
        visible: !!page.st.hasPassword && !page.st.unlocked
        Kirigami.PasswordField { id: unlockPass; Kirigami.FormData.label: "Parent password:"; onAccepted: unlock.clicked() }
        QQC2.Button {
            id: unlock
            text: "Unlock"
            icon.name: "object-unlocked"
            enabled: unlockPass.text !== ""
            onClicked: { page.message = Browser.parentalUnlock(unlockPass.text); unlockPass.text = "" }
        }
    }
    QQC2.Label { visible: page.message !== ""; text: page.message; color: Kirigami.Theme.negativeTextColor; wrapMode: Text.Wrap; Layout.fillWidth: true }

    // ---- unlocked ----
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: page.unlocked
        type: Kirigami.MessageType.Information
        text: page.st.child ? "Unlocked until Settings is closed. Meanwhile this profile may do what it otherwise may not; blocked sites stay blocked." : "Unlocked until Settings is closed."
        actions: [ Kirigami.Action { text: "Lock"; icon.name: "object-locked"; onTriggered: Browser.parentalLock() } ]
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: page.unlocked && page.st.restored > 0
        type: Kirigami.MessageType.Warning
        text: "On " + new Date(page.st.restored || 0).toLocaleString(Qt.locale(), Locale.ShortFormat) + ", the parental controls were changed or removed outside Kite. Kite put them back as they were."
        actions: [ Kirigami.Action { text: "OK"; icon.name: "dialog-ok"; onTriggered: page.message = Browser.parentalDismissRestored() } ]
    }

    SettingsSectionHeader { visible: page.unlocked; text: "Parent Password" }
    SettingsForm {
        visible: page.unlocked
        QQC2.CheckBox {
            text: "Ask for it before a profile that isn't a child's opens"
            checked: !!page.st.lockUnrestricted
            onToggled: page.message = Browser.parentalSetLockUnrestricted(checked)
        }
        SettingsHint { text: "And before a private session started from outside Kite. Without this, anyone can open the other profiles, and a child's profile holds nobody." }
        QQC2.Button { text: "Change Password…"; icon.name: "lock"; visible: !changeFields.visible; onClicked: changeFields.visible = true }
        ColumnLayout {
            id: changeFields
            visible: false
            Kirigami.PasswordField { id: curPass; placeholderText: "Current password"; Layout.fillWidth: true }
            Kirigami.PasswordField { id: newPass; placeholderText: "New password"; Layout.fillWidth: true }
            Kirigami.PasswordField { id: newPass2; placeholderText: "Repeat it"; Layout.fillWidth: true }
            RowLayout {
                QQC2.Button {
                    text: "Change"
                    enabled: curPass.text !== "" && newPass.text.length >= page.st.minPassword && newPass.text === newPass2.text
                    onClicked: {
                        page.message = Browser.parentalSetPassword(curPass.text, newPass.text)
                        if (page.message === "") { changeFields.visible = false; page.prefs.app.toast("The parent password is changed", "dialog-password") }
                        curPass.text = ""; newPass.text = ""; newPass2.text = ""
                    }
                }
                QQC2.Button { text: "Cancel"; onClicked: { changeFields.visible = false; curPass.text = ""; newPass.text = ""; newPass2.text = "" } }
            }
        }
    }

    SettingsSectionHeader { visible: page.unlocked; text: "Profiles" }
    // (Rows back to back, as a list.)
    ColumnLayout {
        visible: page.unlocked
        Layout.fillWidth: true
        spacing: 0
        Repeater {
            model: page.st.profiles.map(p => p.name)
            delegate: SettingsListRow {
                id: row
                required property string modelData
                readonly property bool child: page.isChild(modelData)
                RowLayout {
                    QQC2.CheckBox {
                        Accessible.name: row.modelData + " is a child's profile"
                        checked: row.child
                        // (The default profile is the parent's.)
                        enabled: !page.profileOf(row.modelData).isDefault
                        onToggled: {
                            page.change(row.modelData, checked ? (page.profileOf(row.modelData).restrictions || page.st.defaults) : null)
                            if (checked) page.editing = row.modelData
                        }
                    }
                    ColumnLayout {
                        spacing: 0
                        Layout.fillWidth: true
                        QQC2.Label { text: row.modelData + (row.modelData === page.st.profile ? " (this profile)" : ""); font.bold: row.child && row.modelData === page.editing; Layout.fillWidth: true; elide: Text.ElideRight }
                        QQC2.Label {
                            text: page.profileOf(row.modelData).isDefault ? "The parent's" : row.child ? "A child's profile" : "Not a child's profile"
                            font: Kirigami.Theme.smallFont; opacity: 0.7
                        }
                    }
                    SettingsRowButton {
                        icon.name: "configure"
                        text: "Show what " + row.modelData + " may see and do"
                        visible: row.child
                        onClicked: page.editing = row.modelData
                    }
                }
            }
        }
    }
    SettingsHint { visible: page.unlocked; Layout.maximumWidth: -1; text: "Tick a profile to make it a child's. A new one is made in the profile picker (Profiles → Manage Profiles…), where it can be a child's from the start." }

    // ---- what the chosen child's profile may see and do ----
    Loader {
        Layout.fillWidth: true
        active: page.unlocked && page.editing !== "" && page.isChild(page.editing)
        visible: active
        sourceComponent: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            SettingsSectionHeader { text: "What “" + page.editing + "” May See" }
            SettingsForm {
                QQC2.CheckBox { id: adult; text: "Block adult sites"; checked: !!page.cur.blockAdult; onToggled: page.patch({blockAdult: checked}) }
                SettingsHint { text: "Sites named on the lists below don't open, nor does anything load from them. Until a list has been downloaded, no site opens." }
                Repeater {
                    model: page.st.lists
                    delegate: ColumnLayout {
                        id: listRow
                        required property var modelData
                        spacing: 0
                        QQC2.CheckBox {
                            text: listRow.modelData.name
                            enabled: adult.checked
                            checked: (page.cur.lists || []).includes(listRow.modelData.id)
                            onToggled: {
                                const now = (page.cur.lists || []).filter(id => id !== listRow.modelData.id)
                                page.patch({lists: checked ? now.concat([listRow.modelData.id]) : now})
                            }
                        }
                        SettingsHint {
                            leftPadding: Kirigami.Units.gridUnit * 1.5
                            text: listRow.modelData.about
                                + (listRow.modelData.error !== "" ? " " + listRow.modelData.error : listRow.modelData.updated > 0 ? " Downloaded " + page.when(listRow.modelData.updated) + "." : "")
                        }
                    }
                }
                RowLayout {
                    // (The lists are this Kite's to fetch for the profile it runs.)
                    visible: !!page.st.child && page.editing === page.st.profile
                    QQC2.Button { text: "Update Lists Now"; icon.name: "view-refresh"; enabled: adult.checked; onClicked: { Browser.parentalUpdateLists(); page.watching = 40 } }
                    QQC2.Label { opacity: 0.7; text: page.st.sites > 0 ? page.st.sites.toLocaleString(Qt.locale(), "f", 0) + " sites listed" : "No list downloaded yet" }
                }
                QQC2.CheckBox { text: "Keep search engines to safe search"; checked: !!page.cur.safeSearch; onToggled: page.patch({safeSearch: checked}) }
                SettingsHint { text: "Google, YouTube, Bing, DuckDuckGo and Ecosia are sent to their safe search that can't be switched off. Search engines without one, such as Startpage, are blocked. Followed from when the profile is opened next." }
                QQC2.CheckBox { text: "Open only the sites allowed below"; checked: !!page.cur.onlyAllowed; onToggled: page.patch({onlyAllowed: checked}) }
                QQC2.TextArea {
                    id: allowedArea
                    Kirigami.FormData.label: "Always allowed:"
                    Kirigami.FormData.labelAlignment: Qt.AlignTop
                    Layout.fillWidth: true
                    // (Three lines at least; it grows with what's in it.)
                    implicitHeight: Math.max(contentHeight + topPadding + bottomPadding, Kirigami.Units.gridUnit * 4)
                    placeholderText: "example.org"
                    wrapMode: TextEdit.NoWrap
                    // (Not bound: setting it again while typing would move the cursor.)
                    function load() { text = ((page.held(page.editing) || {}).allowed || []).join("\n") }
                    Component.onCompleted: load()
                    Connections { target: page; function onEditingChanged() { allowedArea.load() } function onReloaded() { allowedArea.load() } }
                    onTextChanged: if (JSON.stringify(page.sites(text)) !== JSON.stringify((page.held(page.editing) || {}).allowed || [])) page.patch({allowed: page.sites(text)})
                }
                QQC2.TextArea {
                    id: blockedArea
                    Kirigami.FormData.label: "Always blocked:"
                    Kirigami.FormData.labelAlignment: Qt.AlignTop
                    Layout.fillWidth: true
                    // (Three lines at least; it grows with what's in it.)
                    implicitHeight: Math.max(contentHeight + topPadding + bottomPadding, Kirigami.Units.gridUnit * 4)
                    placeholderText: "example.com"
                    wrapMode: TextEdit.NoWrap
                    // (Not bound: setting it again while typing would move the cursor.)
                    function load() { text = ((page.held(page.editing) || {}).blocked || []).join("\n") }
                    Component.onCompleted: load()
                    Connections { target: page; function onEditingChanged() { blockedArea.load() } function onReloaded() { blockedArea.load() } }
                    onTextChanged: if (JSON.stringify(page.sites(text)) !== JSON.stringify((page.held(page.editing) || {}).blocked || [])) page.patch({blocked: page.sites(text)})
                }
                SettingsHint { text: "One site to a line. A name stands for all of its pages and for the names under it: example.com also for news.example.com. A blocked site's page offers to allow it, with the parent password." }
            }

            SettingsSectionHeader { text: "What “" + page.editing + "” May Do" }
            SettingsForm {
                QQC2.CheckBox { text: "Install, remove and switch extensions"; checked: !!page.cur.extensions; onToggled: page.patch({extensions: checked}) }
                QQC2.CheckBox { text: "Install and remove plugins, and switch them on or off"; checked: !!page.cur.plugins; onToggled: page.patch({plugins: checked}) }
                QQC2.CheckBox { text: "Change settings"; checked: !!page.cur.settings; onToggled: page.patch({settings: checked}) }
                SettingsHint { text: "Without it, four pages of Settings stay open: Appearance, Tabs, Site Settings and Shortcuts." }
                QQC2.CheckBox { text: "Open private windows"; checked: !!page.cur.privateWindows; onToggled: page.patch({privateWindows: checked}) }
                SettingsHint { text: "Sites stay blocked in them, but they leave no history." }
                QQC2.CheckBox { text: "Delete history"; checked: !!page.cur.clearHistory; onToggled: page.patch({clearHistory: checked}) }
                SettingsHint { text: "Opening a profile that isn't a child's always takes the parent password, as set above." }
            }
            SettingsHint {
                visible: page.editing !== page.st.profile && !!page.profileOf(page.editing).inUse
                Layout.maximumWidth: -1
                text: "“" + page.editing + "” is open in Kite right now: what's changed here is followed when it's opened next."
            }
        }
    }
}
