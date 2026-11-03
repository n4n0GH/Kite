//! `kite:start` — the new-tab / speed-dial page (Falkon's falkon:speeddial).

use crate::state::app;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn render() -> String {
    let a = app();
    let sites = kite_core::history::top_sites(&a.shared.db(), 12).unwrap_or_default();
    let engine = a.shared.settings().get_str("general.searchEngine");
    let tiles: String = sites
        .iter()
        .map(|s| {
            let host = kite_core::util::host_of(&s.url);
            let title = if s.title.is_empty() { host.clone() } else { s.title.clone() };
            let letter = host.trim_start_matches("www.").chars().next().unwrap_or('?').to_uppercase().to_string();
            // Icons come from Kite's local cache (saved while browsing), so the
            // speed dial never contacts the sites; without one the letter shows.
            let icon = kite_core::favicons::key(&s.url)
                .map(|k| format!(r#"<img src="kite:favicon?url={}" onerror="this.remove()">"#, esc(&percent_encoding::utf8_percent_encode(&k, percent_encoding::NON_ALPHANUMERIC).to_string())))
                .unwrap_or_default();
            format!(
                r#"<a class="tile" href="{url}" title="{t}"><span class="ic">{icon}<b>{letter}</b></span><span class="t">{t}</span><span class="h">{h}</span></a>"#,
                url = esc(&s.url),
                t = esc(&title),
                h = esc(&host),
                letter = esc(&letter)
            )
        })
        .collect();
    let empty = if sites.is_empty() { r#"<p class="empty">Sites you visit often will appear here.</p>"# } else { "" };
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
</style></head><body>
<h1>K<span>i</span>te</h1>
<form onsubmit="location.href='kite:search?q='+encodeURIComponent(this.q.value);return false;">
<input name="q" autofocus placeholder="Search with {engine} or enter address" autocomplete="off">
</form>
<div class="grid">{tiles}</div>{empty}
</body></html>"#,
        engine = esc(&engine)
    )
}
