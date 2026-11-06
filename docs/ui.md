# Where the interface is defined

All paths are under `crates/kite/qml/`, except for plugins. QML files are compiled into the
`org.kde.kite` module, so they can use each other by name.

## Windows

| Window | File | Opened by |
|---|---|---|
| Browser window | `BrowserWindow.qml` | `Main.newWindow` |
| Settings | `PreferencesWindow.qml` | `Main.showPreferences(page)` |
| Extensions | `AddonsWindow.qml` | `Main.showAddons` |
| Library (history and bookmarks) | `LibraryWindow.qml` | `Main.showLibrary(page)` |
| Downloads | `DownloadsWindow.qml` | `Main.showDownloads` |
| JavaScript popups | `PopupWindow.qml` | `WebTab.onNewWindowRequested` |
| Extension options | `ExtensionOptionsDialog.qml` | `Main` |
| Extension login flow | `AuthFlow.qml` | `Main` |
| Picture-in-picture | `plugins/pip/PipWindow.qml` | `Main.startPip` |
| Profile picker | `crates/kite/cpp/profiles.cpp` | `kite --pick-profile`, its own process |

Settings, Extensions, Library and Downloads are based on `ManagerWindow.qml`, and only one of each is
open at a time (`Main.showSingleton`).

## The browser window

```
BrowserWindow.qml
├─ header
│  ├─ BrowserMenuBar         optional classic menu bar
│  ├─ TabStripSlot           tab bar, when above the address bar
│  ├─ NavigationToolbar      built from the "toolbar.items" setting
│  │  ├─ LocationBar         address bar and its icons
│  │  └─ ExtensionButtons    extensions button and pinned extensions
│  ├─ BookmarksBar
│  └─ TabStripSlot           tab bar, when below the address bar
├─ SideTabs (left and right) vertical tab list (TabStrip inside)
├─ splitView
│  ├─ Sidebar
│  ├─ content                left pane
│  │  ├─ the WebTabs of the pane (z: -1)
│  │  ├─ FindBar, statusBubble, toastBox
│  └─ rightContent           right pane, in split view
├─ footer: TabStripSlot      tab bar, when below the page
└─ not in the layout: MainMenu, TabSearch, CommandPalette, ExtensionPopup,
   historyMenu, groupMenu, SessionDialogs, CustomizeToolbarDialog, plugin window items
```

`TabStripSlot.qml` holds a horizontal `TabStrip.qml`, which shows `TabButton.qml`s. In split view it
holds one strip per pane. The tab bar's place comes from the `tabs.position` setting.

Each `WebTab.qml` holds the page's own overlays: the certificate warning, permission prompts,
`ReaderControls.qml`, `AutofillPopup.qml`.

## Menus

There's no shared menu definition: **the main menu, the menu bar and the tab menu are written
separately**. A new command needs adding to each one where it belongs.

| Menu | Opened by | Defined in |
|---|---|---|
| Main menu (☰) | The toolbar's menu button | `MainMenu.qml`, through `BrowserWindow.openMainMenu`. The About box is here too |
| Classic menu bar (File, Edit, View…) | Setting, or tapping Alt | `BrowserMenuBar.qml` |
| Page context menu | Right-click on a page | `PageContextMenu.qml`, including extensions' `menus` entries |
| Tab context menu | Right-click on a tab | `TabButton.qml` |
| Tab bar options | Right-click on the tab bar | `TabBarMenu.qml`, also a submenu of the tab menu |
| Tab group menu | A group's chip or header | `groupMenu` in `BrowserWindow.qml` |
| Toolbar context menu | Right-click on the toolbar | `toolbarMenu` in `NavigationToolbar.qml` |
| Bookmarks button | The toolbar's bookmarks button | `bookmarksMenu` in `NavigationToolbar.qml` |
| Back and forward history | Holding Back or Forward | `historyMenu` in `BrowserWindow.qml` |
| Bookmark folders | Everywhere bookmarks are listed | `BookmarkFolderMenu.qml` (loads itself recursively) |
| Containers, profiles, sessions | Submenus of the above | `ContainerMenu.qml`, `ProfileMenu.qml`, `SessionMenu.qml` |
| Extension's menu | Right-click on an extension's button | `extMenu` in `ExtensionButtons.qml` |
| Library's lists | Right-click in the Library | `LibraryWindow.qml` |

### Address bar icons (`LocationBar.qml`)

| What the user sees | Defined as |
|---|---|
| Suggestions under the address | `popup` |
| Security icon: site info, "Your connection to this site is encrypted", View Certificate, permissions | `siteInfo` |
| Key icon: saved logins | `keyMenu` |
| Shield icon: content blocker | `shieldMenu` |
| Star: add or edit a bookmark | `bookmarkPopup` |
| Private window icon, reader view and picture-in-picture buttons | In the same file |

## Command bars

`CommandBar.qml` is the search field and list that both of these use:

| Bar | File | Where its rows come from |
|---|---|---|
| Command palette (Ctrl+Shift+P) | `CommandPalette.qml` | `Main.shortcutActions`, sessions, profiles, plugins, and settings from `palette.js` |
| Tab search (Ctrl+Shift+E) | `TabSearch.qml` | The open tabs of all windows |

Matching and ranking are in `fuzzy.js` and `CommandBar.refresh`. A setting's score is weighted by
`SETTING_WEIGHT` in `palette.js`, so actions come first.

## The panels and popups

| What | File |
|---|---|
| Extensions panel (extensions and plugins, with toggles) | `ExtensionButtons.qml` |
| An extension's popup | `ExtensionPopup.qml` |
| Sidebar: bookmarks, history, an extension's panel | `Sidebar.qml` |
| Find in page | `FindBar.qml` |
| Reader view controls | `ReaderControls.qml` |
| Form and password suggestions | `AutofillPopup.qml` |

## Dialogs

| Dialog | File |
|---|---|
| Add an extension? | `InstallDialog.qml` |
| An extension asking for permissions | `PermissionDialog.qml` |
| HTTP and proxy login | `AuthDialog.qml` |
| Master password | `MasterPasswordDialog.qml` |
| Save, rename and manage sessions | `SessionDialogs.qml` |
| Customize Toolbar | `CustomizeToolbarDialog.qml` |
| Where to save a download | `SaveDialog.qml` |
| Print | `crates/kite/cpp/print.cpp` (Qt's own dialog) |
| Rename a tab group, open and save files | `BrowserWindow.qml` |

## Settings pages

The list is `pages` in `PreferencesWindow.qml`, and each page is a `Component` in that file:

General, Appearance, Browsing, Search Engines, Privacy, Passwords, Content Blocking, Downloads, Site
Settings, Plugins & Extensions, and Shortcuts.

Submenus are pages pushed on top of them. Most are in the same file, such as a site's settings, a
start page file and a plugin's settings. Two have files of their own:

| Page | File |
|---|---|
| Manage Cookies | `CookiesPage.qml` |
| Manage Certificate Exceptions | `CertificatesPage.qml` |
| A plugin's settings | `plugins/<id>/…`, see [plugins.md](plugins.md) |

## Small pieces

| Piece | File |
|---|---|
| Tab icon | `TabIcon.qml` |
| The × on tabs | `CloseButton.qml` |
| The items the toolbar can show | `ToolbarItems.qml` |
| Tab tree and group logic (tested with node) | `tabtree.js` |

## Actions and shortcuts

Keyboard shortcuts, the palette and the Shortcuts page all use one list: `coreActions` in `Main.qml`,
plus the actions of plugins that are on. `BrowserWindow.runAction(id)` runs them. To add one, see
[Common changes](architecture.md#common-changes).

Pages that aren't QML, such as `kite:start`, `kite:reader` and `kite:certificate`, are generated in
Rust. See [A kite: page](architecture.md#a-kite-page).
