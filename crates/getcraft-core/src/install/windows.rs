use super::{remove_path, swap_into_place};
use crate::catalog::Tool;
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn install_zip(zip_path: &Path, apps_dir: &Path, tool: &Tool) -> Result<PathBuf> {
    let dest = apps_dir.join(&tool.id);
    let staged = apps_dir.join(format!(".{}.getcraft-new", tool.id));
    remove_path(&staged)?;
    if let Err(e) = super::archive::extract_zip(zip_path, &staged, super::archive::MAX_TOTAL_BYTES) {
        let _ = remove_path(&staged);
        return Err(e);
    }

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

/// The program must be `<id>.exe` at the top of the package, as in every Crafting App. Guessing
/// among other executables could start the wrong (or a planted) program.
fn find_exe(dir: &Path, tool: &Tool) -> Option<PathBuf> {
    let exe = dir.join(format!("{}.exe", tool.id));
    exe.is_file().then_some(exe)
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
