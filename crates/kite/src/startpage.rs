//! `kite:start` — the new-tab / speed-dial page (Falkon's falkon:speeddial).

use crate::state::app;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const MAX_TILES: usize = 12;

/// Sites removed from the speed dial since Kite started. They stay removed for
/// good ("speeddial.hidden"), but only these are offered back: over time that
/// list only grows, and adding a site again brings it back anyway.
static REMOVED_THIS_RUN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The same site for the speed dial: scheme, "www." and a trailing slash
/// don't matter.
fn same_site(a: &str, b: &str) -> bool {
    let norm = |u: &str| {
        let u = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
        u.trim_start_matches("www.").trim_end_matches('/').to_lowercase()
    };
    norm(a) == norm(b)
}

fn list(key: &str) -> Vec<serde_json::Value> {
    app().shared.settings().get(key).as_array().cloned().unwrap_or_default()
}

/// A change from the speed dial page (`kite:<page>?add=…&title=…`,
/// `?remove=…`, `?restore=…`, `?restoreAll`). Only Kite's own pages (or the
/// address bar) may make them, never a website. Returns a message for the
/// page ("" for none), and the URL to undo with.
pub fn edit(url: &str, initiator: &str) -> Option<(String, String)> {
    if !(initiator.is_empty() || initiator.starts_with("kite:")) {
        return None;
    }
    let q: std::collections::HashMap<String, String> = url::Url::parse(url).ok()?.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    // Resolve an added address first: resolve_input reads the settings.
    let added = q.get("add").map(|i| i.trim().to_owned()).filter(|i| i.contains("://") || (i.contains('.') && !i.contains(' ')) || i.starts_with("localhost")).map(|i| crate::browser::resolve_input(&i));
    let a = app();
    let mut settings = a.shared.settings();
    let mut pinned: Vec<serde_json::Value> = settings.get("speeddial.pinned").as_array().cloned().unwrap_or_default();
    let mut hidden: Vec<String> = serde_json::from_value(settings.get("speeddial.hidden")).unwrap_or_default();
    let mut out = (String::new(), String::new());
    if q.contains_key("add") {
        let Some(target) = added else {
            return Some(("Enter a web address, like kde.org".into(), String::new()));
        };
        let title = q.get("title").map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).unwrap_or_else(|| kite_core::util::host_of(&target).trim_start_matches("www.").to_owned());
        pinned.retain(|p| !same_site(p["url"].as_str().unwrap_or(""), &target));
        pinned.push(serde_json::json!({"url": target, "title": title}));
        hidden.retain(|h| !same_site(h, &target));
        out.0 = format!("Added {title}");
    } else if let Some(target) = q.get("remove") {
        let before = pinned.len();
        pinned.retain(|p| !same_site(p["url"].as_str().unwrap_or(""), target));
        if pinned.len() == before && !hidden.iter().any(|h| same_site(h, target)) {
            hidden.push(target.clone());
            REMOVED_THIS_RUN.lock().unwrap_or_else(|e| e.into_inner()).push(target.clone());
        }
        out = ("Tile removed".into(), format!("restore={}", url::form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>()));
        if pinned.len() != before {
            // A tile the user added: undo adds it back.
            let title = q.get("title").cloned().unwrap_or_default();
            out.1 = format!("add={}&title={}", url::form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>(), url::form_urlencoded::byte_serialize(title.as_bytes()).collect::<String>());
        }
    } else if let Some(target) = q.get("restore") {
        hidden.retain(|h| !same_site(h, target));
        REMOVED_THIS_RUN.lock().unwrap_or_else(|e| e.into_inner()).retain(|h| !same_site(h, target));
    } else if q.contains_key("restoreAll") {
        let mut removed = REMOVED_THIS_RUN.lock().unwrap_or_else(|e| e.into_inner());
        hidden.retain(|h| !removed.iter().any(|r| same_site(h, r)));
        removed.clear();
    } else if let Some(order) = q.get("order") {
        // Tiles dragged into a new order (a JSON array of their URLs).
        let order: Vec<String> = serde_json::from_str(order).unwrap_or_default();
        let _ = settings.set("speeddial.order", serde_json::json!(order.into_iter().take(64).collect::<Vec<_>>()));
        return Some((String::new(), String::new()));
    } else {
        return None;
    }
    let _ = settings.set("speeddial.pinned", serde_json::Value::Array(pinned));
    let _ = settings.set("speeddial.hidden", serde_json::json!(hidden));
    Some(out)
}

/// The speed dial page; `message` and `undo` (a query) show a bar on top.
/// (`in_private`: for a private window, where the history only shows if
/// "privacy.privateHistoryTiles" says so; the user's own tiles always do.)
pub fn render(message: &str, undo: &str, in_private: bool) -> String {
    let a = app();
    // The user's tiles first, then the most visited sites not removed.
    let pinned = list("speeddial.pinned");
    let hidden: Vec<String> = list("speeddial.hidden").iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
    let mut sites: Vec<(String, String, bool)> = pinned.iter().filter_map(|p| Some((p["url"].as_str()?.to_owned(), p["title"].as_str().unwrap_or("").to_owned(), true))).collect();
    let from_history = !in_private || a.shared.settings().get_bool("privacy.privateHistoryTiles");
    let top_sites = if from_history { kite_core::history::top_sites(&a.shared.db(), MAX_TILES * 3).unwrap_or_default() } else { vec![] };
    for s in top_sites {
        if sites.len() >= MAX_TILES.max(pinned.len()) {
            break;
        }
        if hidden.iter().any(|h| same_site(h, &s.url)) || sites.iter().any(|(u, _, _)| same_site(u, &s.url)) {
            continue;
        }
        sites.push((s.url, s.title, false));
    }
    // The order the user dragged the tiles into; tiles not in it keep theirs
    // after them.
    let order: Vec<String> = list("speeddial.order").iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
    let rank = |u: &str| order.iter().position(|o| same_site(o, u)).unwrap_or(usize::MAX);
    sites.sort_by_key(|(u, _, _)| rank(u));
    let engine = a.shared.settings().get_str("general.searchEngine");
    // "speeddial.showTiles" off: just the search box.
    let show_tiles = a.shared.settings().get_bool("speeddial.showTiles");
    if !show_tiles {
        sites.clear();
    }
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    let mut tiles: String = sites
        .iter()
        .map(|(url, title, mine)| {
            let host = kite_core::util::host_of(url);
            let title = if title.is_empty() { host.clone() } else { title.clone() };
            let letter = host.trim_start_matches("www.").chars().next().unwrap_or('?').to_uppercase().to_string();
            // Icons come from Kite's local cache (saved while browsing), so the
            // speed dial never contacts the sites; without one the letter shows.
            let icon = kite_core::favicons::key(url)
                .map(|k| format!(r#"<img src="kite:favicon?url={}" onerror="this.remove()">"#, esc(&percent_encoding::utf8_percent_encode(&k, percent_encoding::NON_ALPHANUMERIC).to_string())))
                .unwrap_or_default();
            format!(
                r#"<div class="cell" draggable="true" data-url="{url}"><a class="tile{mine}" href="{url}" title="{t}" draggable="false"><span class="ic">{icon}<b>{letter}</b></span><span class="t">{t}</span><span class="h">{h}</span></a><a class="x" href="?remove={eu}&amp;title={et}" title="Remove from the speed dial" aria-label="Remove">×</a></div>"#,
                mine = if *mine { " mine" } else { "" },
                url = esc(url),
                t = esc(&title),
                h = esc(&host),
                eu = esc(&enc(url)),
                et = esc(&enc(&title)),
                letter = esc(&letter)
            )
        })
        .collect();
    if show_tiles { tiles.push_str(r#"<div class="cell"><button class="tile add" type="button" onclick="document.getElementById('addform').hidden=false;document.getElementById('addurl').focus()" title="Add a site"><span class="ic"><b>+</b></span><span class="t">Add site</span></button></div>"#); }
    let empty = match (sites.is_empty() && show_tiles, from_history) {
        (false, _) => "",
        (true, true) => r#"<p class="empty">Sites you visit often will appear here.</p>"#,
        (true, false) => r#"<p class="empty">Sites you add will appear here.</p>"#,
    };
    let removed = REMOVED_THIS_RUN.lock().unwrap_or_else(|e| e.into_inner()).len();
    let restore = if removed == 0 || !show_tiles { String::new() } else { format!(r#"<p class="foot"><a href="?restoreAll">Show the {} removed site{} again</a></p>"#, removed, if removed == 1 { "" } else { "s" }) };
    let bar = if message.is_empty() { String::new() } else {
        let undo = if undo.is_empty() { String::new() } else { format!(r#" <a href="?{}">Undo</a>"#, esc(undo)) };
        format!(r#"<div class="bar">{}{undo}</div>"#, esc(message))
    };
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>New Tab</title>
<meta name="color-scheme" content="light dark">
<style>
:root {{ --bg:#eff0f1; --fg:#232629; --card:#fcfcfc; --muted:#707d8a; --accent:#3daee9; }}
@media (prefers-color-scheme: dark) {{ :root {{ --bg:#1b1e20; --fg:#fcfcfc; --card:#2a2e32; --muted:#a1a9b1; }} }}
body {{ margin:0; font-family: "Noto Sans", sans-serif; background:var(--bg); color:var(--fg); display:flex; flex-direction:column; align-items:center; min-height:100vh; }}
h1 {{ font-weight:300; margin:12vh 0 24px; font-size:42px; letter-spacing:1px; }}
h1 span {{ color:var(--accent); }}
form {{ width:min(640px, 90vw); }}
input {{ width:100%; box-sizing:border-box; font-size:18px; padding:12px 18px; border-radius:24px; border:1px solid color-mix(in srgb, var(--fg) 20%, transparent); background:var(--card); color:var(--fg); outline:none; }}
input:focus {{ border-color:var(--accent); box-shadow:0 0 0 3px color-mix(in srgb, var(--accent) 30%, transparent); }}
.grid {{ display:grid; grid-template-columns:repeat(auto-fill, 150px); gap:16px; width:min(860px, 92vw); justify-content:center; margin-top:40px; }}
.tile {{ background:var(--card); border-radius:10px; padding:16px 10px 12px; text-decoration:none; color:var(--fg); display:flex; flex-direction:column; align-items:center; gap:6px; box-shadow:0 1px 3px rgba(0,0,0,.12); transition:transform .1s; }}
.tile:hover {{ transform:translateY(-2px); box-shadow:0 4px 12px rgba(0,0,0,.18); }}
.ic {{ width:48px; height:48px; border-radius:12px; background:color-mix(in srgb, var(--accent) 18%, transparent); display:flex; align-items:center; justify-content:center; position:relative; overflow:hidden; }}
.ic img {{ width:32px; height:32px; position:absolute; background:var(--card); }}
.ic b {{ font-size:22px; color:var(--accent); }}
.t {{ font-size:13px; max-width:130px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }}
.h {{ font-size:11px; color:var(--muted); max-width:130px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }}
.empty {{ color:var(--muted); margin-top:40px; }}
.cell {{ position:relative; }}
.cell[draggable] {{ cursor:grab; }}
.cell.dragging {{ opacity:.35; }}
.cell .tile {{ width:100%; box-sizing:border-box; height:100%; }}
.mine .ic {{ box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--accent) 45%, transparent); }}
.x {{ position:absolute; top:4px; right:4px; width:22px; height:22px; border-radius:11px; display:flex; align-items:center; justify-content:center; text-decoration:none; color:var(--muted); font-size:16px; line-height:1; opacity:0; transition:opacity .1s; }}
.cell:hover .x, .x:focus {{ opacity:1; }}
.x:hover {{ background:color-mix(in srgb, var(--fg) 12%, transparent); color:var(--fg); }}
button.tile {{ border:2px dashed color-mix(in srgb, var(--fg) 18%, transparent); background:transparent; box-shadow:none; cursor:pointer; font:inherit; }}
button.tile:hover {{ border-color:var(--accent); }}
#addform {{ display:flex; gap:8px; width:min(640px, 90vw); margin-top:24px; }}
#addform[hidden] {{ display:none; }}
#addform input {{ font-size:15px; padding:8px 14px; }}
#addform button {{ font:inherit; padding:6px 16px; border-radius:18px; border:1px solid color-mix(in srgb, var(--fg) 20%, transparent); background:var(--card); color:var(--fg); cursor:pointer; }}
#addform button.ok {{ background:var(--accent); border-color:var(--accent); color:#fff; }}
.bar {{ position:fixed; top:16px; padding:8px 16px; border-radius:18px; background:var(--card); box-shadow:0 2px 8px rgba(0,0,0,.2); font-size:14px; }}
.bar a {{ color:var(--accent); margin-left:8px; }}
.foot {{ margin:32px 0; font-size:13px; }}
.foot a {{ color:var(--muted); }}
</style></head><body>
<h1>K<span>i</span>te</h1>
<form onsubmit="location.href='kite:search?q='+encodeURIComponent(this.q.value);return false;">
<input name="q" autofocus placeholder="Search with {engine} or enter address" autocomplete="off">
</form>
<form id="addform" hidden onsubmit="location.replace('?add='+encodeURIComponent(this.u.value)+'&title='+encodeURIComponent(this.n.value));return false;">
<input id="addurl" name="u" placeholder="Address, e.g. kde.org" autocomplete="off" required>
<input name="n" placeholder="Name (optional)" autocomplete="off">
<button class="ok" type="submit">Add</button><button type="button" onclick="this.form.hidden=true">Cancel</button>
</form>
<div class="grid">{tiles}</div>{empty}{restore}{bar}
<script>
// Edits come back as ?add=…/?remove=…: show the plain page address again,
// and make the edit links replace the page rather than add history.
if (location.search) history.replaceState(null, '', location.href.split('?')[0]);
document.addEventListener('click', e => {{ const a = e.target.closest('a[href^="?"]'); if (a) {{ e.preventDefault(); location.replace(a.getAttribute('href')); }} }});
setTimeout(() => {{ const b = document.querySelector('.bar'); if (b) b.remove(); }}, 8000);
// Drag tiles to reorder them; the new order is saved when dropped.
(() => {{
  const grid = document.querySelector('.grid');
  let dragged = null, start = '';
  const urls = () => [...grid.querySelectorAll('.cell[data-url]')].map(c => c.dataset.url);
  grid.addEventListener('dragstart', e => {{
    dragged = e.target.closest('.cell[data-url]');
    if (!dragged) return;
    start = JSON.stringify(urls());
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/x-kite-tile', dragged.dataset.url);
    setTimeout(() => dragged && dragged.classList.add('dragging'), 0);
  }});
  grid.addEventListener('dragover', e => {{
    if (!dragged) return;
    e.preventDefault();
    const over = e.target.closest('.cell[data-url]');
    if (!over || over === dragged) return;
    // Moving forward: after the tile it is over; backward: before it.
    const cells = [...grid.children];
    grid.insertBefore(dragged, cells.indexOf(dragged) < cells.indexOf(over) ? over.nextSibling : over);
  }});
  grid.addEventListener('drop', e => {{ if (dragged) e.preventDefault(); }});
  grid.addEventListener('dragend', () => {{
    if (!dragged) return;
    dragged.classList.remove('dragging');
    dragged = null;
    const now = JSON.stringify(urls());
    if (now !== start) location.replace('?order=' + encodeURIComponent(now));
  }});
}})();
</script>
</body></html>"#,
        engine = esc(&engine)
    )
}
