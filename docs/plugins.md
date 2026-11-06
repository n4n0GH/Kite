# Native plugins

Plugins are Kite's own optional features. Mouse Gestures, Vim Keys and Picture-in-Picture are all
plugins. Each is a folder in `crates/kite/plugins/`, and users switch them on and off in
**Settings → Plugins & Extensions** or the toolbar's extensions panel.

## Plugins or WebExtensions?

| | Native plugin | WebExtension |
|---|---|---|
| Ships | Inside Kite, compiled in | Installed by the user |
| Can use | Anything in Kite, including QML and internal functions | The `browser.*` API |
| Runs in | Kite's process, fully trusted | Isolated pages and worlds |
| Compatibility | None promised, internals can change | A stable API |
| Written by | Kite's developers | Anyone |

So a plugin is **not an interface for third parties**. It's a way to keep a feature separate, and to
have nothing of it running while it's off. To change a plugin you rebuild Kite.

## What a plugin is made of

```
crates/kite/plugins/<id>/
    plugin.json        the manifest, required
    SomeWindow.qml     optional: runs once per browser window while it's on
    SomeSettings.qml   optional: its page in Settings
    page.js            optional: runs in web pages while it's on
    helpers.js         optional: plain logic, ideally with helpers.test.js beside it
```

A plugin can use any of these, and all three existing plugins use most of them. You only need
what your feature needs: a manifest alone is valid, although it doesn't do anything.

## The manifest

| Field | Required | Meaning |
|---|---|---|
| `id` | Yes | The folder's name. Only `a-z`, `0-9` and `-` |
| `name`, `description`, `icon` | Yes | What Settings shows. `icon` is an icon theme name |
| `defaultEnabled` | No | On for users who haven't chosen. Default `false` |
| `window` | No | The QML item made once per browser window |
| `settingsPage` | No | The QML item shown under its **Configure** button |
| `actions` | No | Actions with optional shortcuts. They need `window` |
| `pageScripts` | No | Scripts for web pages |
| `settings` | No | Its settings and their defaults, any JSON. `enabled` isn't allowed |

Paths are relative to `crates/kite/`, and must be inside the plugin's folder.

**An action** has `id` (`<plugin id>.<name>`), `group` (the heading in Settings → Shortcuts),
`name`, `keys` (a list, can be empty) and `icon`.

**A page script** has these fields:

| Field | Meaning |
|---|---|
| `files` | One or more files, joined in order into one script |
| `worldId` | Which isolated world it runs in. Use `1`, Kite's own. `0`, the page's world, is refused because the page could read the plugin's secret |
| `runsOnSubFrames` | Whether it also runs in frames |
| `injectionPoint` | **Kite's own numbering**: `0` as the document is created, `1` when it's ready, anything else later. Not the numbering of Qt's enum |

`kite_core::plugins` checks all of this when the manifest loads, and a test makes it fail the test
run if it's wrong.

## What each part gets

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
can be anything. Treat it as text. Never put it into code you run, or into markup.

## Registering a plugin

A plugin's files are compiled in, so two lists in `crates/kite/src/browser.rs` have to know it:

| List | Add |
|---|---|
| `all_plugins()` | Its `plugin.json`. Always |
| `PLUGIN_FILES` | Each page script file. Only if it has any |

The test `plugins_are_valid_and_complete` fails if you forget either, or if a named file doesn't exist.
QML files are picked up by `build.rs` by themselves. **Their names must be unique across all of Kite**,
so start them with the plugin's name.

## Example: Word Count

This plugin counts the words on the current page and estimates a reading time. It shows every part:
an action with a shortcut, a page script, a message back, a setting and a settings page.

Create the folder `crates/kite/plugins/word-count/` with these four files.

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

Then register it in `crates/kite/src/browser.rs`. Add this line to the list in `all_plugins()`:

```rust
        include_str!("../plugins/word-count/plugin.json"),
```

And this line to `PLUGIN_FILES`:

```rust
    ("plugins/word-count/page.js", include_str!("../plugins/word-count/page.js")),
```

Build and try it:

```sh
cargo test -p kite plugins_are_valid_and_complete
cargo run
```

1. Open **Settings → Plugins & Extensions** and switch on **Word Count**.
2. Reload the page you want to count, since open pages don't have the script yet.
3. Press **Ctrl+Shift+L**. A message at the bottom of the window shows the count.

There's nothing else to wire up. Because of what it declares, the plugin also has:
- a row in the toolbar's extensions panel, with its switch
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

If a plugin needs something Kite doesn't offer, add it to Kite (a hook in `BrowserWindow.qml`, say) and
use it from the plugin. That's what the private interface is for. Do put the feature's logic in the
plugin's folder, so that nothing of it runs while it's off.
