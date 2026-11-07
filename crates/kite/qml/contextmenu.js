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
