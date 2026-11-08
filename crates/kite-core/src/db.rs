//! `browsedata.db` — the same SQLite schema Falkon uses, so a Falkon
//! profile's database can be dropped into a Kite profile and just work.

use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS autofill (
    id INTEGER PRIMARY KEY, server TEXT NOT NULL, data TEXT, password TEXT,
    username TEXT, last_used INTEGER DEFAULT 0 NOT NULL);
CREATE INDEX IF NOT EXISTS autofill_serverindex ON autofill (server);
CREATE TABLE IF NOT EXISTS autofill_exceptions (id INTEGER PRIMARY KEY, server TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS autofill_exception_serverindex ON autofill_exceptions (server);
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY, url TEXT NOT NULL, title TEXT,
    date INTEGER DEFAULT 0 NOT NULL, count INTEGER DEFAULT 0 NOT NULL);
CREATE INDEX IF NOT EXISTS history_titleindex ON history (title);
CREATE UNIQUE INDEX IF NOT EXISTS history_urluniqueindex ON history (url);
CREATE TABLE IF NOT EXISTS kite_cert_exceptions (
    host TEXT NOT NULL, fingerprint TEXT NOT NULL, description TEXT, added INTEGER, UNIQUE (host, fingerprint));
CREATE TABLE IF NOT EXISTS search_engines (
    id INTEGER PRIMARY KEY, name TEXT NOT NULL, url TEXT, icon BLOB, shortcut TEXT,
    suggestionsUrl TEXT, suggestionsParameters TEXT, postData TEXT);
CREATE TABLE IF NOT EXISTS icons (id INTEGER PRIMARY KEY, url TEXT NOT NULL, icon BLOB);
CREATE UNIQUE INDEX IF NOT EXISTS icons_urluniqueindex ON icons (url);
CREATE TABLE IF NOT EXISTS site_settings (
    id INTEGER PRIMARY KEY, server TEXT NOT NULL,
    zoom_level INTEGER DEFAULT -1, allow_cookies INTEGER DEFAULT 0,
    wa_autoload_images INTEGER DEFAULT 0, wa_js_enabled INTEGER DEFAULT 0,
    wa_js_open_windows INTEGER DEFAULT 0, wa_js_access_clipboard INTEGER DEFAULT 0,
    wa_js_can_paste INTEGER DEFAULT 0, wa_js_window_activation INTEGER DEFAULT 0,
    wa_local_storage INTEGER DEFAULT 0, wa_fullscreen_support INTEGER DEFAULT 0,
    wa_run_insecure_content INTEGER DEFAULT 0, wa_playback_needs_gesture INTEGER DEFAULT 0,
    wa_reading_from_canvas INTEGER DEFAULT 0, wa_force_dark_mode INTEGER DEFAULT 0,
    f_notifications INTEGER DEFAULT 0, f_geolocation INTEGER DEFAULT 0,
    f_media_audio_capture INTEGER DEFAULT 0, f_media_video_capture INTEGER DEFAULT 0,
    f_media_audio_video_capture INTEGER DEFAULT 0, f_mouse_lock INTEGER DEFAULT 0,
    f_desktop_video_capture INTEGER DEFAULT 0, f_desktop_audio_video_capture INTEGER DEFAULT 0);
CREATE UNIQUE INDEX IF NOT EXISTS site_settings_serveruniqueindex ON site_settings (server);
"#;

pub fn open(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(SCHEMA)?;
    upgrade(&conn)?;
    Ok(conn)
}

/// Columns Kite added to Falkon's schema, for databases created before them.
fn upgrade(conn: &Connection) -> anyhow::Result<()> {
    let has = |table: &str, col: &str| -> anyhow::Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names: Vec<String> = stmt.query_map([], |r| r.get(1))?.collect::<Result<_, _>>()?;
        Ok(names.iter().any(|n| n == col))
    };
    for (col, kind) in [("kite_css", "TEXT DEFAULT ''"), ("kite_js", "TEXT DEFAULT ''"), ("kite_fp", "INTEGER DEFAULT 0"), ("kite_midi", "INTEGER DEFAULT 0"), ("kite_midi_sysex", "INTEGER DEFAULT 0"), ("kite_ua", "TEXT DEFAULT ''")] {
        if !has("site_settings", col)? {
            conn.execute_batch(&format!("ALTER TABLE site_settings ADD COLUMN {col} {kind}"))?;
        }
    }
    Ok(())
}

pub fn open_in_memory() -> anyhow::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    upgrade(&conn)?;
    Ok(conn)
}
