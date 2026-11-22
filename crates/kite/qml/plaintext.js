.pragma library
// Text that isn't Kite's own (a page's title, an address, an add-on's name) for an item
// that can't be told its text is plain: a tooltip, a menu entry, a message box. Those
// decide by the text itself whether it is markup, and markup there is drawn (bold, a
// link) and can name a picture, which Kite would then fetch itself, past everything
// that is set for pages. A label is simply given `textFormat: Text.PlainText`.

function escaped(t) {
    return String(t === undefined || t === null ? "" : t).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}

// For a tooltip. What has neither "<" nor "&" in it can't be taken for markup and stays
// as it is. Anything else is made markup that shows the text letter for letter: the tag
// in front says it is markup (escaped text alone may not be taken for it, and would then
// show its "&amp;"s), and the escaping keeps it from being more.
function plain(t) {
    const s = String(t === undefined || t === null ? "" : t)
    return /[<&]/.test(s) ? "<span>" + escaped(s).replace(/\n/g, "<br>") + "</span>" : s
}

// For a menu entry (or a menu's title). As `plain`, and a menu also reads "&" before a
// letter as that letter's shortcut and takes the "&" out, which "&&" keeps it from:
// "AT&T" would else show as "ATT", and the escaping above come apart.
function menuText(t) {
    return plain(t).replace(/&/g, "&&")
}
