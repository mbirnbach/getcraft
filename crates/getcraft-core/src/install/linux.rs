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
    // The engine only updates an app that isn't running (see `install::is_running`), so this
    // rename never pulls the file out from under a running copy.
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
        "[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec={exec} %F\nIcon={icon}\n\
         Terminal=false\nCategories=Graphics;Office;AudioVideo;\nX-GetCraft-Id={id}\n",
        name = desktop_value(&tool.name),
        comment = desktop_value(&tool.kind),
        exec = desktop_value(&exec_arg(&exe.to_string_lossy())),
        icon = desktop_value(&icon),
        id = desktop_value(&tool.id),
    );
    fs::write(path, entry)?;
    Ok(())
}

/// A string value per the Desktop Entry spec: no line breaks or other control characters (which
/// could start a new key), and backslashes escaped.
fn desktop_value(text: &str) -> String {
    crate::trust::clean_text(text).replace('\\', "\\\\")
}

/// One quoted argument for the `Exec` key: inside double quotes, `"`, `` ` ``, `$` and `\` are
/// backslash-escaped, and `%` (field codes) is doubled.
fn exec_arg(arg: &str) -> String {
    let mut quoted = String::from('"');
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_values_cannot_add_keys() {
        assert_eq!(desktop_value("Evil\nExec=rm -rf ~"), "Evil Exec=rm -rf ~");
        assert_eq!(desktop_value("a\\b"), "a\\\\b");
    }

    #[test]
    fn exec_paths_are_quoted() {
        assert_eq!(exec_arg("/home/a b/x.AppImage"), "\"/home/a b/x.AppImage\"");
        assert_eq!(exec_arg("/h/$x\"`%"), "\"/h/\\$x\\\"\\`%%\"");
    }
}
