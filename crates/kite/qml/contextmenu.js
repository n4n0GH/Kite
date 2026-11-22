.pragma library
// What the page context menu (PageContextMenu.qml) makes of what was clicked.

// Is this an image in a link that leads to an image? A thumbnail and the full picture,
// usually, or the same one twice. Then the link's entries are the image's, and the
// thumbnail's own wouldn't be what anyone is after.
function linkLeadsToImage(linkUrl, mediaUrl, isImage) {
    if (!isImage || !linkUrl) return false
    if (linkUrl === mediaUrl) return true
    const path = String(linkUrl).split(/[?#]/)[0]
    return /\.(?:jpe?g|png|gif|webp|avif|bmp|svg|jxl)(?::\w+)?$/i.test(path)
}

// Can a link on this page be opened from the menu (a new tab or window)? That load
// is the browser's own, so it gets past what Chromium refuses a page: a website's
// link may lead to the web or to another app, not to Kite's pages, local files,
// data: documents or scripts. A page of the same kind may link to its own kind
// (Kite's pages to each other, a local file to a local file).
function linkOpenable(linkUrl, pageUrl) {
    const scheme = u => (String(u).match(/^([a-z][a-z0-9+.-]*):/i) || [])[1]?.toLowerCase() || ""
    const to = scheme(linkUrl)
    if (to === "" || to === "javascript") return false
    if (!["kite", "file", "data", "view-source", "moz-extension", "about", "blob", "filesystem", "chrome", "devtools", "qrc"].includes(to)) return true
    return to === scheme(pageUrl)
}

// Can the picture under the pointer be opened in a tab of its own? As a link can, and
// also when the page holds it itself: as data, which then has to be a picture's (a
// document's would be loaded as one, by the browser), or as a blob of that page's site.
function imageOpenable(imageUrl, pageUrl) {
    const u = String(imageUrl)
    if (/^data:/i.test(u)) return /^data:image\/(?!svg)/i.test(u)
    const origin = a => (String(a).match(/^https?:\/\/[^\/?#]+/i) || [""])[0].toLowerCase()
    if (/^blob:/i.test(u)) return origin(pageUrl) !== "" && origin(u.substring(5)) === origin(pageUrl)
    return linkOpenable(u, pageUrl)
}
