use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Streams `url` to `dest`, reporting `(bytes_done, bytes_total)` as it goes.
///
/// The file is written to `dest.part` and only renamed into place once complete and (when
/// `expected_sha256` is given) verified, so a half-finished download is never mistaken for a
/// good one.
pub fn download(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    let resp = agent.get(url).call()?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(Error::Http(format!("download failed with HTTP {status}")));
    }
    let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = dest.with_extension("part");
    let result = (|| {
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
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            done += n as u64;
            progress(done, total);
        }
        out.flush()?;
        if let Some(expected) = expected_sha256 {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(Error::Checksum { expected: expected.to_owned(), actual });
            }
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
