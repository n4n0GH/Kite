import QtQuick
import org.kde.kite
import "ytdl.js" as Ytdl

// YouTube Download for the whole browser: finds yt-dlp, and runs the downloads,
// so they go on whichever window started them (YouTubeDownloadWindow.qml shows
// them). Turning the plugin off stops them.
Item {
    id: root
    property var app
    property var plugin

    // yt-dlp, and ffmpeg (which joins the best video and audio), where they are:
    // "" when they aren't installed. Looked up again when Settings shows the
    // plugin's page, or its setting changes, so installing needs no restart.
    property string program: ""
    property string ffmpeg: ""
    // yt-dlp wants a JavaScript runtime for YouTube; deno it finds itself, node
    // it has to be told of.
    property string jsRuntime: ""
    function findPrograms() {
        program = finder.findProgram(app.pluginSetting(plugin, "program") || "yt-dlp")
        ffmpeg = finder.findProgram("ffmpeg")
        jsRuntime = finder.findProgram("deno") === "" && finder.findProgram("node") !== "" ? "node" : ""
    }
    Process { id: finder }
    Component.onCompleted: findPrograms()
    Connections {
        target: Browser
        function onSettingChanged(key) { if (key === "plugins." + root.plugin.id + ".program") root.findPrograms() }
    }

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
    property var processes: ({})

    // Starts saving the video at `url` (Ytdl.videoUrl); one at a time per video.
    function start(url, title, quality) {
        const old = find(url)
        if (!program || (old && old.state === "running")) return false
        const p = processComponent.createObject(root, {url: url})
        if (!p.start(program, JSON.stringify(Ytdl.args(url, folder(), quality, ffmpeg !== "", jsRuntime)))) {
            p.destroy()
            return false
        }
        processes[url] = p
        downloads = downloads.filter(d => d.url !== url).concat([{url: url, title: title || url, quality: quality, state: "running", done: 0, total: 0, part: 1, file: "", error: ""}])
        return true
    }
    function forget(url) { if (!processes[url]) downloads = downloads.filter(d => d.url !== url) }

    function showFile(path) { Qt.openUrlExternally("file://" + path) }
    function showFolder(path) { Qt.openUrlExternally("file://" + path.substring(0, path.lastIndexOf("/"))) }

    Component {
        id: processComponent
        Process {
            id: proc
            property string url
            property bool stopping: false
            onOutput: line => {
                const m = Ytdl.parse(line)
                if (!m) return
                const d = root.find(url)
                if (!d) return
                // (Video, then audio: each from the start.)
                if (m.progress) root.update(url, {done: m.progress.done, total: m.progress.total, part: m.progress.done < d.done ? d.part + 1 : d.part})
                else if (m.saved) root.update(url, {file: m.saved})
                else if (m.error) root.update(url, {error: m.error})
            }
            function stopNow() { stopping = true; stop() }
            onFinished: code => {
                delete root.processes[url]
                const d = root.find(url)
                if (d) {
                    const state = code === 0 && d.file ? "done" : stopping ? "stopped" : "failed"
                    root.update(url, {state: state, error: state === "failed" && !d.error ? "yt-dlp stopped with code " + code + "." : d.error})
                    if (state === "done") {
                        const file = d.file
                        root.app.showNotification("Video saved", file.substring(file.lastIndexOf("/") + 1), "", () => root.showFile(file), null,
                                                  [{label: "Show in Folder", run: () => root.showFolder(file)}])
                    } else if (state === "failed") {
                        root.app.showNotification("Video not saved", d.title + ": " + root.find(url).error, "", null, null, [])
                    }
                }
                proc.destroy()
            }
        }
    }
    // Stops a download (what it saved so far stays, as yt-dlp's .part file).
    function cancel(url) { if (processes[url]) processes[url].stopNow() }
}
