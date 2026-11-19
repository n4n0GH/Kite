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
