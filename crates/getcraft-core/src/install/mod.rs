//! Installs, removes, finds and launches tools. Every installer works without admin rights and
//! swaps the new version into place only once it's fully unpacked.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(windows)]
mod windows;

use crate::assets::PackageKind;
use crate::catalog::Tool;
use crate::state::Paths;
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Installer {
    /// Where tools are installed.
    pub apps_dir: PathBuf,
    // Only the macOS installer needs scratch space (for mounting disk images) and scans
    // system-wide folders for hand-installed apps.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    work_dir: PathBuf,
    /// Whether to look for hand-installed copies outside `apps_dir` (e.g. `/Applications`).
    /// Off for custom/test directories so they never see, or touch, the real system.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    scan_system_dirs: bool,
}

impl Installer {
    pub fn new(paths: &Paths) -> Self {
        Self { apps_dir: default_apps_dir(), work_dir: paths.work_dir.clone(), scan_system_dirs: true }
    }

    /// Installs into `apps_dir` only and ignores tools installed anywhere else.
    pub fn with_apps_dir(apps_dir: PathBuf, paths: &Paths) -> Self {
        Self { apps_dir, work_dir: paths.work_dir.clone(), scan_system_dirs: false }
    }

    /// Installs `tool` from a downloaded package and returns its launch path. An update
    /// (`previous` is the current launch path) replaces the existing copy where it is, so
    /// nothing else on disk is ever moved or removed. `icon` is a PNG for Linux menu entries.
    #[allow(unused_variables)]
    pub fn install(
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
                let dir = previous.filter(|p| p.extension().is_some_and(|e| e == "app")).and_then(Path::parent);
                macos::install_dmg(package, dir.unwrap_or(&self.apps_dir), &self.work_dir)
            }
            #[cfg(windows)]
            PackageKind::PortableZip => windows::install_zip(package, &self.apps_dir, tool),
            #[cfg(target_os = "linux")]
            PackageKind::AppImage => linux::install_appimage(package, &self.apps_dir, tool, icon),
            #[allow(unreachable_patterns)]
            other => Err(Error::Install(format!("{other:?} packages can't be installed on this system"))),
        }
    }

    #[allow(unused_variables)]
    pub fn uninstall(&self, tool: &Tool, path: &Path) -> Result<()> {
        #[cfg(target_os = "macos")]
        return macos::uninstall(path);
        #[cfg(windows)]
        return windows::uninstall(tool, path);
        #[cfg(target_os = "linux")]
        return linux::uninstall(tool, path);
        #[allow(unreachable_code)]
        Err(Error::Install("unsupported platform".into()))
    }

    /// The version actually on disk, where the platform records one (macOS bundles do).
    #[allow(unused_variables)]
    pub fn installed_version(&self, path: &Path) -> Option<String> {
        #[cfg(target_os = "macos")]
        return macos::bundle_version(path);
        #[allow(unreachable_code)]
        None
    }

    /// Finds a copy of `tool` that was installed without GetCraft, returning its launch path
    /// and version.
    #[allow(unused_variables)]
    pub fn find_existing(&self, tool: &Tool) -> Option<(PathBuf, String)> {
        #[cfg(target_os = "macos")]
        return macos::find_existing(tool, &self.apps_dir, self.scan_system_dirs);
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

/// Whether any process is running from the tool's install location. Updating a running app
/// would pull files out from under it, so updates wait until it's closed.
pub fn is_running(path: &Path) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let root = install_root(path);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes().values().any(|p| p.exe().is_some_and(|exe| exe.starts_with(root)))
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
