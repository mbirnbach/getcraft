//! Native desktop notifications for things worth interrupting the user for.

use getcraft_core::engine::Event;

/// Must run once before the first notification. On macOS, notifications are attributed to an
/// app bundle; without this the library asks LaunchServices for a placeholder app and the user
/// gets a "Where is use_default?" dialog.
pub fn init() {
    #[cfg(target_os = "macos")]
    {
        let bundle_id = objc2_foundation::NSBundle::mainBundle().bundleIdentifier().map(|id| id.to_string());
        // Unbundled development builds borrow Finder's identity so notifications still show.
        let bundle_id = bundle_id.unwrap_or_else(|| "com.apple.Finder".to_owned());
        if let Err(e) = notify_rust::set_application(&bundle_id) {
            log::warn!("notifications may not work: {e}");
        }
    }
}

pub fn desktop(event: &Event) {
    let (title, body) = match event {
        Event::UpdateAvailable { tool, version } => {
            (format!("{tool} {version} is available"), "Open GetCraft to update.".to_owned())
        }
        Event::Installed { tool, version, updated: true } => {
            (format!("{tool} was updated"), format!("You're now on version {version}."))
        }
        // Automatic updates happen while nobody's looking, so failures need telling.
        Event::Failed { tool, error, automatic: true } => {
            (format!("Couldn't update {tool}"), format!("{error}. Open GetCraft for details."))
        }
        Event::LauncherFailed { version, error } => {
            (format!("GetCraft couldn't update itself to {version}"), format!("{error}. It will try again later."))
        }
        // Fresh installs and failures the user started are shown in the window, where they acted.
        _ => return,
    };
    message(&title, &body);
}

pub fn message(title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    // Showing a notification can block (e.g. on first-run permission prompts); keep it off the
    // engine and UI threads.
    std::thread::spawn(move || {
        let mut notification = notify_rust::Notification::new();
        notification.appname("GetCraft").summary(&title).body(&body);
        #[cfg(windows)]
        if let Some(app_id) = windows_app_id() {
            notification.app_id(app_id);
        }
        if let Err(e) = notification.show() {
            log::warn!("could not show notification: {e}");
        }
    });
}

/// Windows shows a notification under the app whose ID it carries, but only knows that ID from
/// a Start menu shortcut. The installer creates one with GetCraft's ID; portable copies have none
/// and keep notify-rust's default (which Windows labels "Windows PowerShell").
#[cfg(windows)]
fn windows_app_id() -> Option<&'static str> {
    let appdata = std::path::PathBuf::from(std::env::var_os("APPDATA")?);
    let shortcut = appdata.join(r"Microsoft\Windows\Start Menu\Programs\GetCraft.lnk");
    shortcut.exists().then_some("net.brnbch.getcraft")
}
