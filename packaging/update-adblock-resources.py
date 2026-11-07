#!/usr/bin/env python3
"""Makes crates/kite-core/resources/ublock-resources.json from a checkout of uBlock Origin:

    git clone --depth 1 https://github.com/gorhill/uBlock
    packaging/update-adblock-resources.py uBlock

These are uBO's scriptlets and redirect resources (GPLv3, as Kite is under a compatible
licence), which its filter lists rely on, in the form Brave's adblock engine takes. Lists are
updated while Kite runs, but these only with Kite, so refresh them when uBO's lists start
naming scriptlets this file doesn't have. It reports what it can't find.

The source is only read as text. Each file's top-level functions and classes end at the first
line that is just "}", as uBO's source is formatted; a change there shows up as a syntax error
when running what this makes, or as what's reported missing.
"""
import base64
import json
import re
import subprocess
import sys
from pathlib import Path

MIMES = {
    ".js": "application/javascript", ".css": "text/css", ".html": "text/html", ".txt": "text/plain",
    ".json": "application/json", ".xml": "text/xml", ".gif": "image/gif", ".png": "image/png",
    ".mp3": "audio/mp3", ".mp4": "video/mp4", ".vtt": "text/plain", ".wav": "audio/wav",
}
TRUSTED = 1  # kite_core::adblock: the permission of lists that may use these


def top_level(source):
    """The functions and classes at the top of a file, by name."""
    found = {}
    lines = source.split("\n")
    for at, line in enumerate(lines):
        m = re.match(r"(?:export\s+)?((?:async\s+)?function\s*\*?\s*(\w+)\s*\(.*|class\s+(\w+)\b.*)", line)
        if not m:
            continue
        end = next((i for i in range(at, len(lines)) if lines[i] == "}"), None)
        if end is None:
            continue
        body = [m.group(1)] + lines[at + 1 : end + 1]
        found[m.group(2) or m.group(3)] = "\n".join(body)
    return found


def field(block, name):
    m = re.search(rf"^\s*{name}:\s*'([^']*)'", block, re.M)
    return m.group(1) if m else None


def listed(block, name):
    m = re.search(rf"^\s*{name}:\s*\[(.*?)\]", block, re.M | re.S)
    if not m:
        return []
    return [t.strip().strip("'\"") for t in m.group(1).split(",") if t.strip()]


def scriptlets(resources):
    code = {}
    entries = []
    for path in sorted(resources.glob("*.js")):
        source = path.read_text()
        code.update(top_level(source))
        for fn, block in re.findall(r"^registerScriptlet\((\w+)\s*,\s*\{\n(.*?)^\}\);", source, re.M | re.S):
            entries.append((fn, block))
        for block in re.findall(r"^builtinScriptlets\.push\(\{\n(.*?)^\}\);", source, re.M | re.S):
            m = re.search(r"^\s*fn:\s*(\w+)", block, re.M)
            if m:
                entries.append((m.group(1), block))
    # What some of them use from uBO's main code.
    for shared in ("jsonpath.js", "urlskip.js", "arglist-parser.js"):
        code.update(top_level((resources.parent / shared).read_text()))
    names = {fn: field(block, "name") for fn, block in entries}
    out = []
    problems = []
    for fn, block in entries:
        name = field(block, "name")
        if fn not in code or not name:
            problems.append(f"no source for {fn} ({name})")
            continue
        deps = []
        for dep in listed(block, "dependencies"):
            resolved = dep if dep.endswith((".fn", ".js")) else names.get(dep)
            if resolved:
                deps.append(resolved)
            else:
                problems.append(f"{name}: unknown dependency {dep}")
        entry = {
            "name": name,
            "aliases": listed(block, "aliases"),
            "kind": {"mime": "fn/javascript" if name.endswith(".fn") else "application/javascript"},
            "content": base64.b64encode(code[fn].encode()).decode(),
            "dependencies": deps,
        }
        if re.search(r"^\s*requiresTrust:\s*true", block, re.M):
            entry["permission"] = TRUSTED
        out.append(entry)
    return out, problems


def redirects(root):
    table = (root / "src/js/redirect-resources.js").read_text()
    out = []
    problems = []
    folder = root / "src/web_accessible_resources"
    for name, body in re.findall(r"\[\s*'([^']+)',\s*\{(.*?)\}\s*\]", table, re.S):
        path = folder / name
        if not path.is_file():
            problems.append(f"no file for redirect resource {name}")
            continue
        one = re.search(r"alias:\s*'([^']+)'", body)
        many = re.search(r"alias:\s*\[(.*?)\]", body, re.S)
        aliases = [one.group(1)] if one else [t.strip().strip("'\"") for t in many.group(1).split(",") if t.strip()] if many else []
        out.append({
            "name": name,
            "aliases": aliases,
            "kind": {"mime": MIMES.get(path.suffix, "application/octet-stream")},
            "content": base64.b64encode(path.read_bytes()).decode(),
        })
    return out, problems


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    root = Path(sys.argv[1])
    made, problems = scriptlets(root / "src/js/resources")
    more, others = redirects(root)
    problems += others
    # A name only once, the scriptlet winning: the engine refuses duplicates.
    taken = set()
    resources = []
    for entry in made + more:
        entry["aliases"] = [a for a in entry["aliases"] if a not in taken and a != entry["name"]]
        if entry["name"] in taken:
            problems.append(f"{entry['name']} comes twice")
            continue
        taken.add(entry["name"])
        taken.update(entry["aliases"])
        resources.append(entry)
    target = Path(__file__).resolve().parent.parent / "crates/kite-core/resources/ublock-resources.json"
    target.parent.mkdir(exist_ok=True)
    commit = subprocess.run(["git", "-C", str(root), "log", "-1", "--format=%h %cs"], capture_output=True, text=True).stdout.strip()
    target.write_text(json.dumps({"from": f"uBlock Origin {commit}", "resources": resources}, separators=(",", ":")))
    print(f"{len(made)} scriptlets and {len(more)} redirect resources from uBlock Origin {commit}: {target.stat().st_size} bytes")
    for p in problems:
        print("  ", p)


if __name__ == "__main__":
    main()
