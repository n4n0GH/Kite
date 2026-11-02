//! Developer tool: install a Firefox add-on from addons.mozilla.org into a
//! Kite profile without the GUI.
//!
//!     KITE_DATA_DIR=/tmp/kite cargo run -p kite-core --example amo-install -- darkreader [profile]

use kite_core::webext::{amo, registry::Registry};

fn main() -> anyhow::Result<()> {
    let slug = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: amo-install <slug|url> [profile]"))?;
    let profile = kite_core::Profile::open(&std::env::args().nth(2).unwrap_or_else(|| "default".into()))?;
    let d = amo::details(&amo::slug_from_url(&slug).unwrap_or(slug))?;
    println!("{} {} ({}) — {} users", d.name, d.version, d.guid, d.users);
    println!("permissions: {:?} {:?}", d.permissions, d.host_permissions);
    let bytes = amo::download(&d.file_url, &d.file_hash)?;
    println!("downloaded {} bytes, checksum ok", bytes.len());
    let m = Registry::inspect(&bytes)?;
    println!("manifest v{}: {} background={:?} content_scripts={} action={}", m.manifest_version, m.name,
        m.background.as_ref().map(|b| (&b.scripts, &b.page)), m.content_scripts.len(), m.action.is_some());
    for w in m.permission_warnings() {
        println!("  - {w}");
    }
    let mut reg = Registry::open(profile.extensions_dir());
    let r = reg.install_xpi(&bytes, "amo", &d.slug)?;
    println!("installed {} {} (updated: {}) into {}", r.name, r.version, r.updated, profile.root.display());
    Ok(())
}
