#![recursion_limit = "512"]
//! Kite browser backend.
//!
//! Everything that does not need a GUI lives here so it can be unit tested
//! without Qt: profile handling, history, bookmarks, sessions, search engines,
//! content blocking and the WebExtension runtime (manifest parsing, XPI
//! installation from addons.mozilla.org, storage and API dispatch).

pub mod adblock;
pub mod backup;
pub mod bookmarks;
pub mod browser_import;
pub mod certs;
pub mod certview;
pub mod config;
pub mod crashlog;
pub mod db;
pub mod downloads;
pub mod favicons;
pub mod history;
pub mod instance;
pub mod kwallet;
pub mod kwin;
pub mod midi;
pub mod formfill;
pub mod passwords;
pub mod plugins;
pub mod privacy;
pub mod paths;
pub mod plasma;
pub mod profiles;
pub mod reader;
pub mod screensaver;
pub mod safebrowsing;
pub mod search;
pub mod session;
pub mod settings;
pub mod shared;
pub mod sitesettings;
pub mod standin;
pub mod startpage;
pub mod util;
pub mod webext;
pub mod welcome;

pub use paths::Profile;
pub use rusqlite;
