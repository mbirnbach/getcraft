use super::{remove_path, swap_into_place};
use crate::catalog::Tool;
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `/Applications` when the user can write to it (admins can, without a password prompt),
/// otherwise `~/Applications`. Both show up in Launchpad and Spotlight.
pub fn default_apps_dir() -> PathBuf {
    let system = PathBuf::from("/Applications");
    let probe = system.join(format!(".getcraft-write-test-{}", std::process::id()));
    if fs::write(&probe, b"").is_ok() {
        let _ = fs::remove_file(&probe);
        system
    } else {
        dirs::home_dir().unwrap_or_default().join("Applications")
    }
}

/// Detaches a mounted disk image when dropped, even if installation fails.
struct Mounted(PathBuf);

impl Drop for Mounted {
    fn drop(&mut self) {
        let ok = Command::new("hdiutil").arg("detach").arg(&self.0).arg("-quiet").status().is_ok_and(|s| s.success());
        if !ok {
            let _ = Command::new("hdiutil").arg("detach").arg(&self.0).args(["-force", "-quiet"]).status();
        }
        let _ = fs::remove_dir(&self.0);
    }
}

pub fn install_dmg(dmg: &Path, apps_dir: &Path, work_dir: &Path) -> Result<PathBuf> {
    let mount_point = work_dir.join(format!("mount-{}", std::process::id()));
    let _ = Command::new("hdiutil").arg("detach").arg(&mount_point).args(["-force", "-quiet"]).status();
    fs::create_dir_all(&mount_point)?;
    let out = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(&mount_point)
        .arg(dmg)
        .output()?;
    if !out.status.success() {
        return Err(Error::Install(format!(
            "could not open the disk image: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let mounted = Mounted(mount_point);

    let app = fs::read_dir(&mounted.0)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| Error::Install("the disk image doesn't contain an app".into()))?;
    let dest = apps_dir.join(app.file_name().unwrap());
    let staged = apps_dir.join(format!(".{}.getcraft-new", app.file_name().unwrap().to_string_lossy()));
    remove_path(&staged)?;

    // ditto preserves code signatures, extended attributes and symlinks inside the bundle.
    let out = Command::new("ditto").arg(&app).arg(&staged).output()?;
    if !out.status.success() {
        let _ = remove_path(&staged);
        return Err(Error::Install(format!("copying the app failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    drop(mounted);
    swap_into_place(&staged, &dest)?;
    Ok(dest)
}

pub fn uninstall(app: &Path) -> Result<()> {
    if app.extension().is_none_or(|e| e != "app") {
        return Err(Error::Install(format!("refusing to delete {}: not an app bundle", app.display())));
    }
    // Move to the Trash rather than deleting, so an accidental uninstall is recoverable.
    if let Err(e) = trash::delete(app) {
        log::warn!("could not move {} to the Trash ({e}), deleting it", app.display());
        remove_path(app)?;
    }
    Ok(())
}

pub fn bundle_version(app: &Path) -> Option<String> {
    let info = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    let dict = info.as_dictionary()?;
    dict.get("CFBundleShortVersionString")
        .or_else(|| dict.get("CFBundleVersion"))
        .and_then(|v| v.as_string())
        .map(str::to_owned)
}

pub fn find_existing(tool: &Tool, apps_dir: &Path, scan_system_dirs: bool) -> Option<(PathBuf, String)> {
    let mut candidates = vec![apps_dir.to_path_buf()];
    if scan_system_dirs {
        candidates.push(PathBuf::from("/Applications"));
        candidates.extend(dirs::home_dir().map(|h| h.join("Applications")));
    }
    candidates
        .into_iter()
        .map(|dir| dir.join(format!("{}.app", tool.name)))
        .find_map(|app| bundle_version(&app).map(|v| (app, v)))
}
