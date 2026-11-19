# Parental controls

A profile can be a child's. A child's profile keeps adult sites out, holds search engines to their
safe search, can be held to less than an ordinary profile may do, and can't be left for another
profile without the parent password. Everything is set in **Settings → Parental Controls**.

The code is `parental.rs` in `kite-core` (the password, which profiles are a child's, what each is
held to, the site filter and its lists), `ParentalSession` in `src/state.rs` (what the running Kite
follows), and `ParentalPage.qml` (the settings page).

## What it holds against, and what it doesn't

It works inside Kite. It keeps a child in their profile and keeps listed sites away from it.

It doesn't stop another browser on the computer. And all of it is in files of the user's own, in
Kite's data folder (`~/.local/share/kite`, or `~/.var/app/io.github.n4n0gh.kite/data/kite` for the
Flatpak): `parental.json`, and the lists in `sitefilter`. The easy ways around that are closed
(see "Changes outside Kite" below), but someone who knows how the files are made, or starts Kite
with another data folder from a terminal, gets around it. A system account of the child's own,
without the parent's password, is what holds against that. The Settings page says so.

## Changes outside Kite

Changing `parental.json` by hand, removing it, or emptying a list is noticed, once, as a profile
loads (`Parental::check`, called by `parental_check` in `main.rs` before anything goes by the
file, the profile picker included):

- **The seal.** `parental.json` carries a hash of its own state (`seal`): the file's JSON without
  that field, written compactly, hashed as the password is (Argon2id, same cost), with the
  password's salt reversed as the salt. Kite writes it with every change. A file whose seal
  doesn't fit wasn't written by Kite.
- **The second copy.** Each sealed state is also written to `profiles/.parental`. If
  `parental.json` is missing, unreadable, without its password or doesn't fit its seal, and the
  copy fits its own, the file is put back from the copy. The window says so when it opens
  ("Parental controls were changed outside Kite and have been put back"), and Settings → Parental
  Controls tells the parent when it happened, until they press OK.
- **With no copy to go by** (it was removed or changed too), no profile opens without the parent
  password, a child's neither. With the password, the file counts as it is, and gets its seal.
  A `parental.json` from a Kite before there were seals is taken the same way, once.
- **The lists.** The file notes the SHA-256 of each downloaded list and of the compiled filter
  (`files`). A list that isn't as it was downloaded is removed and fetched anew, and a compiled
  filter that isn't as Kite wrote it is built anew; until then no site opens. This is looked at
  whenever the lists are (at the start, twice a day, and when Settings changes them).

The seal takes no secret to make: it holds against changing or deleting files, not against someone
who reads this and writes their own. That's the trade for Kite being able to check, and to note the
lists it downloads, without the password.

**A forgotten password:** remove both `parental.json` and `profiles/.parental`. Parental controls
start over, with no password and no child's profile.

## The parent password

There is one, for all profiles. It's set with the first child's profile (in the profile picker's
"New Profile…"), or before that in Settings → Parental Controls. It's stored as an Argon2id hash.
After three wrong ones in a row there's a wait before the next is looked at, growing with each up
to five minutes; the count is kept in the file, so starting Kite again doesn't shorten it.

It's asked:

- before a profile that isn't a child's opens (the default profile is always the parent's, under whatever name), and
  before a private session started from outside Kite (`kite --private`, the launcher's action),
  which is held to nothing. This is "Ask for it before a profile that isn't a child's opens" in
  Settings, on by default. Without it, a child's profile holds nobody;
- to make, rename or delete profiles in the picker, once one of them is a child's, and to change
  the picture of a profile that isn't a child's (a child's profile's picture is anyone's to change);
- to open Settings → Parental Controls for changes, in any profile;
- by "Allow this site…" on the page of a blocked site.

A child's profile opens without it.

How the asking works: `main()` (`parental_gate`) starts `kite --parent-password <profile>`, a small
process of its own with the dialog (`cpp/profiles.cpp`), as the profile picker is, because this has
to be settled before QtWebEngine starts. A Kite running a profile that isn't a child's hands the
Kite it starts a pass instead (a random name of a file in `parent-pass`, good once and for five
minutes, in the variable `KITE_PARENT_PASS`): from the parent's profile, the Profiles menu opens
the others without asking again. A link from another application that joins the only Kite that's
open isn't asked about either: nothing new opens.

## What a child's profile may see

**Adult sites are blocked** by lists of site names, downloaded when the profile first runs and
again every two days, into `sitefilter` beside the profiles (shared by all child profiles):

| List | Sites | On at first |
|---|---|---|
| [StevenBlack's adult sites](https://github.com/StevenBlack/hosts) | about 77,000 | yes |
| [Anti-Porn HOSTS File](https://github.com/4skinSkywalker/Anti-Porn-HOSTS-File), from the NoFap community | about 70,000 | yes |
| [OISD NSFW](https://oisd.nl) | about 500,000 | yes |
| [The Block List Project: Porn](https://github.com/blocklistproject/Lists) | about 950,000 | no |
| [Bon Appétit porn domains](https://github.com/Bon-Appetit/porn-domains) | about 440,000 | no |

The three that are on come to about 550,000 sites. They're kept as sorted hashes of the names (about
4 MB in memory, and in `filter.bin` for the next start), so a check is a few lookups. A name stands
for all under it: `example.com` also for `img.cdn.example.com`. Every request goes through the
check (`kite_intercept` in `src/ffi.rs`), not only pages: nothing loads from a listed site into
another page either, and extensions' own requests are held to the same. A page load gets a page
that says why (`kite:blocked`), with "Allow this site…" for the parent.

Until a list has been downloaded, no site opens ("Not ready yet", which tries again by itself):
without a list there's nothing to tell sites apart by.

**Safe search.** Google, YouTube, Bing, DuckDuckGo and Ecosia keep addresses for safe search that
can't be switched off, meant for networks to point the engines' names at
(`forcesafesearch.google.com`, `restrict.youtube.com`, `strict.bing.com`, `safe.duckduckgo.com`,
`strict-safe-search.ecosia.org`). A child's profile does that for itself, with Chromium's
`--host-resolver-rules` (`safe_search_resolver_rules`). As a start-up flag, it's followed from when
the profile is opened next. Search engines without such an address (Startpage, Brave Search, Qwant,
Yandex, Yahoo and others) are blocked while safe search is on; if one of them was the profile's
search engine, DuckDuckGo takes its place. With a proxy set in Kite, names are resolved by the
proxy and this doesn't hold.

**The parent's own lists.** "Always allowed" and "Always blocked" sites, and "Open only the sites
allowed below" for a profile that should reach a handful of sites and nothing else. The parent's
lists come before the downloaded ones.

## What a child's profile may do

Each of these is off for a new child's profile and can be allowed in Settings:

- **Install, remove and switch extensions.**
- **Install and remove plugins, and switch them on or off.** Without it, the plugins that are on
  can still be configured (their toolbar buttons lead to their settings, and Settings → Plugins &
  Extensions shows them): `plugins.<id>.<setting>` is open while that plugin is on,
  `plugins.<id>.enabled` isn't.
- **Change settings.** Without it, the Settings window shows all its pages, but only the ones
  that make the browser one's own can be used: **Appearance, Tabs, Site Settings and Shortcuts**
  (`CHILD_PAGES` in `palette.js`), and Parental Controls. Plugins & Extensions is open in part
  (`CHILD_PAGES_IN_PART`): the settings of plugins that are on, with the switches, installing,
  removing and the extension settings disabled. The others show a padlock and a note.
  The command palette offers only the settings that can be changed, `kite:config` is closed,
  settings can't be imported, and search engines can't be changed. The rule itself is in Rust
  (`may_change_setting` in `src/browser.rs`): open are the settings of those pages and what the
  window changes as one goes (toolbars, the bookmarks bar, sizes). `palette.test.js` keeps the
  two in step: a setting put on an open page that Rust would refuse fails it.
- **Open private windows.** Sites stay blocked in them, but they leave no history.
- **Delete history.** Without it, history entries can't be deleted, and "clear on exit" leaves
  history, downloads and closed tabs alone.

The checks are in Rust (`ParentalSession::allows`), so a menu entry that was missed changes
nothing; the interface greys out what it knows of.

While Parental Controls is unlocked with the password, the profile may do all of this, so that a
parent can install an extension or change a setting. It locks again when Settings is closed.

## Testing

`cargo test -p kite-core parental` covers the password, the seal and the second copy, the lists
and the filter. The dialogs and the running browser were checked on a headless KWin with real
input: making a child's profile in the picker, the password before another profile and before a
private session, the lists downloading and a listed site getting the blocked page, changes in
Settings followed at once, "Allow this site…", the search engines answering from their safe
search addresses, a `parental.json` edited and removed by hand being put back, one without a seal
or copy asking for the password, emptied lists being fetched anew, and the open pages of Settings
and the command palette in a child's profile. The seal was checked against a second
implementation (Python's `cryptography`).
