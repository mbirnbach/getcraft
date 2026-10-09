use super::{remove_path, swap_into_place};
use crate::catalog::Tool;
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use windows_registry::{Key, LOCAL_MACHINE};

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

// ------------------------------------------------------------------------------------------------
// Copies installed with the apps' own `.msi`
//
// Every Crafting App's Windows Installer package installs for all users into
// `Program Files\<Name>` and registers, under HKLM (which only admins can change):
// - `App Paths\<id>.exe`, whose default value is the full path of the program;
// - an uninstall entry (key = product code) with `DisplayName` = the app's name, `Publisher` =
//   the publisher's manufacturer name and `DisplayVersion` (numeric, pre-release tags dropped).
// The packages share one upgrade code across versions and architectures, so running a newer
// `.msi` replaces the installed one. That needs admin rights, so GetCraft only runs it when
// the user asks, and Windows shows its permission prompt. (WordCraft 0.1.0 had another upgrade
// code, so an update can leave its entry next to the new one; we report the newest version and
// uninstall all of them.)

const APP_PATHS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths";
const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
/// Registry views: the 64-bit one (x64 and ARM64 packages) and the 32-bit one (x86 packages).
const KEY_WOW64_64KEY: u32 = 0x0100;
const KEY_WOW64_32KEY: u32 = 0x0200;

/// A Crafting App installed with its own Windows Installer package.
#[derive(Debug)]
pub struct MsiInstall {
    pub exe: PathBuf,
    /// The newest installed version.
    pub version: String,
    /// Usually one; all of them are removed on uninstall.
    product_codes: Vec<String>,
    view: u32,
}

/// Windows Installer runs one installation at a time; queue ours instead of failing.
static MSIEXEC: Mutex<()> = Mutex::new(());

fn open_key(view: u32, path: &str) -> Option<Key> {
    LOCAL_MACHINE.options().read().access(view).open(path).ok()
}

pub fn find_msi(tool: &Tool) -> Option<MsiInstall> {
    let manufacturer = crate::trust::publisher_for(&tool.repo)?.windows_manufacturer;
    [KEY_WOW64_64KEY, KEY_WOW64_32KEY].into_iter().find_map(|view| {
        let exe = open_key(view, &format!(r"{APP_PATHS}\{}.exe", tool.id))?.get_string("").ok()?;
        let exe = PathBuf::from(exe.trim().trim_matches('"'));
        if !is_program_of(&exe, tool) || !exe.is_file() {
            return None;
        }
        let uninstall = open_key(view, UNINSTALL)?;
        let mut entries: Vec<(String, String)> = uninstall
            .keys()
            .ok()?
            .filter_map(|code| {
                let entry = open_key(view, &format!(r"{UNINSTALL}\{code}"))?;
                let text = |name: &str| entry.get_string(name).ok().map(|v| v.trim().to_owned());
                let matches = is_product_code(&code)
                    && entry.get_u32("WindowsInstaller").ok() == Some(1)
                    && text("Publisher").as_deref() == Some(manufacturer)
                    && text("DisplayName").is_some_and(|n| n.eq_ignore_ascii_case(&tool.name));
                matches.then(|| text("DisplayVersion")).flatten().filter(|v| !v.is_empty()).map(|v| (code, v))
            })
            .collect();
        // Newest first (unreadable versions last).
        entries.sort_by_key(|(_, v)| std::cmp::Reverse(crate::version::parse_tag(v)));
        let version = entries.first()?.1.clone();
        Some(MsiInstall { exe, version, product_codes: entries.into_iter().map(|(code, _)| code).collect(), view })
    })
}

/// `<id>.exe` at an absolute path; the same rule as for GetCraft's own copies.
fn is_program_of(exe: &Path, tool: &Tool) -> bool {
    exe.is_absolute()
        && exe.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&format!("{}.exe", tool.id)))
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`; anything else never reaches `msiexec`.
fn is_product_code(code: &str) -> bool {
    let b = code.as_bytes();
    b.len() == 38
        && b[0] == b'{'
        && b[37] == b'}'
        && b[1..37]
            .iter()
            .enumerate()
            .all(|(i, c)| if [8, 13, 18, 23].contains(&i) { *c == b'-' } else { c.is_ascii_hexdigit() })
}

/// The MSI copy of `tool` at `exe`, if that's what `exe` is.
pub fn msi_at(tool: &Tool, exe: &Path) -> Option<MsiInstall> {
    find_msi(tool).filter(|m| same_path(&m.exe, exe))
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.as_os_str().eq_ignore_ascii_case(b.as_os_str()),
    }
}

/// Updates the MSI copy at `previous` by running the newer package, and returns the program's
/// path afterwards. Windows asks the user for permission.
pub fn install_msi(package: &Path, tool: &Tool, previous: &Path, log_dir: &Path) -> Result<PathBuf> {
    let installed = msi_at(tool, previous)
        .ok_or_else(|| Error::Install(format!("{} is no longer installed with its Windows installer", tool.name)))?;
    // Keep the desktop shortcut as the user chose it at first install (the package records it).
    let desktop = open_key(installed.view, &format!(r"SOFTWARE\{}\Shortcuts", tool.name))
        .is_some_and(|k| k.get_u32("Desktop").is_ok());
    let mut cmd = msiexec();
    cmd.arg("/i")
        .arg(package)
        .args(["/passive", "/norestart"])
        .arg(format!("INSTALLDESKTOPSHORTCUT={}", u8::from(desktop)));
    run_msiexec(cmd, tool, "update", log_dir)?;
    find_msi(tool).map(|m| m.exe).ok_or_else(|| Error::Install(format!("{} wasn't found after the update", tool.name)))
}

/// Removes the MSI copy at `exe` with its own uninstaller. Windows asks the user for permission.
pub fn uninstall_msi(tool: &Tool, exe: &Path, log_dir: &Path) -> Result<()> {
    let installed = msi_at(tool, exe)
        .ok_or_else(|| Error::Install(format!("{} is no longer installed with its Windows installer", tool.name)))?;
    for code in &installed.product_codes {
        let mut cmd = msiexec();
        cmd.arg("/x").arg(code).args(["/passive", "/norestart"]);
        run_msiexec(cmd, tool, "uninstall", log_dir)?;
    }
    Ok(())
}

/// `msiexec` from the system folder, never from the search path.
fn msiexec() -> Command {
    let system_root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    Command::new(system_root.join("System32").join("msiexec.exe"))
}

fn run_msiexec(mut cmd: Command, tool: &Tool, what: &str, log_dir: &Path) -> Result<()> {
    fs::create_dir_all(log_dir)?;
    let log = log_dir.join(format!("{}-{what}-msi.log", tool.id));
    cmd.arg("/l*").arg(&log);
    let _queue = MSIEXEC.lock().unwrap_or_else(|e| e.into_inner());
    log::info!("running {cmd:?}");
    let code = cmd.status()?.code();
    log::info!("msiexec for {} exited with {code:?}", tool.name);
    match code {
        // 3010: done, a restart is needed to finish; 1641: Windows is restarting.
        Some(0 | 3010 | 1641) => Ok(()),
        Some(1602 | 1223) => Err(Error::Install(format!("The {what} of {} was cancelled", tool.name))),
        Some(1618) => Err(Error::Install("Another installation is in progress; try again when it's finished".into())),
        Some(1625) => Err(Error::Install(format!("Windows didn't allow the {what} of {} (error 1625)", tool.name))),
        other => Err(Error::Install(format!(
            "The Windows installer of {} failed (error {}); details in {}",
            tool.name,
            other.map_or("unknown".into(), |c| c.to_string()),
            log.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_codes_are_plain_guids() {
        assert!(is_product_code("{EA00C367-7270-4C50-9D8B-A7A5C88AD27F}"));
        assert!(is_product_code("{ea00c367-7270-4c50-9d8b-a7a5c88ad27f}"));
        for bad in [
            "",
            "EA00C367-7270-4C50-9D8B-A7A5C88AD27F",
            "{EA00C367-7270-4C50-9D8B-A7A5C88AD27F} /q",
            "{EA00C367 7270-4C50-9D8B-A7A5C88AD27F}",
            "{GA00C367-7270-4C50-9D8B-A7A5C88AD27F}",
        ] {
            assert!(!is_product_code(bad), "{bad:?}");
        }
    }
}
