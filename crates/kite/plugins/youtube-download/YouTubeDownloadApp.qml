import QtQuick
import org.kde.kite
import "ytdl.js" as Ytdl

// YouTube Download for the whole browser: runs the downloads through the
// yt-dlp-only runner (YtDownloader), so they go on whichever window started
// them (YouTubeDownloadWindow.qml shows them). Turning the plugin off, or
// quitting, stops them.
Item {
    id: root
    property var app
    property var plugin

    // yt-dlp's path ("" when it isn't installed; the button appears only when it
    // is), and whether ffmpeg is there (to join the best video and audio). The
    // runner reports both; checked again when Settings asks, so installing needs
    // no restart.
    property string program: ""
    property bool ffmpeg: false
    function recheck() { engine.checkTools() }

    function folder() { return app.pluginSetting(plugin, "folder") || app.setting("downloads.directory") }

    // The downloads of this session, newest last: {url, title, quality, state
    // ("running" | "done" | "failed" | "stopped"), done, total, part, file, error}.
    // Replaced, not changed in place, so what shows them follows.
    property var downloads: []
    function find(url) {
        for (let i = downloads.length - 1; i >= 0; --i) if (downloads[i].url === url) return downloads[i]
        return null
    }
    function update(url, changes) {
        downloads = downloads.map(d => d.url === url ? Object.assign({}, d, changes) : d)
    }

    // Stops the user asked for, so a stop reads as "stopped", not "failed".
    property var stopping: ({})

    YtDownloader {
        id: engine
        onTools: (found, version, hasFfmpeg) => { root.program = found; root.ffmpeg = hasFfmpeg }
        onProgress: (url, done, total) => {
            const d = root.find(url)
            // (Video, then audio: each counts from the start.)
            if (d) root.update(url, {done: done, total: total, part: done < d.done ? d.part + 1 : d.part})
        }
        onSaved: (url, path) => root.update(url, {file: path})
        onFailed: (url, message) => root.update(url, {error: message})
        onFinished: (url, code) => {
            const d = root.find(url)
            if (!d) return
            const state = code === 0 && d.file ? "done" : root.stopping[url] ? "stopped" : "failed"
            delete root.stopping[url]
            root.update(url, {state: state, error: state === "failed" && !d.error ? "yt-dlp stopped with code " + code + "." : d.error})
            if (state === "done") {
                const file = d.file
                root.app.showNotification("Video saved", file.substring(file.lastIndexOf("/") + 1), "", () => root.showFile(file), null,
                                          [{label: "Show in Folder", run: () => root.showFolder(file)}])
            } else if (state === "failed") {
                root.app.showNotification("Video not saved", d.title + ": " + root.find(url).error, "", null, null, [])
            }
        }
    }
    Component.onCompleted: engine.checkTools()

    // Starts saving the video at `url`; one at a time per video. The runner
    // checks the URL and quality itself (and builds yt-dlp's arguments), so what
    // reaches it here is only ever asked for, never trusted.
    function start(url, title, quality) {
        const old = find(url)
        if (!program || (old && old.state === "running")) return false
        if (!engine.start(url, quality, folder())) return false
        downloads = downloads.filter(d => d.url !== url).concat([{url: url, title: title || url, quality: quality, state: "running", done: 0, total: 0, part: 1, file: "", error: ""}])
        return true
    }
    // Stops a download (what it saved so far stays, as yt-dlp's .part file).
    function cancel(url) { const d = find(url); if (d && d.state === "running") { stopping[url] = true; engine.stop(url) } }
    function forget(url) { const d = find(url); if (!d || d.state !== "running") downloads = downloads.filter(x => x.url !== url) }

    function showFile(path) { Qt.openUrlExternally("file://" + path) }
    function showFolder(path) { Qt.openUrlExternally("file://" + path.substring(0, path.lastIndexOf("/"))) }
}
