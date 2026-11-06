import QtQuick
import org.kde.kite
import "palette.js" as Palette
import "fuzzy.js" as Fuzzy

// Type what you want to do or change: Kite's actions, its settings (switched
// right here if they are switches, otherwise their page opens), tab bar places
// and profiles.
CommandBar {
    id: palette
    placeholder: "Type an action or setting"
    readonly property var app: win.app

    function keysOf(id) {
        const k = app.shortcutKeys(id)
        return k.length ? Fuzzy.keycaps(Browser.keySequenceDisplay(k[0])) : undefined
    }
    function toggle_(t) {
        const now = !app.setting(t.key)
        Browser.setSetting(t.key, JSON.stringify(now))
        win.toast(t.text + ": " + (now ? "on" : "off") + (t.restart ? " (after restarting Kite)" : ""), "configure")
    }

    // Settings make room for actions, unless what was typed only matches
    // those by letters scattered over them.
    readonly property real settingWeight: Palette.SETTING_WEIGHT
    source: () => {
        const out = []
        for (const a of app.shortcutActions) {
            if (/^tab(\d|Last)/.test(a.id)) continue
            const id = a.id
            out.push({prefix: a.group, title: a.name, icon: Palette.ACTION_ICONS[id] || a.icon || "system-run", keys: keysOf(id), run: () => win.runAction(id)})
        }
        out.push({prefix: "Page", title: "Save page as PDF", icon: "application-pdf", run: () => win.savePdf()})
        out.push({prefix: "Windows", title: "Split view", icon: "view-split-left-right", run: () => win.toggleSplit()})
        out.push({prefix: "Help", title: "About Kite", icon: "help-about", run: () => win.showAbout()})
        for (const [where, name] of [["top", "above the address bar"], ["belowToolbar", "below the address bar"], ["bottom", "below the page"], ["vertical", "vertical list on the left"], ["verticalRight", "vertical list on the right"]]) {
            const place = where
            out.push({prefix: "Tab bar", title: name, icon: "tab-duplicate", current: false, checked: win.tabPosition === place, run: () => win.setTabPosition(place)})
        }
        for (const s of JSON.parse(Browser.namedSessions())) {
            const name = s.name
            out.push({prefix: "Sessions", title: "Open " + name, icon: "document-open", detail: s.tabs + (s.tabs === 1 ? " tab" : " tabs"), run: () => app.openNamedSession(name, false)})
            out.push({prefix: "Sessions", title: "Switch to " + name, icon: "document-open", detail: "closes the windows open now", run: () => app.openNamedSession(name, true)})
        }
        const here = Browser.currentProfile()
        for (const p of JSON.parse(Browser.profiles())) {
            if (p.name === here || p.inUse) continue
            const name = p.name
            out.push({prefix: "Profiles", title: "Open " + name, icon: "user-identity", run: () => Browser.openProfile(name)})
        }
        out.push({prefix: "Profiles", title: "Manage profiles", icon: "user-identity", run: () => Browser.manageProfiles()})
        for (const t of Palette.TOGGLES) {
            const item = t
            out.push({prefix: "Settings: " + Palette.PAGE_TITLES[t.page] + (t.prefix ? ", " + t.prefix : ""), title: t.text, weight: settingWeight, icon: "configure", checked: !!app.setting(t.key), run: () => palette.toggle_(item)})
        }
        for (const p of app.pluginsDisabled ? [] : app.plugins) {
            const plugin = p
            const on = app.pluginEnabled(plugin)
            out.push({prefix: "Plugins", title: plugin.name, icon: plugin.icon, checked: on, run: () => {
                app.setPluginEnabled(plugin, !on)
                win.toast(plugin.name + ": " + (on ? "off" : "on"), plugin.icon)
            }})
        }
        for (const c of Palette.CHOICES) {
            const page = c.page
            out.push({prefix: "Settings: " + Palette.PAGE_TITLES[c.page], title: c.text, weight: settingWeight, icon: "configure", run: () => app.showPreferences(page)})
        }
        return out.sort((a, b) => (a.weight === undefined ? 0 : 1) - (b.weight === undefined ? 0 : 1) || (a.prefix + a.title).localeCompare(b.prefix + b.title))
    }
}
