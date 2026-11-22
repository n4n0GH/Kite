.pragma library
// Where a call to the extension API came from, for the hosts of an ExtBridge.

// The page a web view shows, as the engine has it: the one it has loaded, not
// one it is on its way to (`view.url` is that one already). The runtime takes
// a call for an extension page's only when this is that extension's page; from
// any other document the caller is a content script, with the little those may
// call. Nothing a page says of itself counts.
function documentOf(view) {
    const frame = view ? view.mainFrame : null
    return frame && frame.isValid ? frame.url.toString() : ""
}

// The clipboard, asked for through the engine by a page in a view of an extension's own (its
// popup, sidebar, options or background page), where there is nobody to ask. Such a view's
// settings let its pages write, and read if the extension's manifest asks to, so what still
// arrives here is a page that may not: it is told no, or it would wait for good.
function refuseClipboard(permission, clipboardType) {
    if (permission.permissionType === clipboardType) permission.deny()
}

// The extension an address belongs to, as "moz-extension://<id>/"; "" for any other address.
function extensionOf(url) {
    const m = /^moz-extension:\/\/[^\/?#]+/i.exec(String(url))
    return m ? m[0].toLowerCase() + "/" : ""
}

// Whether a view of an extension's own (its popup, sidebar, options or background page,
// made for a page at `home`) may itself go to `url`: only to that extension's pages. Such
// a view has the web channel in its page's own world and may use the clipboard, both of
// which are the extension's; a web page loaded into it would have them.
function staysHome(url, home) {
    const u = String(url)
    return u === "about:blank" || (extensionOf(home) !== "" && extensionOf(u) === extensionOf(home))
}

// Whether a tab may be opened at `url` for an extension's page at `home` that linked to
// it or asked for a window: the web, or that extension's own pages. As for the tabs an
// extension opens through the API (the runtime's tab_url_allowed): a tab Kite opens is
// Kite's own load, which gets Kite's pages and local files where a page's link would not.
function mayOpenFrom(url, home) {
    const u = String(url)
    return /^https?:/i.test(u) || u === "about:blank" || (extensionOf(home) !== "" && extensionOf(u) === extensionOf(home))
}
