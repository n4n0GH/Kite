import QtCore
import QtQuick
import QtQuick.Dialogs
import QtWebEngine
import org.kde.kite

// A page's file input. Qt's own dialog for these can't be told where to open:
// KDE's then picks Documents, or the folder Kite was started from.
QtObject {
    id: root
    required property var request
    property Window parentWindow

    readonly property url home: StandardPaths.writableLocation(StandardPaths.HomeLocation)

    function finish(files) {
        if (files) request.dialogAccept(files)
        else request.dialogReject()
        Qt.callLater(root.destroy)
    }

    property FileDialog _files: FileDialog {
        parentWindow: root.parentWindow
        title: root.request.mode === FileDialogRequest.FileModeSave ? "Save File" : root.request.mode === FileDialogRequest.FileModeOpenMultiple ? "Open Files" : "Open File"
        fileMode: root.request.mode === FileDialogRequest.FileModeSave ? FileDialog.SaveFile : root.request.mode === FileDialogRequest.FileModeOpenMultiple ? FileDialog.OpenFiles : FileDialog.OpenFile
        nameFilters: JSON.parse(Browser.fileFilters(JSON.stringify(Array.from(root.request.acceptedMimeTypes))))
        currentFolder: root.home
        onAccepted: root.finish(Array.from(selectedFiles).map(f => f.toString()))
        onRejected: root.finish(null)
    }
    property FolderDialog _folder: FolderDialog {
        parentWindow: root.parentWindow
        title: "Upload Folder"
        currentFolder: root.home
        onAccepted: root.finish([selectedFolder.toString()])
        onRejected: root.finish(null)
    }

    Component.onCompleted: {
        if (request.mode === FileDialogRequest.FileModeUploadFolder) {
            _folder.open()
            return
        }
        if (request.mode === FileDialogRequest.FileModeSave && request.defaultFileName !== "")
            _files.currentFile = home + "/" + encodeURIComponent(request.defaultFileName)
        _files.open()
    }
}
