import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts
import QtWebEngine
import org.kde.kirigami as Kirigami
import org.kde.kite

// A page's file input. Qt's own dialog for these can't be told where to open:
// KDE's then picks Documents, or the folder Kite was started from.
QtObject {
    id: root
    required property var request
    required property var app
    // The tab's site. (The request doesn't say which frame asked.)
    required property string site
    property Window parentWindow

    readonly property string startFolder: app.fileFolder()
    readonly property bool inPrivate: !!parentWindow && parentWindow.privateMode

    function finish(files, folder) {
        if (files) {
            app.rememberFileFolder(folder, inPrivate)
            // (Paths: from a URL, Qt reads names with # or < in them wrong, and the page
            // gets an empty file.)
            request.dialogAccept(files.map(f => Browser.localPath(f)))
        } else {
            request.dialogReject()
        }
        Qt.callLater(root.destroy)
    }

    property FileDialog _files: FileDialog {
        parentWindow: root.parentWindow
        title: root.request.mode === FileDialogRequest.FileModeSave ? "Save File" : root.request.mode === FileDialogRequest.FileModeOpenMultiple ? "Open Files" : "Open File"
        fileMode: root.request.mode === FileDialogRequest.FileModeSave ? FileDialog.SaveFile : root.request.mode === FileDialogRequest.FileModeOpenMultiple ? FileDialog.OpenFiles : FileDialog.OpenFile
        nameFilters: JSON.parse(Browser.fileFilters(JSON.stringify(Array.from(root.request.acceptedMimeTypes))))
        currentFolder: root.startFolder
        onAccepted: {
            const files = Array.from(selectedFiles).map(f => f.toString())
            root.finish(files, files[0].substring(0, files[0].lastIndexOf("/")))
        }
        onRejected: root.finish(null)
    }
    property FolderDialog _folder: FolderDialog {
        parentWindow: root.parentWindow
        title: "Upload Folder"
        currentFolder: root.startFolder
        onAccepted: {
            const folder = selectedFolder.toString()
            const count = Browser.folderFileCount(folder)
            if (count === 0) root.finish([folder], folder)
            else root._confirm.ask(folder, count)
        }
        onRejected: root.finish(null)
    }

    // A whole folder is easily more than meant, and a page gets all of it at once.
    property Window _confirm: Window {
        id: confirm
        property string folder: ""
        property int count: 0
        // (A page's site and a folder's name are shown as plain text, never as markup.)
        readonly property string path: Browser.localPath(folder)
        readonly property string countText: count > 10000 ? "more than 10,000 files" : count === 1 ? "1 file" : count.toLocaleString(Qt.locale("en_US"), "f", 0) + " files"
        transientParent: root.parentWindow
        flags: Qt.Dialog
        modality: Qt.WindowModal
        title: "Upload Folder"
        width: Kirigami.Units.gridUnit * 26
        height: confirmLayout.implicitHeight + Kirigami.Units.gridUnit * 2
        color: Kirigami.Theme.backgroundColor
        function ask(f, n) { folder = f; count = n; visible = true; raise(); requestActivate(); cancelButton.forceActiveFocus() }
        function answer(ok) {
            if (folder === "") return
            const f = folder
            folder = ""
            visible = false
            if (ok) root.finish([f], f)
            else root.finish(null)
        }
        onClosing: answer(false)
        Shortcut { sequence: "Esc"; onActivated: confirm.answer(false) }
        ColumnLayout {
            id: confirmLayout
            anchors { fill: parent; margins: Kirigami.Units.gridUnit }
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Heading {
                level: 3
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: "Upload " + confirm.countText + " to this page?"
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: "Everything in this folder and its subfolders goes to " + (root.site || "this page") + ":"
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
                font.bold: true
                text: confirm.path
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: "Only do this if you trust the site."
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                QQC2.Button { text: "Upload"; icon.name: "document-send"; onClicked: confirm.answer(true) }
                QQC2.Button { id: cancelButton; text: "Cancel"; icon.name: "dialog-cancel"; highlighted: true; onClicked: confirm.answer(false); Keys.onReturnPressed: confirm.answer(false); Keys.onEnterPressed: confirm.answer(false) }
            }
        }
    }

    Component.onCompleted: {
        if (request.mode === FileDialogRequest.FileModeUploadFolder) {
            _folder.open()
            return
        }
        if (request.mode === FileDialogRequest.FileModeSave && request.defaultFileName !== "")
            _files.currentFile = startFolder + "/" + encodeURIComponent(request.defaultFileName)
        _files.open()
    }
}
