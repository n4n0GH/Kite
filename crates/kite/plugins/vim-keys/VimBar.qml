import QtQuick
import org.kde.kite

// Vim keys' `o` and `O`: open an address or search, in the look of the tab search.
// What it offers is what the address bar would, in the same order, by address.
CommandBar {
    id: bar
    property bool newTab: false
    property var completions: []
    property var suggestions: []
    placeholder: newTab ? "Open in a new tab, or search" : "Open an address, or search (Shift+Enter: new tab)"
    emptyText: ""
    keepOrder: true

    function openIt(input) { win.locationBar.go(input, newTab || pickedWithShift ? "tab" : "") }
    source: () => {
        const t = query.trim()
        if (t === "") return []
        const out = [{title: t, icon: "search", detail: "Go or search", first: true, run: () => bar.openIt(t)}]
        for (const c of completions) {
            const url = c.url
            out.push({title: Browser.displayUrl(url), icon: c.type === "bookmark" ? "bookmarks" : "view-history", detail: c.title, run: () => bar.openIt(url)})
        }
        for (const s of suggestions) {
            const term = s
            if (term !== t) out.push({title: term, icon: "search", run: () => bar.openIt("? " + term)})
        }
        return out
    }
    onQueryChanged: {
        const t = query.trim()
        completions = t ? JSON.parse(Browser.complete(t, win.privateMode)) : []
        suggestions = []
        // (What is typed in a private window stays here. Nor an address for
        // another program.)
        if (t && !win.privateMode && !Browser.isExternalUrl(t)) Browser.requestSuggestions(t)
        refresh()
    }
    Connections {
        target: Browser
        function onSuggestionsReady(q, json) {
            if (!bar.opened || q !== bar.query.trim()) return
            bar.suggestions = JSON.parse(json)
            bar.refresh()
        }
    }
}
