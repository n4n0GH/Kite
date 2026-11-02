import QtQuick
import QtQuick.Dialogs

// "Ask where to save downloads" file dialog.
FileDialog {
    id: dlg
    property var download: null
    signal accepted2(var d)
    fileMode: FileDialog.SaveFile
    title: "Save File"
    function ask(d) {
        download = d
        currentFile = "file://" + d.downloadDirectory + "/" + d.downloadFileName
        open()
    }
    onAccepted: {
        const p = selectedFile.toString().replace("file://", "")
        download.downloadDirectory = p.substring(0, p.lastIndexOf("/"))
        download.downloadFileName = p.substring(p.lastIndexOf("/") + 1)
        accepted2(download)
    }
    onRejected: download.cancel()
}
