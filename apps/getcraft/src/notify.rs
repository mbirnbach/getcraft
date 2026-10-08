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
        // Fresh installs and failures are shown inside the window, where the user just acted.
        _ => return,
    };
    message(&title, &body);
}

pub fn message(title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    // Showing a notification can block (e.g. on first-run permission prompts); keep it off the
    // engine and UI threads.
    std::thread::spawn(move || {
        if let Err(e) = notify_rust::Notification::new().appname("GetCraft").summary(&title).body(&body).show() {
            log::warn!("could not show notification: {e}");
        }
    });
}
