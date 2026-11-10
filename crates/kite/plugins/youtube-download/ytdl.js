.pragma library
// YouTube Download: the rules, without QML (tested with node: ytdl.test.js).

// Whether a YouTube address shows a single video (so the button appears): its
// canonical address, or "" otherwise. The runner (ytdlp.rs) checks this again
// in Rust before anything runs; this is only for the UI.
function videoUrl(url) {
    const s = String(url)
    const id = /^https?:\/\/(?:www\.|m\.|music\.)?youtube\.com\/watch\?(?:[^#]*&)?v=([\w-]{11})(?![\w-])/i.exec(s)
        || /^https?:\/\/(?:www\.|m\.)?youtube\.com\/(?:shorts|live)\/([\w-]{11})(?![\w-])/i.exec(s)
        || /^https?:\/\/youtu\.be\/([\w-]{11})(?![\w-])/i.exec(s)
    return id ? "https://www.youtube.com/watch?v=" + id[1] : ""
}

// The video's name from its tab's title ("Name - YouTube").
function titleOf(tabTitle) {
    return String(tabTitle || "").replace(/^\(\d+\)\s*/, "").replace(/\s+-\s+YouTube$/, "").trim()
}

// What can be chosen, in Settings and before a download.
const QUALITIES = [
    {k: "best", t: "Best available"},
    {k: "1080", t: "Up to 1080p"},
    {k: "720", t: "Up to 720p"},
    {k: "480", t: "Up to 480p"},
    {k: "audio", t: "Audio only"},
]

// "12.3 MB" and the like.
function size(bytes) {
    const units = ["B", "KB", "MB", "GB"]
    let n = Number(bytes) || 0, i = 0
    while (n >= 1000 && i < units.length - 1) { n /= 1000; i++ }
    return (i ? n.toFixed(1) : String(Math.round(n))) + " " + units[i]
}
