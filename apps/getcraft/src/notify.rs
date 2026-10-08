//! Native desktop notifications for things worth interrupting the user for.

use getcraft_core::engine::Event;

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
    // Showing a notification can block (e.g. on first-run permission prompts); keep it off the
    // engine and UI threads.
    std::thread::spawn(move || {
        if let Err(e) = notify_rust::Notification::new().appname("GetCraft").summary(&title).body(&body).show() {
            log::warn!("could not show notification: {e}");
        }
    });
}
