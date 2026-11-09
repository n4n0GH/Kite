import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kite

// The last crash's report (crash.rs, kite_core::crashlog): shown at the start
// after a crash ("general.crashReportNotice"), and from Help → Last Crash Report.
// It stays on this computer; Copy and Open Folder are for passing it on.
Kirigami.Dialog {
    id: dlg
    property string report: ""
    property bool showText: false

    // The report's first lines: when, and what happened.
    readonly property var lines: report.split("\n")
    function field(name) {
        const l = lines.slice(0, 12).find(l => l.startsWith(name + ": "))
        return l ? l.substring(name.length + 2) : ""
    }
    readonly property string when: lines.length && lines[0].startsWith("Crash report of Kite, ") ? lines[0].substring(22) : ""
    readonly property bool survived: report.includes("(Kite went on running after this and was closed normally.)")
    readonly property string what: field("signal") ? "The crash came from signal " + field("signal") + ", in Kite or the libraries it uses (Qt, QtWebEngine)."
        : field("message") ? "Kite's own code failed: “" + field("message") + "” at " + field("at") + "."
        : ""

    function show(expanded) {
        report = Browser.crashReport()
        showText = expanded
        open()
    }

    title: survived ? "Kite Ran Into an Error" : "Kite Crashed Last Time"
    padding: Kirigami.Units.largeSpacing
    preferredWidth: Kirigami.Units.gridUnit * (showText ? 46 : 28)
    standardButtons: Kirigami.Dialog.Close
    customFooterActions: [
        Kirigami.Action {
            text: dlg.showText ? "Hide Report" : "View Report"
            icon.name: "document-preview"
            onTriggered: dlg.showText = !dlg.showText
        },
        Kirigami.Action {
            text: "Copy"
            icon.name: "edit-copy"
            onTriggered: Browser.copyText(dlg.report)
        },
        Kirigami.Action {
            text: "Open Folder"
            icon.name: "folder-open"
            onTriggered: Qt.openUrlExternally(Browser.logFolderUrl())
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dlg.report === "" ? "There's no crash report."
                : (dlg.survived ? "Kite kept running" + (dlg.when ? " (" + dlg.when + ")" : "") + ", but part of it failed. "
                                : "Kite closed unexpectedly" + (dlg.when ? " on " + dlg.when : "") + ". ")
                  + dlg.what
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dlg.report !== ""
            wrapMode: Text.Wrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: "The report has what happened and the end of that run's log. It stays on this computer; addresses from private windows are scrambled in it."
        }
        QQC2.ScrollView {
            visible: dlg.showText && dlg.report !== ""
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 22
            QQC2.TextArea {
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.NoWrap
                font.family: "monospace"
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                text: dlg.report
            }
        }
        QQC2.CheckBox {
            text: "Show the report after a crash"
            checked: !!JSON.parse(Browser.setting("general.crashReportNotice"))
            onToggled: Browser.setSetting("general.crashReportNotice", JSON.stringify(checked))
        }
    }
}
