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
    if (!["kite", "file", "data", "view-source", "moz-extension", "about", "blob", "filesystem"].includes(to)) return true
    return to === scheme(pageUrl)
}
