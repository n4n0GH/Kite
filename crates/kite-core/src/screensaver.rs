//! Keeping the screen on for pages holding a Screen Wake Lock.
//!
//! Uses the freedesktop screen saver API (`org.freedesktop.ScreenSaver.Inhibit`),
//! one inhibition per holder (a tab). On Plasma 6 KWin implements it and
//! hands it on to PowerDevil, which (after its ~5 s delay for short-lived
//! inhibitions) blocks screen dimming, turning the screen off and suspend,
//! and lists Kite in the battery applet. The inhibitions belong to our D-Bus
//! connection, so they end with Kite even if it crashes.

use std::collections::HashMap;
use zbus::blocking::Connection;

const SERVICE: &str = "org.freedesktop.ScreenSaver";
const PATH: &str = "/org/freedesktop/ScreenSaver";

#[derive(Default)]
pub struct ScreenInhibitor {
    conn: Option<Connection>,
    /// holder → inhibition cookie
    held: HashMap<i64, u32>,
}

impl ScreenInhibitor {
    /// Keep the screen on for `holder` (or stop). `reason` is shown by the
    /// desktop (e.g. in Plasma's battery applet).
    pub fn set(&mut self, holder: i64, on: bool, reason: &str) -> anyhow::Result<()> {
        if on {
            if self.held.contains_key(&holder) {
                return Ok(());
            }
            let conn = match &self.conn {
                Some(c) => c,
                None => self.conn.insert(Connection::session()?),
            };
            let reply = conn.call_method(Some(SERVICE), PATH, Some(SERVICE), "Inhibit", &("Kite", reason))?;
            let cookie: u32 = reply.body().deserialize()?;
            self.held.insert(holder, cookie);
        } else if let Some(cookie) = self.held.remove(&holder) {
            if let Some(conn) = &self.conn {
                conn.call_method(Some(SERVICE), PATH, Some(SERVICE), "UnInhibit", &(cookie,))?;
            }
        }
        Ok(())
    }

    pub fn is_held(&self, holder: i64) -> bool {
        self.held.contains_key(&holder)
    }
}
