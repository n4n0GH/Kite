#![recursion_limit = "256"]
//! Kite browser backend.
//!
//! Everything that does not need a GUI lives here so it can be unit tested
//! without Qt: profile handling, history, bookmarks, sessions, search engines,
//! content blocking and the WebExtension runtime (manifest parsing, XPI
//! installation from addons.mozilla.org, storage and API dispatch).

pub mod adblock;
pub mod bookmarks;
pub mod db;
pub mod downloads;
pub mod favicons;
pub mod history;
pub mod kwallet;
pub mod passwords;
pub mod paths;
pub mod screensaver;
pub mod search;
pub mod session;
pub mod settings;
pub mod shared;
pub mod sitesettings;
pub mod standin;
pub mod util;
pub mod webext;

pub use paths::Profile;
pub use rusqlite;
