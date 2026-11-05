.pragma library
// Vim keys: the rules, without a page or QML (tested with node: keys.test.js).

// What can be bound, in the order Settings shows it.
const COMMANDS = [
    {id: "scrollDown", name: "Scroll down"},
    {id: "scrollUp", name: "Scroll up"},
    {id: "scrollLeft", name: "Scroll left"},
    {id: "scrollRight", name: "Scroll right"},
    {id: "halfPageDown", name: "Half a page down"},
    {id: "halfPageUp", name: "Half a page up"},
    {id: "top", name: "Top of the page"},
    {id: "bottom", name: "Bottom of the page"},
    {id: "hints", name: "Follow a link"},
    {id: "hintsNewTab", name: "Open a link in a new tab"},
    {id: "open", name: "Open an address or search"},
    {id: "openNewTab", name: "Open in a new tab"},
    {id: "tabSearch", name: "Search tabs"},
    {id: "find", name: "Find in page"},
    {id: "findNext", name: "Next match"},
    {id: "findPrevious", name: "Previous match"},
    {id: "back", name: "Back"},
    {id: "forward", name: "Forward"},
    {id: "previousTab", name: "Previous tab"},
    {id: "nextTab", name: "Next tab"},
    {id: "closeTab", name: "Close tab"},
    {id: "reopenTab", name: "Reopen closed tab"},
    {id: "newTab", name: "New tab"},
    {id: "moveTabLeft", name: "Move the tab left"},
    {id: "moveTabRight", name: "Move the tab right"},
    {id: "reload", name: "Reload"},
    {id: "copyUrl", name: "Copy the page's address"},
    {id: "insert", name: "Pass keys to the page until Esc"},
]
// Those the window does, by the same name as its actions (BrowserWindow.runAction).
const WINDOW_ACTIONS = ["back", "forward", "reload", "previousTab", "nextTab", "closeTab", "reopenTab", "newTab", "tabSearch"]

// From command → keys to keys → command; a command without keys is unbound.
function bindings(keys) {
    const out = {}
    for (const c of COMMANDS) if (keys[c.id]) out[keys[c.id]] = c.id
    return out
}

// What pressing `key` does after `pending` was pressed: {run: a command} when a
// binding is complete, {pending} when one is on its way, or {} for neither, when
// the key is the page's. A wrong second key starts over with itself.
function press(map, pending, key) {
    const typed = pending + key
    if (Object.prototype.hasOwnProperty.call(map, typed)) return {run: map[typed]}
    if (Object.keys(map).some(k => k.startsWith(typed))) return {pending: typed}
    return pending ? press(map, "", key) : {}
}

// Bindings that can't all work: the same keys twice, or one that starts another
// (which would then never be reached). As a list of command ids.
function conflicts(keys) {
    const given = COMMANDS.filter(c => keys[c.id])
    return given.filter(c => given.some(o => o !== c && (keys[o.id] === keys[c.id] || keys[o.id].startsWith(keys[c.id]) || keys[c.id].startsWith(keys[o.id])))).map(c => c.id)
}

// As many labels as asked for, all the same length so none starts another. The
// letters' order counts: the first are the easiest to reach.
function labels(count, chars) {
    const letters = Array.from(new Set(String(chars).toLowerCase().split(""))).filter(c => /\S/.test(c))
    if (count <= 0 || letters.length < 2) return []
    let length = 1
    while (Math.pow(letters.length, length) < count) length++
    const out = []
    for (let i = 0; i < count; i++) {
        let n = i, label = ""
        for (let p = 0; p < length; p++) { label += letters[n % letters.length]; n = Math.floor(n / letters.length) }
        out.push(label)
    }
    return out
}

// How fast a held key scrolls, px/s: what's set, within what makes sense.
const SCROLL_SPEEDS = {least: 200, most: 5000, step: 100, normal: 1400}
function scrollSpeed(value) {
    const v = Number(value)
    return Number.isFinite(v) && v > 0 ? Math.max(SCROLL_SPEEDS.least, Math.min(SCROLL_SPEEDS.most, v)) : SCROLL_SPEEDS.normal
}

// The look of hint labels from one color, "#rrggbb": a gradient from a lighter
// shade of it down to it, a darker outline, and text that can be read on it.
const DEFAULT_HINT_COLOR = "#ffc542"
function hintTheme(color) {
    const hex = /^#[0-9a-f]{6}$/i.test(String(color)) ? color : DEFAULT_HINT_COLOR
    const rgb = [1, 3, 5].map(i => parseInt(hex.substring(i, i + 2), 16))
    const mix = (other, part) => "#" + rgb.map((c, i) => Math.round(c + (other[i] - c) * part).toString(16).padStart(2, "0")).join("")
    const white = [255, 255, 255], black = [0, 0, 0]
    const light = (0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]) / 255 > 0.5
    return {
        top: mix(white, 0.5),
        bottom: hex.toLowerCase(),
        outline: mix(black, 0.3),
        text: light ? mix(black, 0.8) : mix(white, 0.92),
        typed: light ? mix(black, 0.4) : mix(white, 0.45),
    }
}

// The site of an address, as it's kept in the list: "" if it has none.
function siteOf(url) {
    const m = /^https?:\/\/(?:[^@/?#]*@)?([^:/?#]+)/i.exec(String(url))
    return m ? m[1].toLowerCase().replace(/^www\./, "") : ""
}

// Is this host one of the sites, or under one of them?
function isExcepted(host, sites) {
    const h = String(host).toLowerCase().replace(/^www\./, "")
    return sites.some(s => h === s || h.endsWith("." + s))
}
