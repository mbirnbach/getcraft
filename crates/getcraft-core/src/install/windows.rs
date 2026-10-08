use super::{remove_path, swap_into_place};
use crate::catalog::Tool;
use crate::{Error, Result};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub fn install_zip(zip_path: &Path, apps_dir: &Path, tool: &Tool) -> Result<PathBuf> {
    let dest = apps_dir.join(&tool.id);
    let staged = apps_dir.join(format!(".{}.getcraft-new", tool.id));
    remove_path(&staged)?;
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?)
        .map_err(|e| Error::Install(format!("the download isn't a valid zip: {e}")))?;
    archive.extract(&staged).map_err(|e| Error::Install(format!("unpacking failed: {e}")))?;

    // Portable zips often wrap everything in one top-level folder; install its contents.
    let entries: Vec<_> = fs::read_dir(&staged)?.filter_map(|e| e.ok()).collect();
    let root = match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => staged.clone(),
    };
    swap_into_place(&root, &dest)?;
    remove_path(&staged)?;

    let exe =
        find_exe(&dest, tool).ok_or_else(|| Error::Install("couldn't find the program inside the download".into()))?;
    if let Err(e) = create_shortcut(tool, &exe) {
        log::warn!("could not create Start Menu shortcut for {}: {e}", tool.name);
    }
    Ok(exe)
}

fn find_exe(dir: &Path, tool: &Tool) -> Option<PathBuf> {
    let exes: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .collect();
    let stem = |p: &PathBuf| p.file_stem().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    exes.iter()
        .find(|p| stem(p) == tool.id || stem(p) == tool.name.to_ascii_lowercase())
        .or_else(|| exes.iter().find(|p| !stem(p).contains("uninstall") && !stem(p).ends_with("-cli")))
        .cloned()
}

fn shortcut_path(tool: &Tool) -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(r"Microsoft\Windows\Start Menu\Programs\GetCraft").join(format!("{}.lnk", tool.name)))
}

fn create_shortcut(tool: &Tool, exe: &Path) -> Result<()> {
    let lnk = shortcut_path(tool).ok_or_else(|| Error::Install("no Start Menu folder".into()))?;
    fs::create_dir_all(lnk.parent().unwrap())?;
    let mut link = mslnk::ShellLink::new(exe).map_err(|e| Error::Install(format!("{e:?}")))?;
    link.set_working_dir(exe.parent().map(|p| p.display().to_string()));
    link.create_lnk(&lnk).map_err(|e| Error::Install(format!("{e:?}")))
}

pub fn uninstall(tool: &Tool, exe: &Path) -> Result<()> {
    let dir = exe.parent().ok_or_else(|| Error::Install("bad install path".into()))?;
    // Only ever delete folders GetCraft created.
    if dir.file_name().is_none_or(|n| n.to_string_lossy() != tool.id) {
        return Err(Error::Install(format!("refusing to delete {}", dir.display())));
    }
    remove_path(dir)?;
    if let Some(lnk) = shortcut_path(tool) {
        let _ = fs::remove_file(lnk);
    }
    Ok(())
}
