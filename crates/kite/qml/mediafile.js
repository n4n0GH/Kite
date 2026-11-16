.pragma library
// File names for what the Page Info window saves from a page's Media list
// (PageInfoWindow.qml, MediaSaver.qml).

const EXTENSIONS = {
    "image/png": "png", "image/jpeg": "jpg", "image/gif": "gif", "image/webp": "webp", "image/avif": "avif", "image/svg+xml": "svg",
    "image/bmp": "bmp", "image/x-icon": "ico", "image/vnd.microsoft.icon": "ico", "image/jxl": "jxl",
    "video/webm": "webm", "video/mp4": "mp4", "video/ogg": "ogv", "audio/mpeg": "mp3", "audio/ogg": "ogg", "audio/wav": "wav", "audio/webm": "weba",
}

// What can be saved: what any view of the profile can fetch. (A blob: address
// is only the page's own.)
function savable(url) { return /^(https?|data):/i.test(String(url)) }

// A name for the file of `url`: the last part of its path, made safe for a
// folder (the address is the page's to write: no folders, no hidden file, no
// characters a file system minds), or its `kind` ("image") where it has none.
function fileName(url, kind) {
    url = String(url)
    let name = ""
    let extension = ""
    const data = /^data:([a-z0-9.+-]+\/[a-z0-9.+-]+)?[;,]/i.exec(url)
    if (data) {
        extension = EXTENSIONS[(data[1] || "").toLowerCase()] || ""
    } else {
        const last = url.replace(/[?#].*$/, "").replace(/\/+$/, "").replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]*/i, "").replace(/^.*\//, "")
        try { name = decodeURIComponent(last) } catch (e) { name = last }
    }
    name = name.replace(/[\/\\:*?"<>|\u0000-\u001f\u007f]/g, "_").replace(/^[.\s]+/, "").replace(/[.\s]+$/, "")
    if (name.length > 120) {
        const dot = name.lastIndexOf(".")
        const end = dot > 0 && name.length - dot <= 8 ? name.slice(dot) : ""
        name = name.slice(0, 120 - end.length) + end
    }
    if (name === "") name = (String(kind || "").replace(/[^a-z]/gi, "") || "file") + (extension ? "." + extension : "")
    return name
}
