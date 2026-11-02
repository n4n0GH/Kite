//! Unpacking `.xpi` packages (zip files) safely.

use std::io::{Cursor, Read};
use std::path::Path;

/// Upper bounds to protect against zip bombs.
const MAX_FILES: usize = 20_000;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;

/// Read `manifest.json` from XPI bytes without extracting.
pub fn read_manifest(bytes: &[u8]) -> anyhow::Result<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| anyhow::anyhow!("not a valid .xpi/zip file: {e}"))?;
    let mut f = zip.by_name("manifest.json").map_err(|_| anyhow::anyhow!("the package has no manifest.json"))?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

/// Extract XPI bytes into `dest` (which must not exist yet).
pub fn extract(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    if zip.len() > MAX_FILES {
        anyhow::bail!("package contains too many files");
    }
    std::fs::create_dir_all(dest)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        // enclosed_name rejects absolute paths and `..` traversal.
        let Some(rel) = f.enclosed_name() else {
            anyhow::bail!("package contains an unsafe path: {:?}", f.name().unwrap_or_default());
        };
        let out = dest.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        total += f.size();
        if total > MAX_TOTAL {
            anyhow::bail!("package is too large when unpacked");
        }
        let mut w = std::fs::File::create(&out)?;
        std::io::copy(&mut (&mut f).take(MAX_TOTAL), &mut w)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn make_xpi(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        for (name, body) in files {
            z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extract_and_reject_traversal() {
        let x = make_xpi(&[("manifest.json", "{}"), ("a/b.js", "1")]);
        assert_eq!(read_manifest(&x).unwrap(), "{}");
        let d = tempfile::tempdir().unwrap();
        extract(&x, &d.path().join("e")).unwrap();
        assert_eq!(std::fs::read_to_string(d.path().join("e/a/b.js")).unwrap(), "1");
        let bad = make_xpi(&[("../evil.js", "x")]);
        assert!(extract(&bad, &d.path().join("f")).is_err());
        assert!(read_manifest(b"not a zip").is_err());
    }
}
