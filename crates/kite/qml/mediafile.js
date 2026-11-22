.pragma library
// File names for what the Page Info window saves from a page's Media list
// (PageInfoWindow.qml, MediaSaver.qml), and for the file a tab shows in place
// of a page (Save Page As).

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

// ---- the file a tab shows (Save Page As, BrowserWindow.savePage) ----

// Is a document of this type (`document.contentType`) a page, to be saved as
// one? Anything else a tab shows is a file, saved as it is: a PDF document, a
// picture, a video, text. (No type known: a page.)
function isPage(type) {
    type = String(type || "").toLowerCase()
    return type === "" || type === "text/html" || type === "application/xhtml+xml"
}

// The text the engine can save as a page, and so names like one ("notes.mhtml"
// for notes.txt): what a file of each is called, where the address doesn't say.
const TEXT_EXTENSIONS = {"text/plain": "txt", "text/xml": "xml", "text/css": "css"}

// A name for the file a tab shows. `engineName`: the name the engine suggests
// for it. For what it downloads (`asPage` false) that is the one: as the server
// named the file, or the address does, with an extension for its type. What it
// would save as a page (`asPage`) gets the address's own name, with an
// extension for its type if it has none.
function documentName(url, type, engineName, asPage) {
    engineName = String(engineName || "")
    if (!asPage && engineName !== "") return engineName
    type = String(type || "").toLowerCase()
    let name = fileName(url, "")
    // (An address without a name: the engine's, which is the site's then.)
    const nameless = name === "file"
    if (nameless) name = engineName.replace(/\.mhtml?$/i, "") || "file"
    if (nameless || name.indexOf(".") < 0) name += "." + (TEXT_EXTENSIONS[type] || (/(java|ecma)script/.test(type) ? "js" : "txt"))
    return name
}
