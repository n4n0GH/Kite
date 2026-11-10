.pragma library
// YouTube Download: the rules, without QML (tested with node: ytdl.test.js).

// The video a YouTube address shows, as the address yt-dlp gets (that video
// alone, not the playlist it's in), or "" for any other page.
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

// The marks Kite asks yt-dlp to put on the lines it reads (see parse()).
const PROGRESS = "kite-progress"
const SAVED = "kite-saved"

// yt-dlp's arguments for a download of `url` to `folder` in `quality`. Without
// ffmpeg, video and audio can't be joined (or audio converted): it takes the
// best file that has both. `jsRuntime`: a JavaScript runtime other than deno
// (yt-dlp's default) for it to use on YouTube ("node"), or "".
function args(url, folder, quality, ffmpeg, jsRuntime) {
    const a = ["--newline", "--no-playlist", "--progress",
               "--progress-template", "download:" + PROGRESS + " %(progress.downloaded_bytes)s %(progress.total_bytes,progress.total_bytes_estimate)s",
               "--print", "after_move:" + SAVED + " %(filepath)s",
               "-P", folder, "-o", "%(title)s [%(id)s].%(ext)s"]
    if (jsRuntime) a.push("--js-runtimes", jsRuntime)
    const h = /^\d+$/.test(quality) ? quality : ""
    if (quality === "audio") a.push(...(ffmpeg ? ["-f", "ba/b", "-x"] : ["-f", "ba[ext=m4a]/ba/b"]))
    else if (h) a.push("-f", ffmpeg ? "bv*[height<=" + h + "]+ba/b[height<=" + h + "]/b" : "b[height<=" + h + "]/b")
    else if (!ffmpeg) a.push("-f", "b")
    a.push("--", url)
    return a
}

// What a line yt-dlp printed says: {progress: {done, total}} (total 0 when it
// isn't known), {saved: path}, {error: text}, or null.
function parse(line) {
    const s = String(line)
    if (s.startsWith(PROGRESS + " ")) {
        const [done, total] = s.slice(PROGRESS.length + 1).split(" ").map(n => Number(n))
        return {progress: {done: isFinite(done) ? done : 0, total: isFinite(total) ? total : 0}}
    }
    if (s.startsWith(SAVED + " ")) return {saved: s.slice(SAVED.length + 1)}
    if (s.startsWith("ERROR: ")) return {error: s.slice(7).replace(/^\[[^\]]*\]\s*\S+:\s*/, "")}
    return null
}

// "12.3 MB" and the like.
function size(bytes) {
    const units = ["B", "KB", "MB", "GB"]
    let n = Number(bytes) || 0, i = 0
    while (n >= 1000 && i < units.length - 1) { n /= 1000; i++ }
    return (i ? n.toFixed(1) : String(Math.round(n))) + " " + units[i]
}
