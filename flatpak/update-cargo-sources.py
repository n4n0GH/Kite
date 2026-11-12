#!/usr/bin/env python3
# Writes flatpak/cargo-sources.json from Cargo.lock: every crate the build
# needs, as a source flatpak-builder downloads and checks, so that cargo can
# build offline in the sandbox. Run it after Cargo.lock changes:
#
#   flatpak/update-cargo-sources.py
#
# The same thing flatpak-builder-tools' flatpak-cargo-generator does for
# crates from crates.io, without its dependencies: the checksums are in
# Cargo.lock already. Crates from git repositories aren't handled; Kite has none.

import json
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
lock = tomllib.loads((root / "Cargo.lock").read_text())
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"

sources = []
for package in lock["package"]:
    source = package.get("source")
    if source is None:
        continue  # Kite's own crates, in the tree
    if source != CRATES_IO:
        sys.exit(f"{package['name']} {package['version']} comes from {source}: only crates.io is handled")
    name, version, checksum = package["name"], package["version"], package["checksum"]
    dest = f"cargo/vendor/{name}-{version}"
    sources.append({
        "type": "archive",
        "archive-type": "tar-gzip",
        "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
        "sha256": checksum,
        "dest": dest,
    })
    # What cargo checks a vendored crate against.
    sources.append({
        "type": "inline",
        "contents": json.dumps({"package": checksum, "files": {}}),
        "dest": dest,
        "dest-filename": ".cargo-checksum.json",
    })

# Points cargo at the vendored crates: CARGO_HOME is set to this folder.
sources.append({
    "type": "inline",
    "contents": '[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "cargo/vendor"\n',
    "dest": "cargo",
    "dest-filename": "config.toml",
})

out = root / "flatpak" / "cargo-sources.json"
out.write_text(json.dumps(sources, indent=2) + "\n")
print(f"{out}: {len(sources) // 2} crates")
