import QtQuick
import org.kde.kite

// The tab list, searchable: the open tabs of all windows (normal ones or
// private ones, not mixed), this window's first, by title and address.
CommandBar {
    id: search
    placeholder: "Search tabs"
    source: () => {
        const out = []
        const windows = [win].concat(win.app.windows.filter(w => w !== win && w.privateMode === win.privateMode))
        for (const w of windows) {
            for (let i = 0; i < w.tabs.count; ++i) {
                const r = w.tabs.get(i)
                const id = r.tabId, from = w
                const badges = []
                if (r.audible) badges.push("audio-volume-high")
                if (r.pinned) badges.push("window-pin")
                out.push({
                    title: r.title, icon: r.icon, extra: r.url, badges: badges,
                    detail: from !== win ? "Other window" : Browser.hostOf(r.url),
                    current: from === win && win.currentTab && id === win.currentTab.tabId,
                    run: () => { const t = from.tabById(id); if (t) t.activate() },
                    close: () => { const t = from.tabById(id); if (t) t.close() },
                })
            }
        }
        out.push({title: "Reopen Closed Tab", icon: "edit-undo", run: () => win.reopenClosedTab(0)})
        return out
    }
}
