use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Packages larger than this are refused whatever the release metadata claims.
pub const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Streams `url` to `dest`, reporting `(bytes_done, bytes_total)` as it goes.
///
/// The download must match `expected_sha256` and be exactly `expected_size` bytes (the size the
/// release lists for the file). It's written to `dest.part` and only renamed into place once
/// complete and verified, so a half-finished or tampered download is never mistaken for a good one.
pub fn download(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    expected_size: u64,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    if expected_size == 0 || expected_size > MAX_PACKAGE_BYTES {
        return Err(Error::Install(format!("refusing a download of {expected_size} bytes")));
    }
    let resp = agent.get(url).call()?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(Error::Http(format!("download failed with HTTP {status}")));
    }
    let total: Option<u64> =
        resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
    if total.is_some_and(|t| t != expected_size) {
        return Err(Error::Install("the download's size doesn't match the release".into()));
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = dest.with_extension("part");
    let result = (|| {
        // The body is bounded by the size check below, not by ureq's default limit.
        let mut reader = resp.into_body().into_reader();
        let mut out = BufWriter::new(File::create(&part)?);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            done += n as u64;
            if done > expected_size {
                return Err(Error::Install("the download is larger than the release says".into()));
            }
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            progress(done, Some(expected_size));
        }
        if done != expected_size {
            return Err(Error::Install("the download ended early".into()));
        }
        let file = out.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(Error::Checksum { expected: expected_sha256.to_owned(), actual });
        }
        Ok(())
    })();

    match result {
        Ok(()) => Ok(fs::rename(&part, dest)?),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}
