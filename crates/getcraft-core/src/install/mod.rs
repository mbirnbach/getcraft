//! Installs, removes, finds and launches tools. Every installer works without admin rights and
//! swaps the new version into place only once it's fully unpacked. The one exception is a
//! Windows copy installed with the app's own `.msi`: that one is updated and removed by Windows
//! Installer, which asks the user for admin rights.

// Used by the Windows installer and self-updater; tested on every platform.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) mod archive;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(windows)]
mod windows;

use crate::assets::PackageKind;
use crate::catalog::Tool;
use crate::state::{InstallRecord, Paths};
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Installer {
    /// Where tools are installed.
    pub apps_dir: PathBuf,
    /// Scratch space: disk image mount points on macOS, installer logs on Windows.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    work_dir: PathBuf,
    /// Whether to look for hand-installed copies outside `apps_dir` (`/Applications` on macOS,
    /// copies installed with the apps' own `.msi` on Windows). Off for custom/test directories
    /// so they never see, or touch, the real system.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    scan_system_dirs: bool,
    /// Where replaced versions are kept: `<previous_dir>/<id>/<bundle or folder>`.
    previous_dir: PathBuf,
}

/// The result of an install.
#[derive(Debug)]
pub struct Installed {
    /// What gets launched.
    pub path: PathBuf,
    /// The replaced copy, if it was kept.
    pub kept: Option<PathBuf>,
}

/// A copy of a tool that was installed without GetCraft.
#[derive(Debug)]
pub struct Existing {
    /// What gets launched.
    pub path: PathBuf,
    pub version: String,
    /// Installed with the app's own Windows Installer package (see [`InstallRecord::msi`]).
    pub msi: bool,
}

impl Installer {
    pub fn new(paths: &Paths) -> Self {
        Self {
            apps_dir: default_apps_dir(),
            work_dir: paths.work_dir.clone(),
            scan_system_dirs: true,
            previous_dir: paths.previous_dir.clone(),
        }
    }

    /// Installs into `apps_dir` only and ignores tools installed anywhere else.
    pub fn with_apps_dir(apps_dir: PathBuf, paths: &Paths) -> Self {
        Self {
            apps_dir,
            work_dir: paths.work_dir.clone(),
            scan_system_dirs: false,
            previous_dir: paths.previous_dir.clone(),
        }
    }

    /// Installs `tool` from a downloaded package. An update (`previous` is the current launch
    /// path) replaces the existing copy where it is, so nothing else on disk is ever moved or
    /// removed; with `keep_previous` the replaced copy is moved to the previous-versions folder
    /// (replacing the one kept before) instead of being deleted. `icon` is a PNG for Linux menu
    /// entries. An [`PackageKind::Msi`] only updates a copy installed with the app's own `.msi`
    /// and never keeps the old one (Windows Installer can't go back to it).
    pub fn install(
        &self,
        tool: &Tool,
        package: &Path,
        kind: PackageKind,
        previous: Option<&Path>,
        keep_previous: bool,
        icon: Option<&[u8]>,
    ) -> Result<Installed> {
        let Some(current) = previous.filter(|_| keep_previous && kind != PackageKind::Msi) else {
            return Ok(Installed { path: self.install_package(tool, package, kind, previous, icon)?, kept: None });
        };
        let root = install_root(current);
        let name = root.file_name().ok_or_else(|| Error::Install("bad install path".into()))?;
        let dir = self.previous_dir.join(&tool.id);
        // The version kept so far is only dropped once the update has worked.
        let replaced = self.previous_dir.join(format!(".{}.replaced", tool.id));
        remove_path(&replaced)?;
        if dir.exists() {
            fs::rename(&dir, &replaced)?;
        }
        let kept = dir.join(name);
        let result = move_path(root, &kept).and_then(|()| self.install_package(tool, package, kind, previous, icon));
        match result {
            Ok(path) => {
                if let Err(e) = remove_path(&replaced) {
                    log::warn!("could not remove the version kept before: {e}");
                }
                Ok(Installed { path, kept: Some(kept) })
            }
            Err(e) => {
                // Put everything back as it was.
                if kept.exists()
                    && let Err(restore) = move_path(&kept, root)
                {
                    log::error!("could not restore {} after a failed update: {restore}", root.display());
                }
                if replaced.exists() {
                    let _ = remove_path(&dir);
                    if let Err(restore) = fs::rename(&replaced, &dir) {
                        log::error!("could not restore the kept version of {}: {restore}", tool.id);
                    }
                }
                Err(e)
            }
        }
    }

    /// Swaps the installed copy at `current` with the kept one at `kept`, so the kept version is
    /// installed again and the current one is kept in its place. Returns the new launch path and
    /// the new kept path.
    pub fn switch_to_kept(&self, tool: &Tool, current: &Path, kept: &Path) -> Result<(PathBuf, PathBuf)> {
        let dir = self.previous_dir.join(&tool.id);
        let kept_name = kept.file_name().filter(|_| kept.parent() == Some(dir.as_path()) && kept.exists());
        let kept_name = kept_name.ok_or_else(|| Error::Install(format!("the kept {} is missing", tool.name)))?;
        self.check_kept(tool, kept)?;

        let root = install_root(current);
        let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
            return Err(Error::Install("bad install path".into()));
        };
        // The launch path inside the folder (Windows, Linux), or the bundle itself (macOS).
        let inside = current.strip_prefix(root).unwrap_or(Path::new("")).to_path_buf();
        let restored = parent.join(kept_name);
        if restored != root && restored.exists() {
            return Err(Error::Install(format!("{} is in the way", restored.display())));
        }

        let parked = dir.join(".switching");
        remove_path(&parked)?;
        move_path(root, &parked)?;
        if let Err(e) = move_path(kept, &restored) {
            if let Err(undo) = move_path(&parked, root) {
                log::error!("could not put {} back: {undo}", root.display());
            }
            return Err(e);
        }
        let now_kept = dir.join(name);
        fs::rename(&parked, &now_kept)?;
        let launch = if inside.as_os_str().is_empty() { restored } else { restored.join(inside) };
        Ok((launch, now_kept))
    }

    /// Checks a kept copy before it's installed again: it sits in a folder the user can write
    /// to, so it gets the same checks as a fresh download where the platform allows.
    #[allow(unused_variables)]
    fn check_kept(&self, tool: &Tool, kept: &Path) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let identity = crate::trust::mac_identity(&tool.id, &tool.repo)
                .ok_or_else(|| Error::Install(format!("{} isn't from a trusted publisher", tool.name)))?;
            macos::verify_signature(kept, &identity)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let program = if cfg!(windows) { format!("{}.exe", tool.id) } else { format!("{}.AppImage", tool.id) };
            match kept.join(&program).is_file() {
                true => Ok(()),
                false => Err(Error::Install(format!("the kept {} is incomplete", tool.name))),
            }
        }
    }

    /// Deletes the kept version of `tool`, if there is one.
    pub fn remove_kept(&self, tool_id: &str) -> Result<()> {
        if !crate::trust::valid_id(tool_id) {
            return Err(Error::Install(format!("invalid tool id {tool_id:?}")));
        }
        remove_path(&self.previous_dir.join(tool_id)).map_err(Into::into)
    }

    #[allow(unused_variables)]
    fn install_package(
        &self,
        tool: &Tool,
        package: &Path,
        kind: PackageKind,
        previous: Option<&Path>,
        icon: Option<&[u8]>,
    ) -> Result<PathBuf> {
        fs::create_dir_all(&self.apps_dir)?;
        match kind {
            #[cfg(target_os = "macos")]
            PackageKind::Dmg => {
                let identity = crate::trust::mac_identity(&tool.id, &tool.repo)
                    .ok_or_else(|| Error::Install(format!("{} isn't from a trusted publisher", tool.name)))?;
                let dir = previous.filter(|p| p.extension().is_some_and(|e| e == "app")).and_then(Path::parent);
                macos::install_dmg(package, dir.unwrap_or(&self.apps_dir), &self.work_dir, &identity)
            }
            #[cfg(windows)]
            PackageKind::PortableZip => windows::install_zip(package, &self.apps_dir, tool),
            #[cfg(windows)]
            PackageKind::Msi => match previous {
                Some(previous) if self.scan_system_dirs => {
                    windows::install_msi(package, tool, previous, &self.work_dir)
                }
                _ => Err(Error::Install("the Windows installer package only updates an installed copy".into())),
            },
            #[cfg(target_os = "linux")]
            PackageKind::AppImage => linux::install_appimage(package, &self.apps_dir, tool, icon),
            #[allow(unreachable_patterns)]
            other => Err(Error::Install(format!("{other:?} packages can't be installed on this system"))),
        }
    }

    #[allow(unused_variables)]
    pub fn uninstall(&self, tool: &Tool, record: &InstallRecord) -> Result<()> {
        let path = record.path.as_path();
        #[cfg(target_os = "macos")]
        return match crate::trust::mac_identity(&tool.id, &tool.repo) {
            Some(identity) => macos::uninstall(path, &identity.bundle_id),
            None => Err(Error::Install(format!("{} isn't from a trusted publisher", tool.name))),
        };
        #[cfg(windows)]
        return match record.msi {
            true if self.scan_system_dirs => windows::uninstall_msi(tool, path, &self.work_dir),
            true => Err(Error::Install("not removing a copy outside the apps folder".into())),
            false => windows::uninstall(tool, path),
        };
        #[cfg(target_os = "linux")]
        return linux::uninstall(tool, path);
        #[allow(unreachable_code)]
        Err(Error::Install("unsupported platform".into()))
    }

    /// The version actually installed, where the platform records one (macOS bundles and
    /// Windows Installer packages do).
    #[allow(unused_variables)]
    pub fn installed_version(&self, tool: &Tool, record: &InstallRecord) -> Option<String> {
        #[cfg(target_os = "macos")]
        return macos::bundle_version(&record.path);
        #[cfg(windows)]
        return record.msi.then(|| windows::msi_at(tool, &record.path)).flatten().map(|m| m.version);
        #[allow(unreachable_code)]
        None
    }

    /// Finds a copy of `tool` that was installed without GetCraft.
    #[allow(unused_variables)]
    pub fn find_existing(&self, tool: &Tool) -> Option<Existing> {
        #[cfg(target_os = "macos")]
        return macos::find_existing(tool, &self.apps_dir, self.scan_system_dirs).map(|(path, version)| Existing {
            path,
            version,
            msi: false,
        });
        #[cfg(windows)]
        return self.scan_system_dirs.then(|| windows::find_msi(tool)).flatten().map(|m| Existing {
            path: m.exe,
            version: m.version,
            msi: true,
        });
        #[allow(unreachable_code)]
        None
    }
}

fn default_apps_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    return macos::default_apps_dir();
    #[cfg(windows)]
    return dirs::data_local_dir().unwrap_or_default().join("Programs").join("GetCraft");
    #[allow(unreachable_code)]
    dirs::data_local_dir().unwrap_or_default().join("getcraft").join("apps")
}

pub fn launch(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut cmd = {
        let mut c = std::process::Command::new(path);
        if let Some(dir) = path.parent() {
            c.current_dir(dir);
        }
        c
    };
    cmd.spawn().map(drop).map_err(|e| Error::Install(format!("could not start {}: {e}", path.display())))
}

/// The directory (or bundle) a tool lives in, which is what we check for running processes.
pub fn install_root(path: &Path) -> &Path {
    if path.extension().is_some_and(|e| e == "app") { path } else { path.parent().unwrap_or(path) }
}

/// Whether any process is running from the tool's install location. Updates and uninstalls
/// wait until it's closed, on every platform.
pub fn is_running(path: &Path) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    // Compare resolved paths so symlinks or `..` on either side can't hide a running copy.
    let root = install_root(path);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes().values().any(|p| {
        p.exe().is_some_and(|exe| exe.starts_with(&root) || exe.canonicalize().is_ok_and(|exe| exe.starts_with(&root)))
    })
}

/// Replaces `dest` with `staged` as atomically as the filesystem allows, restoring the old
/// version if the swap fails halfway.
#[allow(dead_code)]
pub(crate) fn swap_into_place(staged: &Path, dest: &Path) -> Result<()> {
    if !dest.exists() {
        fs::rename(staged, dest)?;
        return Ok(());
    }
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    let old = dest.with_file_name(format!(".{name}.getcraft-old"));
    remove_path(&old)?;
    fs::rename(dest, &old)?;
    if let Err(e) = fs::rename(staged, dest) {
        let _ = fs::rename(&old, dest);
        return Err(e.into());
    }
    if let Err(e) = remove_path(&old) {
        log::warn!("could not remove previous version at {}: {e}", old.display());
    }
    Ok(())
}

/// Moves `from` to `to`, copying across disks when a rename can't (e.g. `/Applications` on
/// another volume than the user's data).
pub(crate) fn move_path(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            if let Err(e) = copy_tree(from, to) {
                let _ = remove_path(to);
                return Err(e);
            }
            remove_path(from).map_err(Into::into)
        }
        Err(e) => Err(e.into()),
    }
}

/// Copies a file or folder. On macOS `ditto` keeps code signatures and extended attributes.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ditto").arg(from).arg(to).output()?;
        if !out.status.success() {
            return Err(Error::Install(format!("copying failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let meta = fs::symlink_metadata(from)?;
        if meta.is_dir() {
            fs::create_dir_all(to)?;
            for entry in fs::read_dir(from)? {
                let entry = entry?;
                copy_tree(&entry.path(), &to.join(entry.file_name()))?;
            }
        } else if meta.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(from)?, to)?;
        } else {
            fs::copy(from, to)?;
        }
        Ok(())
    }
}

pub(crate) fn remove_path(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_replaces_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("Tool.app");
        let staged = dir.path().join(".Tool.app.new");
        fs::create_dir_all(dest.join("old")).unwrap();
        fs::create_dir_all(staged.join("new")).unwrap();
        swap_into_place(&staged, &dest).unwrap();
        assert!(dest.join("new").exists());
        assert!(!dest.join("old").exists());
        assert!(!staged.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no leftovers");
    }
}
