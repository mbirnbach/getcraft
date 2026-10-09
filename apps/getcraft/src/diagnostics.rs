//! Making failures visible. Release builds have no console (on Windows there isn't even a
//! terminal), so everything is logged to a file and fatal problems are shown in a dialog instead
//! of GetCraft silently quitting.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The log of the current run; the previous run's log is kept next to it as `getcraft.old.log`.
pub fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("getcraft.log")
}

/// Writes every log line to stderr and to the log file.
struct Tee {
    file: Option<Mutex<File>>,
}

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(file) = &self.file {
            let _ = file.lock().unwrap_or_else(|p| p.into_inner()).write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = &self.file {
            let _ = file.lock().unwrap_or_else(|p| p.into_inner()).flush();
        }
        Ok(())
    }
}

pub fn init_logging(data_dir: &Path) {
    let path = log_path(data_dir);
    let _ = fs::create_dir_all(data_dir);
    let _ = fs::rename(&path, data_dir.join("getcraft.old.log"));
    let file = OpenOptions::new().create(true).write(true).truncate(true).open(&path).ok();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(Tee { file: file.map(Mutex::new) })))
        .init();
    log::info!(
        "GetCraft {} on {} {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
    );
}

/// Logs panics, and shows a dialog when the main thread panics (which ends the app).
pub fn install_panic_hook(data_dir: &Path, dialogs: bool) {
    let log_file = log_path(data_dir);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        log::error!("panic in thread {name}: {info}");
        if dialogs && name == "main" {
            show_error(
                "GetCraft stopped unexpectedly",
                &format!("{info}\n\nDetails were written to:\n{}", log_file.display()),
            );
        }
        default_hook(info);
    }));
}

/// Shows a blocking error dialog. Used only when GetCraft can't continue.
pub fn show_error(title: &str, message: &str) {
    log::error!("{title}: {message}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (title, message) = (wide(title), wide(message));
        // SAFETY: both pointers are valid, NUL-terminated UTF-16 strings that outlive the call.
        unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
    }
    #[cfg(target_os = "macos")]
    {
        let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display alert \"{}\" message \"{}\" as critical", quote(title), quote(message));
        let _ = std::process::Command::new("osascript").args(["-e", &script]).status();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let shown = std::process::Command::new("zenity")
            .args(["--error", "--title", title, "--text", message])
            .status()
            .is_ok_and(|s| s.success());
        if !shown {
            let _ = std::process::Command::new("kdialog").args(["--title", title, "--error", message]).status();
        }
    }
}
