//! Sending addresses to devices, through the KDE Connect daemon on the session bus.
//!
//! The daemon isn't started for this: without it running there are just no devices.
//! Every call may wait for it, so they all run on a thread of their own.

use serde::Serialize;
use std::collections::HashMap;
use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::Connection;
use zbus::names::BusName;
use zbus::zvariant::OwnedValue;

const SERVICE: &str = "org.kde.kdeconnect";
const DAEMON_PATH: &str = "/modules/kdeconnect";
const DAEMON: &str = "org.kde.kdeconnect.daemon";
const DEVICE: &str = "org.kde.kdeconnect.device";
const SHARE: &str = "org.kde.kdeconnect.device.share";

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub icon: String,
}

/// An icon for the device's type, as the daemon names those.
fn icon_for(kind: &str) -> &'static str {
    match kind {
        "smartphone" | "phone" => "smartphone",
        "tablet" => "tablet",
        "tv" => "video-television",
        "laptop" => "computer-laptop",
        _ => "computer",
    }
}

/// Fit for a menu. A device names itself, and Qt shows what looks like markup as that.
fn label(name: &str) -> String {
    name.chars().filter(|c| *c != '<' && *c != '&' && !c.is_control()).take(80).collect()
}

fn string(props: &HashMap<String, OwnedValue>, key: &str) -> String {
    props.get(key).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default()
}

/// Paired devices that can be reached now.
pub fn devices() -> anyhow::Result<Vec<Device>> {
    let connection = Connection::session()?;
    let service = BusName::try_from(SERVICE)?;
    if !DBusProxy::new(&connection)?.name_has_owner(service)? {
        return Ok(Vec::new());
    }
    let reply = connection.call_method(Some(SERVICE), DAEMON_PATH, Some(DAEMON), "devices", &(true, true))?;
    let ids: Vec<String> = reply.body().deserialize()?;
    let mut out = Vec::new();
    for id in ids {
        let path = format!("{DAEMON_PATH}/devices/{id}");
        let reply = connection.call_method(Some(SERVICE), path.as_str(), Some("org.freedesktop.DBus.Properties"), "GetAll", &(DEVICE,))?;
        let props: HashMap<String, OwnedValue> = reply.body().deserialize()?;
        let name = label(&string(&props, "name"));
        out.push(Device { name: if name.is_empty() { id.clone() } else { name }, icon: icon_for(&string(&props, "type")).to_owned(), id });
    }
    Ok(out)
}

/// A device id goes into a bus path, so only what can be one.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Which addresses a device can be sent. Its phone opens them, unasked.
pub fn can_send(url: &str) -> bool {
    ["https://", "http://", "tel:", "mailto:", "geo:"].iter().any(|s| url.starts_with(s))
}

pub fn share(device: &str, url: &str) -> anyhow::Result<()> {
    anyhow::ensure!(valid_id(device), "not a device: {device}");
    anyhow::ensure!(can_send(url), "not something to send to a device");
    let connection = Connection::session()?;
    let path = format!("{DAEMON_PATH}/devices/{device}/share");
    connection.call_method(Some(SERVICE), path.as_str(), Some(SHARE), "shareUrl", &(url,))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_are_plain_text() {
        assert_eq!(label("Pixel <img src=x> & co\n"), "Pixel img src=x>  co");
        assert_eq!(label(&"x".repeat(500)).len(), 80);
    }

    #[test]
    fn device_ids_fit_a_bus_path() {
        assert!(valid_id("a1b2_c3"));
        for bad in ["", "../x", "a/b", "a b", "a-b"] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn only_addresses_meant_for_elsewhere_are_sent() {
        for good in ["https://kde.org/", "http://x/", "tel:+123", "mailto:a@b", "geo:1,2"] {
            assert!(can_send(good), "{good}");
        }
        for bad in ["file:///etc/passwd", "kite:config", "javascript:alert(1)", "blob:https://x/1", "data:text/html,x", ""] {
            assert!(!can_send(bad), "{bad}");
        }
    }

    #[test]
    fn device_types_have_icons() {
        assert_eq!(icon_for("smartphone"), "smartphone");
        assert_eq!(icon_for("something new"), "computer");
    }
}
