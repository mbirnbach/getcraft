//! Zip extraction with limits, for packages from the network.

use crate::{Error, Result};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

/// No real app package comes close to these; they stop zip bombs and broken archives.
pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Extracts `zip_path` into `dest`. Rejects entries that would land outside `dest` (absolute
/// paths, `..`), symlinks, more than `MAX_ENTRIES` entries and more than `max_bytes` of content,
/// counting the bytes actually written rather than the sizes the archive claims.
pub fn extract_zip(zip_path: &Path, dest: &Path, max_bytes: u64) -> Result<()> {
    let bad = |what: &str| Error::Install(format!("the download is not a valid package ({what})"));
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?).map_err(|_| bad("not a zip file"))?;
    if archive.len() > MAX_ENTRIES {
        return Err(bad("too many files"));
    }
    fs::create_dir_all(dest)?;
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| bad("unreadable entry"))?;
        // zip would quietly make absolute paths relative; a real package never has them.
        let raw = entry.name();
        if raw.starts_with(['/', '\\']) || raw.as_bytes().get(1) == Some(&b':') {
            return Err(bad("a file path is absolute"));
        }
        let relative = entry.enclosed_name().ok_or_else(|| bad("a file path points outside the package"))?;
        if entry.is_symlink() {
            return Err(bad("contains a symbolic link"));
        }
        let out_path = dest.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        let remaining = max_bytes.saturating_sub(total);
        if entry.size() > remaining {
            return Err(bad("too large when unpacked"));
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        let written = io::copy(&mut entry.by_ref().take(remaining + 1), &mut out)?;
        if written > remaining {
            return Err(bad("too large when unpacked"));
        }
        total += written;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(dir: &Path, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = dir.join("test.zip");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn extracts_normal_packages() {
        let dir = tempfile::tempdir().unwrap();
        let zip = make_zip(dir.path(), &[("app/app.exe", b"MZ"), ("app/README.txt", b"hi")]);
        let out = dir.path().join("out");
        extract_zip(&zip, &out, 1024).unwrap();
        assert_eq!(fs::read(out.join("app/app.exe")).unwrap(), b"MZ");
    }

    #[test]
    fn rejects_paths_outside_the_target() {
        let dir = tempfile::tempdir().unwrap();
        for evil in ["../evil.exe", "/etc/evil", "C:/evil.exe", "\\\\server\\evil.exe", "app/../../evil.exe"] {
            let zip = make_zip(dir.path(), &[(evil, b"x")]);
            let out = dir.path().join("out");
            assert!(extract_zip(&zip, &out, 1024).is_err(), "{evil}");
            assert!(!dir.path().join("evil.exe").exists());
        }
    }

    #[test]
    fn rejects_oversized_content() {
        let dir = tempfile::tempdir().unwrap();
        let zip = make_zip(dir.path(), &[("a.bin", &[0u8; 600]), ("b.bin", &[0u8; 600])]);
        assert!(extract_zip(&zip, &dir.path().join("out"), 1000).is_err());
    }

    #[test]
    fn rejects_non_zips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.zip");
        fs::write(&path, b"not a zip").unwrap();
        assert!(extract_zip(&path, &dir.path().join("out"), 1000).is_err());
    }
}
