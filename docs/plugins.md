# Native plugins

Plugins are optional features that run as part of Kite. Mouse Gestures, Vim Keys,
Picture-in-Picture, Plasma Integration and Reader View are plugins that ship with Kite, in `crates/kite/plugins/`. Anyone can write one and
hand it out as a file, and users install it with **Settings → Plugins & Extensions → Install Plugin
from File…**. Users switch plugins on and off there. The toolbar's extensions panel lists those that are on, with a button to their settings.

## Plugins or WebExtensions?

| | Native plugin | WebExtension |
|---|---|---|
| Installed | Compiled in, or from a file | From addons.mozilla.org or a file |
| Can use | Anything in Kite, including QML and internal functions | The `browser.*` API |
| Runs in | Kite's process, **fully trusted** | Isolated pages and worlds, limited by its permissions |
| Compatibility | None promised, internals can change | A stable API |

Prefer a WebExtension when one can do the job. A plugin is for what needs Kite itself, such as its
windows and shortcuts.

## Security

**Nothing limits a plugin once it's on.** It can read saved logins, cookies and history, and see and
change every page, including in private windows. There's no sandbox and no signing, and no review of
what's installed. Installing one is like installing a program, and the install dialog says so.

Kite only makes sure that nothing of a plugin runs before the user agrees:

| Rule | Why |
|---|---|
| Nothing runs until the user turns it on, whatever `defaultEnabled` says | Installing and running are two decisions |
| A file is read and checked before the dialog, and what gets installed is what was read | The file can't change after it was shown |
| Its name, description, version and action names can't contain `<` or control characters | Qt labels render what looks like markup, and an image in there would load just by showing it |
| `icon` has to be an icon theme's name | An address would be fetched just by listing it |
| An archive can't write outside its folder, or create links | Unpacking stays in the plugin's folder |
| It can't take the id of a plugin that ships with Kite | It can't pass for one |

## What a plugin is made of

```
crates/kite/plugins/<id>/
    plugin.json        the manifest, required
    SomeWindow.qml     optional: runs once per browser window while it's on
    SomeSettings.qml   optional: its page in Settings
    page.js            optional: runs in web pages while it's on
    helpers.js         optional: plain logic, ideally with helpers.test.js beside it
```

A plugin can use any of these, and the shipped plugins use most of them. You only need
what your feature needs: a manifest alone is valid, although it doesn't do anything.

## The manifest

| Field | Required | Meaning |
|---|---|---|
| `id` | Yes | The folder's name. Only `a-z`, `0-9` and `-` |
| `name`, `description`, `icon` | Yes | What Settings shows, as plain text. `icon` is an icon theme name |
| `version` | No | Shown when installing |
| `defaultEnabled` | No | On for users who haven't chosen. Default `false` |
| `window` | No | The QML item made once per browser window |
| `app` | No | The QML item made once for the whole browser |
| `settingsPage` | No | The QML item shown under its **Configure** button |
| `actions` | No | Actions with optional shortcuts. They need `window` |
| `pageScripts` | No | Scripts for web pages |
| `settings` | No | Its settings and their defaults, any JSON. `enabled` isn't allowed |

Paths have the form `plugins/<id>/…`, and must be inside the plugin's folder.

**An action** has `id` (`<plugin id>.<name>`), `group` (the heading in Settings → Shortcuts),
`name`, `keys` (a list, can be empty) and `icon`.

**A page script** has these fields:

| Field | Meaning |
|---|---|
| `files` | One or more files, joined in order into one script |
| `worldId` | Which world it runs in. Use `1`, Kite's own. `0` is the page's own, for what only exists there, like `navigator.mediaSession`: the page can reach it, so it gets neither `KITE_PLUGIN_SETTINGS` nor `kitePluginSend` |
| `runsOnSubFrames` | Whether it also runs in frames |
| `injectionPoint` | **Kite's own numbering**: `0` as the document is created, `1` when it's ready, anything else later. Not the numbering of Qt's enum |

`kite_core::plugins` checks all of this when the manifest loads, and a test makes it fail the test
run if it's wrong.

## What each part gets

### The app item

For what isn't any one window's. Kite creates it once while the plugin is on, and destroys it when the
plugin is turned off. It's given `app` and `plugin`, as the window item is below. `app` tells it about
downloads with `downloadBegan(id, download, inPrivate)`, `downloadProgress(id, download)`,
`downloadPausedChanged(id, download)` and `downloadEnded(id, download, state)`. To tell the user about a
download itself, instead of Kite's notification, it puts the id in `app.claimedDownloads` when it begins.

### The window item

Kite creates it once per browser window while the plugin is on, and destroys it when the plugin is
turned off, so `Component.onDestruction` is where to tidy up. It has no size and sits in no layout, so
it can't cover the page. Anything it shows, it has to put up itself. It's given three properties:

| Property | What it is |
|---|---|
| `win` | This `BrowserWindow`: `currentTab`, `openTab`, `toast`, `runAction`, `tabById`… |
| `app` | `Main`: `pluginSetting`, `showPreferences`, `shortcutActions`… |
| `plugin` | The manifest, as a JavaScript object |

If the plugin has actions, this item needs a `runAction(id)` function.

To add to a page's context menu, give it a `contextItems(info)` function. `info` says what the menu was
opened on (`linkUrl`, `srcUrl`, `mediaType`, `selectionText`, `editable`, `pageUrl`), and it returns a list
of `{title, iconName, run}` or `{title, iconName, children}`.

`win.tabOpened(tab)` tells it when a tab is added (those already open are in `win.tabItems`), and
`win.tabClosed(tabId)` when one closes. What it shows over a tab's page goes in `tab.pageArea`.

### The settings page

It's given `app` and `plugin`, and is shown under the **Configure** button:

| Call | Does |
|---|---|
| `app.pluginSetting(plugin, "key")` | Reads a setting, falling back to the default in the manifest |
| `app.setPluginSetting(plugin, "key", value)` | Stores it |

They're stored as `plugins.<id>.<key>`, and whether it's on as `plugins.<id>.enabled`, with everything
else in the settings file. To react to a change, connect to `Browser.settingChanged`.

### Page scripts

Each one gets two things that nothing else has:

| Name | What it is |
|---|---|
| `KITE_PLUGIN_SETTINGS` | The plugin's settings **as they were when the page loaded** |
| `kitePluginSend(message)` | Sends any JSON-compatible value to the window |

The script is wrapped in a function, so its variables are private. To be callable from QML, it has to
put something on `globalThis`. World `1` is shared with Kite's other scripts, so use a name that won't
clash, starting with `__kite`.

Scripts apply to pages loaded **after** the plugin is switched on or a setting changes. Tabs already
open need a reload, unless the plugin handles them itself (Vim Keys does).

### Talking between them

| Direction | How |
|---|---|
| Page script to window | `kitePluginSend(m)`, which arrives as `win.pluginMessage(tab, {plugin, m})`. Every plugin's window item hears every message, so **check `message.plugin`** first |
| Window to page script | `tab.runInMainFrame(code)`, or `tab.runInFrames(code, (frame, result) => …)` for all frames with their results |

**Page content is untrusted.** The page can't forge the messages, but what your script reads from it
can be anything, and it can imitate whatever a world `0` script tells yours through the DOM. Treat it as text. Never put it into code you run, or into markup.

## Handing out a plugin

Zip the plugin's folder, or what's in it:

```sh
cd crates/kite/plugins/word-count && zip -r ../../../../word-count.zip .
```

The result is what users pick in **Install Plugin from File…**. It's unpacked to
`~/.local/share/kite/plugins/<id>/`, shared by all profiles, and stays off until the user turns it on.
Whether it's on, and its settings, are per profile.

The plugin's folder needs nothing else. Keep the paths in `plugin.json` the same
(`plugins/<id>/…`), and sibling files and `.js` imports work as usual. It can `import org.kde.kite` for
the singletons. Add a `version` to the manifest, since the dialog shows it.

**To update it,** install a newer file with the same `id`. Its files are replaced and its settings
stay. The new version runs after restarting Kite, because Qt keeps what it has loaded.

**To remove it,** use its **Remove…** button. That deletes it for all profiles, and its settings in the
current profile.

**If a plugin causes trouble,** start Kite with `kite --no-plugins`. No plugin runs in that session,
shipped or installed, and what's stored about them doesn't change. They stay listed in Settings, where the
ones you installed can be removed. One with invalid QML only logs an error. One with a broken manifest is
skipped, with the reason in the log.

**Kite makes no compatibility promise.** A plugin uses Kite's internals, so a new version of Kite can
break it. Test yours against each release.

## Shipping a plugin with Kite

Only for plugins that are part of Kite itself. Its files are compiled in, so two lists in
`crates/kite/src/browser.rs` have to know it:

| List | Add |
|---|---|
| `builtin_plugins()` | Its `plugin.json`. Always |
| `PLUGIN_FILES` | Each page script file. Only if it has any |

The test `plugins_are_valid_and_complete` fails if you forget either, or if a named file doesn't exist.
QML files are picked up by `build.rs` by themselves. **Their names must be unique across all of Kite**,
so start them with the plugin's name. (An installed plugin's files aren't part of Kite's QML module, so
this only applies here.)

## Example: Word Count

This plugin counts the words on the current page and estimates a reading time. It shows every part:
an action with a shortcut, a page script, a message back, a setting and a settings page.

Make a folder called `word-count` with these four files.

**`plugins/word-count/plugin.json`**

```json
{
    "id": "word-count",
    "name": "Word Count",
    "description": "Counts the words on a page and estimates how long it takes to read, with Ctrl+Shift+L.",
    "icon": "accessories-text-editor",
    "defaultEnabled": false,
    "window": "plugins/word-count/WordCountWindow.qml",
    "settingsPage": "plugins/word-count/WordCountSettings.qml",
    "actions": [
        {"id": "word-count.show", "group": "Page", "name": "Count words on this page", "keys": ["Ctrl+Shift+L"], "icon": "accessories-text-editor"}
    ],
    "pageScripts": [
        {
            "files": ["plugins/word-count/page.js"],
            "worldId": 1,
            "runsOnSubFrames": false,
            "injectionPoint": 0
        }
    ],
    "settings": {
        "wordsPerMinute": 200
    }
}
```

**`plugins/word-count/page.js`** does the counting, because only a script in the page can read it.

```js
globalThis.__kiteWordCount = () => {
    const text = document.body ? document.body.innerText : ""
    const words = text.split(/\s+/).filter(w => w).length
    const minutes = Math.ceil(words / KITE_PLUGIN_SETTINGS.wordsPerMinute)
    kitePluginSend({cmd: "counted", words: words, minutes: minutes})
}
```

**`plugins/word-count/WordCountWindow.qml`** runs the action and shows the answer.

```qml
import QtQuick

// Word Count in one browser window.
Item {
    id: root
    property var win
    property var app
    property var plugin

    function runAction(id) {
        if (id === "word-count.show" && win.currentTab)
            win.currentTab.runInMainFrame("globalThis.__kiteWordCount && __kiteWordCount()")
    }
    Connections {
        target: root.win
        function onPluginMessage(tab, message) {
            // Every plugin's window hears every message.
            if (message.plugin !== root.plugin.id || message.m.cmd !== "counted") return
            if (tab === root.win.currentTab)
                root.win.toast(message.m.words + " words, about " + message.m.minutes + " min to read", root.plugin.icon)
        }
    }
}
```

**`plugins/word-count/WordCountSettings.qml`** is its page in Settings.

```qml
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Settings → Plugins & Extensions → Word Count.
RowLayout {
    id: root
    property var app
    property var plugin

    QQC2.Label { text: "Words read per minute:" }
    QQC2.SpinBox {
        from: 50
        to: 1000
        stepSize: 10
        value: root.app.pluginSetting(root.plugin, "wordsPerMinute")
        onValueModified: root.app.setPluginSetting(root.plugin, "wordsPerMinute", value)
    }
}
```

Zip it and install it:

```sh
cd word-count && zip -r ../word-count.zip .
```

1. Open **Settings → Plugins & Extensions**, choose **Install Plugin from File…** and pick
   `word-count.zip`.
2. Read the warning and choose **Install**, then switch on **Word Count**.
3. Reload the page you want to count, since open pages don't have the script yet.
4. Press **Ctrl+Shift+L**. A message at the bottom of the window shows the count.

To ship it with Kite instead, put the folder in `crates/kite/plugins/` and add these lines to
`crates/kite/src/browser.rs`, as described in [Shipping a plugin with Kite](#shipping-a-plugin-with-kite).
This one goes in `builtin_plugins()`:

```rust
        include_str!("../plugins/word-count/plugin.json"),
```

And this one in `PLUGIN_FILES`:

```rust
    ("plugins/word-count/page.js", include_str!("../plugins/word-count/page.js")),
```

Then check it with `cargo test -p kite plugins_are_valid_and_complete`.

There's nothing else to wire up. Because of what it declares, the plugin also has:
- a row in the toolbar's extensions panel while it's on, with a button to its settings
- a **Configure** button in Settings
- its action in **Settings → Shortcuts**, where users can change the keys
- an entry in the command palette, both to switch it and to run the action

## Going further

The existing plugins are the best reference:

| Look at | To see |
|---|---|
| `plugins/pip/` | A small window item, a separate window shared by the whole browser, and a page script reporting what it finds |
| `plugins/gestures/` | The window item using a hook in `bootstrap.cpp`, tested logic in `strokes.js`, and a large settings page |
| `plugins/vim-keys/` | Frames, updating already open tabs, its own `VimBar.qml`, and a page script built from several files |
| `plugins/plasma-integration/` | A page script in the page's own world beside one in Kite's, context menu items, and talking to the desktop (`kite_core::plasma`) |
| `plugins/reader-view/` | A part in every tab (`win.tabOpened`, `tab.pageArea`), scripts run in pages only when needed (`Browser.pluginFile`), a settings page sharing its controls with the page's, and a service Kite keeps for it (`kite:reader`, `kite_core::reader`) |

If a plugin needs something Kite doesn't offer, add it to Kite (a hook in `BrowserWindow.qml`, say) and
use it from the plugin. For a plugin shipped with Kite, put the feature's logic in the plugin's folder, so
that nothing of it runs while it's off.
