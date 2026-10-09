use super::{remove_path, swap_into_place};
use crate::catalog::Tool;
use crate::trust::MacIdentity;
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

/// Installs the single app in `dmg` into `apps_dir`, but only if it's signed by `identity`.
pub fn install_dmg(dmg: &Path, apps_dir: &Path, work_dir: &Path, identity: &MacIdentity) -> Result<PathBuf> {
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

    let apps: Vec<PathBuf> = fs::read_dir(&mounted.0)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "app") && !p.is_symlink())
        .collect();
    let [app] = apps.as_slice() else {
        return Err(Error::Install("the disk image should contain exactly one app".into()));
    };
    let file_name = app.file_name().unwrap().to_owned();
    let dest = apps_dir.join(&file_name);
    // Never replace an unrelated app that happens to have the same name.
    if dest.exists() && bundle_id(&dest).as_deref() != Some(identity.bundle_id.as_str()) {
        return Err(Error::Install(format!(
            "{} already exists and is a different app; not replacing it",
            dest.display()
        )));
    }
    let staged = apps_dir.join(format!(".{}.getcraft-new", file_name.to_string_lossy()));
    remove_path(&staged)?;

    // ditto preserves code signatures, extended attributes and symlinks inside the bundle.
    let out = Command::new("ditto").arg(app).arg(&staged).output()?;
    drop(mounted);
    if !out.status.success() {
        let _ = remove_path(&staged);
        return Err(Error::Install(format!("copying the app failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    // Check the copy we're about to install, not the disk image, so nothing can change in between.
    if let Err(e) = verify_signature(&staged, identity) {
        let _ = remove_path(&staged);
        return Err(e);
    }
    swap_into_place(&staged, &dest)?;
    Ok(dest)
}

/// Checks that `app` is intact and signed with an Apple-issued certificate of the expected team
/// for the expected bundle identifier.
pub fn verify_signature(app: &Path, identity: &MacIdentity) -> Result<()> {
    // Quotes would end the requirement string early; ids and team IDs never contain them.
    if [&identity.bundle_id, &identity.team].iter().any(|s| s.contains(['"', '\\'])) {
        return Err(Error::Install("invalid signing identity".into()));
    }
    let requirement = format!(
        "identifier \"{}\" and anchor apple generic and certificate leaf[subject.OU] = \"{}\"",
        identity.bundle_id, identity.team
    );
    // No --strict: some upstream bundles carry harmless Finder metadata that strict mode
    // rejects, while the signature itself is valid and notarized.
    let out = Command::new("codesign")
        .args(["--verify", "--deep", "--verbose=1"])
        .arg(format!("-R={requirement}"))
        .arg(app)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        log::warn!("signature check failed for {}: {}", app.display(), String::from_utf8_lossy(&out.stderr).trim());
        Err(Error::Install(format!(
            "the app isn't signed by its publisher (expected {} from team {}); not installing it",
            identity.bundle_id, identity.team
        )))
    }
}

/// Moves `app` to the Trash, but only if it's the app we expect (by bundle identifier).
pub fn uninstall(app: &Path, expected_bundle_id: &str) -> Result<()> {
    if app.extension().is_none_or(|e| e != "app") {
        return Err(Error::Install(format!("refusing to delete {}: not an app bundle", app.display())));
    }
    if bundle_id(app).as_deref() != Some(expected_bundle_id) {
        return Err(Error::Install(format!("refusing to delete {}: it isn't {expected_bundle_id}", app.display())));
    }
    // Move to the Trash rather than deleting, so an accidental uninstall is recoverable.
    if let Err(e) = trash::delete(app) {
        log::warn!("could not move {} to the Trash ({e}), deleting it", app.display());
        remove_path(app)?;
    }
    Ok(())
}

fn info_plist(app: &Path) -> Option<plist::Dictionary> {
    plist::Value::from_file(app.join("Contents/Info.plist")).ok()?.into_dictionary()
}

pub fn bundle_id(app: &Path) -> Option<String> {
    info_plist(app)?.get("CFBundleIdentifier")?.as_string().map(str::to_owned)
}

pub fn bundle_version(app: &Path) -> Option<String> {
    let dict = info_plist(app)?;
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
    // Only adopt the real thing: an app with the tool's name but another identity is left alone.
    let expected = crate::trust::mac_identity(&tool.id, &tool.repo)?.bundle_id;
    candidates
        .into_iter()
        .map(|dir| dir.join(format!("{}.app", tool.name)))
        .filter(|app| bundle_id(app).as_deref() == Some(expected.as_str()))
        .find_map(|app| bundle_version(&app).map(|v| (app, v)))
}
