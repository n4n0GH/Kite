import QtQuick

// The items the navigation toolbar can show ("toolbar.items"), used by
// NavigationToolbar and the Customize Toolbar dialog.
QtObject {
    readonly property var defaults: ["back", "forward", "reload", "home", "urlbar", "extensions", "downloads", "menu"]
    // `required` items can't be removed (Kite would be unusable without them);
    // `repeatable` ones may appear more than once.
    readonly property var catalog: [
        {id: "back", name: "Back", icon: "go-previous"},
        {id: "forward", name: "Forward", icon: "go-next"},
        {id: "reload", name: "Reload / Stop", icon: "view-refresh"},
        {id: "home", name: "Home", icon: "go-home"},
        {id: "urlbar", name: "Address Bar", icon: "globe", required: true},
        {id: "extensions", name: "Extensions", icon: "preferences-plugin"},
        {id: "downloads", name: "Downloads", icon: "download"},
        {id: "menu", name: "Menu", icon: "application-menu", required: true},
        {id: "newTab", name: "New Tab", icon: "tab-new"},
        {id: "newWindow", name: "New Window", icon: "window-new"},
        {id: "privateWindow", name: "New Private Window", icon: "view-private"},
        {id: "bookmarks", name: "Bookmarks", icon: "bookmarks"},
        {id: "history", name: "History", icon: "view-history"},
        {id: "sidebar", name: "Sidebar", icon: "view-sidetree"},
        {id: "print", name: "Print", icon: "document-print"},
        {id: "find", name: "Find in Page", icon: "edit-find"},
        {id: "zoom", name: "Zoom Controls", icon: "zoom-in"},
        {id: "fullScreen", name: "Full Screen", icon: "view-fullscreen"},
        {id: "addons", name: "Add-ons", icon: "preferences-plugin"},
        {id: "settings", name: "Settings", icon: "settings-configure"},
        {id: "flexibleSpace", name: "Flexible Space", icon: "distribute-horizontal-x", repeatable: true},
    ]
    function info(id) { return catalog.find(c => c.id === id) || null }

    // A stored list made usable: unknown ids dropped, duplicates removed
    // (except repeatable items), required items added if missing.
    function sanitize(list) {
        const out = []
        for (const id of (Array.isArray(list) ? list : defaults)) {
            const c = info(id)
            if (c && (c.repeatable || !out.includes(id))) out.push(id)
        }
        if (!out.includes("urlbar")) out.splice(Math.min(out.length, 4), 0, "urlbar")
        if (!out.includes("menu")) out.push("menu")
        return out
    }
}
