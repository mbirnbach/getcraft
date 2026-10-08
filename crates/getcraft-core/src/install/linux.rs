use super::remove_path;
use crate::catalog::Tool;
use crate::{Error, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn install_appimage(file: &Path, apps_dir: &Path, tool: &Tool, icon: Option<&[u8]>) -> Result<PathBuf> {
    let dir = apps_dir.join(&tool.id);
    fs::create_dir_all(&dir)?;
    let dest = dir.join(format!("{}.AppImage", tool.id));
    let staged = dir.join(format!(".{}.AppImage.getcraft-new", tool.id));
    fs::copy(file, &staged)?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    // Renaming over a running AppImage is fine on Linux: the old inode stays alive until exit.
    fs::rename(&staged, &dest)?;

    let icon_path = dir.join("icon.png");
    if let Some(png) = icon {
        fs::write(&icon_path, png)?;
    }
    if let Err(e) = write_desktop_entry(tool, &dest, icon.is_some().then_some(icon_path.as_path())) {
        log::warn!("could not create menu entry for {}: {e}", tool.name);
    }
    Ok(dest)
}

fn desktop_entry_path(tool: &Tool) -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("applications").join(format!("getcraft-{}.desktop", tool.id)))
}

fn write_desktop_entry(tool: &Tool, exe: &Path, icon: Option<&Path>) -> Result<()> {
    let path = desktop_entry_path(tool).ok_or_else(|| Error::Install("no applications folder".into()))?;
    fs::create_dir_all(path.parent().unwrap())?;
    let icon = icon.map(|p| p.display().to_string()).unwrap_or_else(|| format!("ai.storyteller.{}", tool.id));
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec=\"{exe}\" %F\nIcon={icon}\n\
         Terminal=false\nCategories=Graphics;Office;AudioVideo;\nX-GetCraft-Id={id}\n",
        name = tool.name,
        comment = tool.kind,
        exe = exe.display(),
        id = tool.id,
    );
    fs::write(path, entry)?;
    Ok(())
}

pub fn uninstall(tool: &Tool, exe: &Path) -> Result<()> {
    let dir = exe.parent().ok_or_else(|| Error::Install("bad install path".into()))?;
    if dir.file_name().is_none_or(|n| n.to_string_lossy() != tool.id) {
        return Err(Error::Install(format!("refusing to delete {}", dir.display())));
    }
    remove_path(dir)?;
    if let Some(entry) = desktop_entry_path(tool) {
        let _ = fs::remove_file(entry);
    }
    Ok(())
}
