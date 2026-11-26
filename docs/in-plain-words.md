# Kite in plain words

The short version of these pages, for readers who don't write Rust, QML or C++ every day. It says
what the parts are and how they work together, and leaves out most names of files and functions.
Each section ends with where the full version is.

## The whole thing in a paragraph

Kite is a web browser made of three parts. **Chromium** draws the web pages. Kite gets it ready-made
(through Qt's QtWebEngine) and doesn't change it. **The interface** (windows, tabs, menus, settings)
is written in QML. **The core** is everything Kite remembers and decides: bookmarks, history,
settings, passwords, the content blocker, extensions. It's written in Rust. A thin layer of C++ joins
them where nothing else can.

```
 you ──► the interface (QML) ──► the core (Rust) ──► files on disk
              │                        ▲
              ▼                        │  "may this load?", through the C++ layer
         the pages (Chromium) ─────────┘
```

## The four languages

| Language | What it is, briefly | What Kite uses it for | Where |
|---|---|---|---|
| Rust | A compiled language whose compiler refuses whole kinds of mistakes before the program ever runs, such as using something that's already gone, or ignoring that a step can fail | Data and decisions | `crates/kite-core`, `crates/kite/src` |
| QML | Qt's language for interfaces. A file describes what is there ("a window has a toolbar, the toolbar has a button"), and short pieces of JavaScript say what a click does | Every window, menu and dialog | `crates/kite/qml` |
| C++ | The language Qt itself is written in | Only what Qt offers no other way to reach: starting up, the hooks into Chromium, printing, the profile picker | `crates/kite/cpp` |
| JavaScript | The language of web pages | The scripts Kite puts into pages, and the `browser.*` functions extensions call | `crates/kite/src/js`, `crates/kite-core/src/webext/js` |

**The rule for where code goes:** if it can be decided without a window on screen, it goes in
`kite-core`. That part knows nothing about Qt, so it can be tested by itself, quickly.

Full version: [architecture.md](architecture.md), "The layers" and "Repository layout".

## Enough Rust and QML to read along

You don't need to write either to follow the code. These come up everywhere:

| In Rust | Means |
|---|---|
| `fn name(x: &str) -> bool` | A function taking a piece of text and answering yes or no |
| `let x = …` / `let mut x = …` | A value that stays as it is / one that may change later |
| `&x` | "Have a look at `x`" without taking it away from its owner |
| `Option<T>`: `Some(v)` or `None` | A value that may be missing. Rust has no "null" that turns up unannounced |
| `Result<T, E>`: `Ok(v)` or `Err(e)` | The answer of a step that can fail |
| `?` after a call | "If that failed, stop here and pass the error on" |
| `match x { … }` | "Depending on what `x` is, do one of these", with every case covered |
| `pub`, `mod`, `impl` | Usable from outside / a file or folder of code / the functions belonging to a type |
| `#[test] fn …` | A test, usually at the bottom of the file it tests |

| In QML | Means |
|---|---|
| `Rectangle { width: 100 }` | "There is a rectangle, 100 wide." Things nested inside it are drawn inside it |
| `id: toolbar` | A name other things in the file can use for it |
| `width: parent.width / 2` | A **binding**: not worked out once, but kept true. When the parent's width changes, this follows by itself |
| `property bool busy: false` | A value of its own that others can read, bind to and change |
| `onClicked: { … }` | What to do when it says "clicked". `on` + a signal's name |
| `function open() { … }` | Ordinary JavaScript |

## What happens when…

**Kite starts.** It reads how it was started, works out which profile to open, opens that profile's
files, starts Chromium and shows a window, or brings back the windows and tabs of last time. Tabs
brought back load their page only when you look at them. If it has to ask which profile, the picker
is a small program of its own, because the profile must be known before Chromium starts.

**You click something.** The interface calls one of four helpers that Rust offers it: `Browser`
(settings, history, bookmarks, sessions, downloads), `Extensions`, `Passwords` and `FormFill`. The
code calls them singletons: there is one of each, and any QML file can use it by name. Questions and
answers travel as text. Slow work is done in the background and answers later, so the window never
freezes waiting for it.

**You change a setting.** The settings are one file of names and values, like
`"tabs.position": "top"`. Rust stores the new value and announces it, and whichever part of the
interface cares hears that and changes. A few settings are Chromium's to take at start only, which is
why those need a restart.

**A page loads something.** Every request a page makes, for itself or for a picture or a script,
passes one checkpoint in Rust. In order: is the site known to be dangerous, can the address be
cleaned of tracking and moved to HTTPS, does an extension want it changed, does the content blocker
want it stopped. The checkpoint works on the same thread that draws the interface, so it has to be
quick. Anything slow there makes scrolling and video stutter.

**A page has to tell Kite something**, such as "there is a login form here". Kite puts small scripts
into pages, but into a room of their own (Chromium calls it an isolated world): they see the same
page, and the page's own scripts can neither see nor touch them. Their messages to the browser carry
a secret the page can't read, so a page can't make one up. Each extension gets a room of its own
too.

**You open a `kite:` page** (the start page, `kite:config`, reader view, the warning pages). Nothing
is fetched: Rust writes the page on the spot. Web pages can link to these addresses, so the ones that
do something first check who is asking.

**An extension calls `browser.tabs.create()`.** Kite runs Firefox's extensions, and does so itself:
Chromium has no part in it. The call goes from the extension's JavaScript to Rust, which checks that
the caller is the extension it says it is and has the permission. Rust then answers with a list of
things to do: reply to the extension, have the interface open a tab, tell the toolbar something
changed. As that part only makes lists, it's tested without a browser running.

Full version: [architecture.md](architecture.md), "Start-up", "How the layers talk" and "How things
flow". Which file defines which window or menu: [ui.md](ui.md).

## Extensions and plugins

Kite has two kinds of add-on, and the difference matters:

| | Extension | Plugin |
|---|---|---|
| What it is | A Firefox WebExtension, from addons.mozilla.org or a file | A piece of Kite itself. Six come with Kite (Mouse Gestures, Vim Keys, Picture-in-Picture, Plasma Integration, Reader View, YouTube Download); others install from a file |
| What it may do | What its permissions allow, from rooms of its own | Anything Kite can: read every page, saved logins, history |
| Will it keep working | Yes, the API is a stable one | No promise, since it uses Kite's insides |

Installing a plugin is like installing a program, and the install dialog says so. Kite sees to two
things: nothing of a plugin runs before you switch it on, and no plugin can start a program of its
choosing. The one plugin that needs an outside tool (yt-dlp) gets a runner that can start that tool
and nothing else.

A plugin is a folder: a `plugin.json` that describes it, and next to it whichever of these it needs:
QML for what it shows in a window, QML for its page in Settings, JavaScript to run in web pages.

Full version, with a complete example plugin: [plugins.md](plugins.md).

## Parental controls

A profile can be marked as a child's. Such a profile:

- doesn't open adult sites, going by downloaded lists (about 550,000 sites with the lists that are on
  at first), plus the parent's own "always allowed" and "always blocked";
- holds search engines to their safe search, and blocks the ones that have none;
- may not install extensions, change most settings, open private windows or delete history, unless
  the parent allows each;
- can't be left for another profile without the parent password.

What it's good for: it works inside Kite. It doesn't stop another browser on the same computer, and
someone who knows how its files are made gets around it. Simple tampering (editing or deleting the
file) is noticed and put back. A system account of the child's own is what holds beyond that.

Full version: [parental-controls.md](parental-controls.md).

## How fast it is

Measured in November 2026, on one machine, against Falkon (same engine as Kite) and Firefox:

| | Kite 0.10.3 | Falkon | Firefox |
|---|---:|---:|---:|
| From start to a shown page | 0.68 s | 0.64 s | 1.07 s |
| Memory with five sites open | 843 MiB | 785 MiB | 1297 MiB |
| Processor use doing nothing, five sites open | 2.9 % of a core | 7.9 % | 8.7 % |
| Speedometer 3.1 (more is better) | 25.8 | 25.2 | 24.6 |

What the measuring taught, and what to keep in mind when adding to Kite: whatever every tab or every
page gets is paid for many times over. A menu built for each tab whether it's ever opened, or a
script that watches every change a page makes, looks free with one tab and costs real memory and
speed with thirty. So pieces of the interface are made when first needed, and page scripts wait for
something to do.

Full version, with the method and its limits: [performance.md](performance.md).

## Where things are kept

Everything is under `~/.local/share/kite/` (for the Flatpak,
`~/.var/app/io.github.n4n0gh.kite/data/kite/`), one folder per profile:

| File or folder | Holds |
|---|---|
| `settings.json` | The settings |
| `browsedata.db` | History, site icons, search engines, per-site settings |
| `bookmarks.json` | The bookmarks |
| `session.json` | The windows and tabs of last time, and closed ones |
| `extensions/` | The extensions and what they stored |
| `webengine/` | Chromium's own: cookies, cache, what sites stored |

The log is `logs/<profile>/kite.log`, with `last-crash.txt` beside it after a crash.

## Building, testing, and getting out of trouble

```sh
cargo build                 # builds target/debug/kite
cargo test --workspace      # the Rust tests
node --test crates/kite-core/src/webext/js/*.test.js crates/kite/qml/*.test.js crates/kite/plugins/*/*.test.js
```

| If | Start Kite with |
|---|---|
| A plugin breaks it | `kite --no-plugins`: none runs this time, nothing stored changes |
| A setting breaks it | `kite --factory`: saves the settings to a file, then starts with the defaults. Bookmarks, logins, extensions and sessions stay |

What's needed to build, and packaging: the [main README](../README.md).

## Before you change something

The few rules that save the most trouble. The rest, with the reasons, is the table "Things that will
bite you" in [architecture.md](architecture.md).

- **Logic goes in `kite-core`, with a test.** The interface asks, the core decides.
- **Keep the request checkpoint fast.** It runs for every request, on the thread that draws.
- **Text from outside is text.** A page's title, an address, an add-on's name: Qt draws anything
  that looks like markup as markup, and fetches a picture it names. Such text is always shown as
  plain text.
- **Ask which page is loaded, not which address the tab is going to.** For a moment during a load
  the two differ, and logins, permissions and extensions go by the page that's there.
- **A new command goes into each menu by hand.** The main menu, the menu bar and the tab menu are
  written separately. Shortcuts and the command palette share one list.
- **A new checkbox in Settings also goes into the command palette's list.** A test fails until it
  does. The steps for a new setting, action, `kite:` page, extension API or toolbar button are
  under "Common changes" in [architecture.md](architecture.md).

## Words you'll meet

| Word | Means here |
|---|---|
| Crate | A Rust package. Kite has two: `kite-core` (the core) and `kite` (the program) |
| Singleton | One of the four helpers Rust offers the interface: `Browser`, `Extensions`, `Passwords`, `FormFill` |
| cxx-qt | The tool that turns a piece of Rust into something QML can call |
| FFI, C ABI | The plain function calls by which Rust and C++ reach each other |
| Profile | One user's browser: its own settings, history, logins and cookies |
| Container | A set of tabs with cookies of their own inside a profile |
| World | The room a script runs in on a page. The page has one, Kite has one, each extension has one |
| Interceptor | The checkpoint every request passes |
| Scheme | What stands before the colon in an address: `https:`, `kite:`, `moz-extension:` |
| Effect | One entry in the list of things to do that an extension's call comes back with |
| Content script | A script an extension runs in web pages |
| Native messaging | An extension talking to a program on the computer, such as a password manager's helper |
