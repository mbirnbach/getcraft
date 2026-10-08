//! Updating GetCraft itself. GetCraft releases use the same asset naming as the Crafting Apps,
//! so the asset matcher and downloader are shared; only where the files go differs.

use crate::assets::PackageKind;
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The GitHub repository GetCraft is released from.
pub const REPO: &str = "mbirnbach/getcraft";

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
    pub fn apply(&self, package: &Path, work_dir: &Path) -> Result<PathBuf> {
        match self {
            #[cfg(target_os = "macos")]
            Location::MacApp(app) => {
                let dir = app.parent().ok_or_else(|| Error::Install("bad app location".into()))?;
                crate::install::macos::install_dmg(package, dir, work_dir)
            }
            #[cfg(windows)]
            Location::WindowsDir(dir) => apply_windows(package, dir, work_dir),
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
    let mut archive = zip::ZipArchive::new(fs::File::open(zip_path)?)
        .map_err(|e| Error::Install(format!("the update isn't a valid zip: {e}")))?;
    archive.extract(&staged).map_err(|e| Error::Install(format!("unpacking the update failed: {e}")))?;
    // The zip holds a single `GetCraft/` folder.
    let entries: Vec<_> = fs::read_dir(&staged)?.flatten().collect();
    let root = match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => staged.clone(),
    };
    for entry in fs::read_dir(&root)?.flatten() {
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
