//! Updating GetCraft itself. GetCraft releases use the same asset naming as the Crafting Apps,
//! so the asset matcher and downloader are shared; only where the files go differs.

use crate::assets::PackageKind;
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The GitHub repository GetCraft is released from.
pub const REPO: &str = crate::trust::GETCRAFT_REPO;

/// The public half of the minisign key GetCraft releases are signed with, created by
/// `scripts/setup-update-signing.sh`. Every self-update must carry a valid signature from it;
/// without a key in this file, self-updates are refused.
const UPDATE_PUBLIC_KEY: &str = include_str!("../../../keys/update-signing.pub");

/// Checks the detached minisign `signature` of the downloaded update `package`, which must have
/// been signed as the release file `file_name` (so an older signed build can't be passed off as
/// a newer one).
pub fn verify_signature(package: &Path, file_name: &str, signature: &str) -> Result<()> {
    verify_with_key(UPDATE_PUBLIC_KEY, package, file_name, signature)
}

fn verify_with_key(public_key_file: &str, package: &Path, file_name: &str, signature: &str) -> Result<()> {
    use minisign_verify::{PublicKey, Signature};
    let key = public_key_file
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("RW"))
        .ok_or_else(|| Error::Install("this build has no update signing key, so it can't update itself".into()))?;
    let key = PublicKey::from_base64(key).map_err(|_| Error::Install("invalid update signing key".into()))?;
    let unsigned = || Error::Install("the update isn't signed by GetCraft; not installing it".into());
    let signature = Signature::decode(signature).map_err(|_| unsigned())?;
    key.verify(&fs::read(package)?, &signature, false).map_err(|_| unsigned())?;
    // minisign signs the trusted comment too; it records the file name, which includes the version.
    if !signature.trusted_comment().split('\t').any(|field| field == format!("file:{file_name}")) {
        return Err(Error::Install("the update's signature is for a different file".into()));
    }
    Ok(())
}

/// Where the running GetCraft is installed, in the form its updates replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// `…/GetCraft.app`
    MacApp(PathBuf),
    /// The folder holding `GetCraft.exe` (portable zip install).
    WindowsDir(PathBuf),
    /// The `.AppImage` file itself.
    AppImage(PathBuf),
}

/// Finds the running installation, or `None` for development builds (anything under a cargo
/// `target` directory) and installs we can't update in place (e.g. distro packages).
pub fn locate() -> Option<Location> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Some(Location::AppImage(appimage.into()));
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if exe.components().any(|c| c.as_os_str() == "target") {
        return None;
    }
    if cfg!(target_os = "macos") {
        // …/GetCraft.app/Contents/MacOS/GetCraft
        let app = exe.ancestors().nth(3)?;
        return app.extension().is_some_and(|e| e == "app").then(|| Location::MacApp(app.to_path_buf()));
    }
    if cfg!(windows) {
        return exe.parent().map(|dir| Location::WindowsDir(dir.to_path_buf()));
    }
    None
}

impl Location {
    pub fn package_kind(&self) -> PackageKind {
        match self {
            Location::MacApp(_) => PackageKind::Dmg,
            Location::WindowsDir(_) => PackageKind::PortableZip,
            Location::AppImage(_) => PackageKind::AppImage,
        }
    }

    /// Replaces the installation with `package` and returns what to launch afterwards. The
    /// running process keeps working (its files stay alive until it exits).
    /// `version` is the version being installed; on Windows it's shown in Settings → Apps.
    #[allow(unused_variables)]
    pub fn apply(&self, package: &Path, work_dir: &Path, version: &str) -> Result<PathBuf> {
        match self {
            #[cfg(target_os = "macos")]
            Location::MacApp(app) => {
                let dir = app.parent().ok_or_else(|| Error::Install("bad app location".into()))?;
                // On top of the minisign signature: Apple's signature must be ours too.
                crate::install::macos::install_dmg(package, dir, work_dir, &crate::trust::getcraft_identity())
            }
            #[cfg(windows)]
            Location::WindowsDir(dir) => {
                let exe = apply_windows(package, dir, work_dir)?;
                set_installed_version(version);
                Ok(exe)
            }
            Location::AppImage(file) => {
                let staged = file.with_extension("getcraft-new");
                fs::copy(package, &staged)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
                }
                fs::rename(&staged, file)?;
                let _ = work_dir;
                Ok(file.clone())
            }
            #[allow(unreachable_patterns)]
            _ => Err(Error::Install("self-update isn't supported here".into())),
        }
    }

    /// Removes leftovers from a previous update (Windows can't delete a running exe, so the old
    /// one is renamed and cleaned up on the next start).
    pub fn clean_up(&self) {
        if let Location::WindowsDir(dir) = self
            && let Ok(entries) = fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "getcraft-old") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[cfg(windows)]
fn apply_windows(zip_path: &Path, dir: &Path, work_dir: &Path) -> Result<PathBuf> {
    let staged = work_dir.join("self-update");
    crate::install::remove_path(&staged)?;
    crate::install::archive::extract_zip(zip_path, &staged, crate::install::archive::MAX_TOTAL_BYTES)?;
    // The zip holds a single `GetCraft/` folder.
    let entries: Vec<_> = fs::read_dir(&staged)?.flatten().collect();
    let root = match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => staged.clone(),
    };
    if !root.join("GetCraft.exe").is_file() {
        let _ = crate::install::remove_path(&staged);
        return Err(Error::Install("the update doesn't contain GetCraft.exe".into()));
    }
    for entry in fs::read_dir(&root)?.flatten().filter(|e| e.path().is_file()) {
        let dest = dir.join(entry.file_name());
        if dest.exists() {
            // A running exe can be renamed but not overwritten.
            let mut old = dest.clone().into_os_string();
            old.push(".getcraft-old");
            let old = PathBuf::from(old);
            let _ = fs::remove_file(&old);
            fs::rename(&dest, &old)?;
        }
        fs::rename(entry.path(), &dest)?;
    }
    let _ = crate::install::remove_path(&staged);
    Ok(dir.join("GetCraft.exe"))
}

/// The uninstall entry the Windows installer creates (see packaging/windows/getcraft.iss).
#[cfg(windows)]
const UNINSTALL_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7CF358B6-E104-44DA-AC2E-E883DB1AA9F1}_is1";

/// Keeps the version in Settings → Apps current after a self-update. Only touches the entry if
/// GetCraft was installed with the installer (portable copies have none).
#[cfg(windows)]
fn set_installed_version(version: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let reg = |args: &[&str]| {
        Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().is_ok_and(|o| o.status.success())
    };
    if reg(&["query", UNINSTALL_KEY]) {
        let ok = reg(&["add", UNINSTALL_KEY, "/v", "DisplayVersion", "/t", "REG_SZ", "/d", version, "/f"]);
        if !ok {
            log::warn!("could not update the installed version in Settings → Apps");
        }
    }
}

/// Starts the updated GetCraft once this process has exited; the caller should exit right away.
/// `hidden` keeps a background GetCraft in the background after the update.
pub fn relaunch(target: &Path, hidden: bool) -> Result<()> {
    let mut args = vec!["--after-update"];
    if hidden {
        args.push("--background");
    }
    #[cfg(target_os = "macos")]
    let spawned =
        Command::new("/bin/sh").arg("-c").arg("sleep 1; open -n \"$0\" --args \"$@\"").arg(target).args(&args).spawn();
    #[cfg(windows)]
    let spawned = Command::new("cmd")
        .args(["/C", "timeout", "/T", "1", "/NOBREAK", ">NUL", "&", "start", "\"\""])
        .arg(target)
        .args(&args)
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = Command::new("/bin/sh").arg("-c").arg("sleep 1; exec \"$0\" \"$@\"").arg(target).args(&args).spawn();
    spawned.map(drop).map_err(|e| Error::Install(format!("could not restart GetCraft: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(dir: &Path, file_name: &str, data: &[u8]) -> (String, PathBuf, String) {
        let keys = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let path = dir.join(file_name);
        fs::write(&path, data).unwrap();
        let comment = format!("timestamp:1\tfile:{file_name}\thashed");
        let sig = minisign::sign(Some(&keys.pk), &keys.sk, data, Some(&comment), None).unwrap();
        let public = format!("untrusted comment: test key\n{}\n", keys.pk.to_base64());
        (public, path, sig.into_string())
    }

    #[test]
    fn accepts_a_correct_signature() {
        let dir = tempfile::tempdir().unwrap();
        let (key, path, sig) = signed(dir.path(), "getcraft-0.2.0-linux-x86_64.AppImage", b"build");
        verify_with_key(&key, &path, "getcraft-0.2.0-linux-x86_64.AppImage", &sig).unwrap();
    }

    #[test]
    fn rejects_tampering_and_mismatches() {
        let dir = tempfile::tempdir().unwrap();
        let (key, path, sig) = signed(dir.path(), "getcraft-0.2.0-linux-x86_64.AppImage", b"build");
        // Signed for another (older) file.
        assert!(verify_with_key(&key, &path, "getcraft-0.3.0-linux-x86_64.AppImage", &sig).is_err());
        // Modified contents.
        fs::write(&path, b"evil").unwrap();
        assert!(verify_with_key(&key, &path, "getcraft-0.2.0-linux-x86_64.AppImage", &sig).is_err());
        // Signed by another key.
        let (other_key, other_path, _) = signed(dir.path(), "x", b"build");
        let (_, _, other_sig) = signed(dir.path(), "getcraft-0.2.0-linux-x86_64.AppImage", b"build");
        assert!(verify_with_key(&other_key, &other_path, "getcraft-0.2.0-linux-x86_64.AppImage", &other_sig).is_err());
        // Garbage.
        assert!(verify_with_key(&key, &path, "x", "not a signature").is_err());
    }

    #[test]
    fn refuses_without_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path, sig) = signed(dir.path(), "f", b"build");
        assert!(verify_with_key("untrusted comment: not configured\n", &path, "f", &sig).is_err());
    }
}
