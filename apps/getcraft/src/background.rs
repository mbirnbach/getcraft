//! Running in the background: the menu bar / tray icon, launch at login, and (on macOS) Dock
//! integration, so GetCraft can keep checking for updates while its window is closed.

use crate::logo;
use std::sync::Arc;
use std::sync::mpsc::Sender;

/// Requests from outside the window: the tray menu, the Dock, or a second launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    CheckNow,
    Quit,
}

/// Sends a command and wakes the UI so it gets handled even while the window is hidden.
#[derive(Clone)]
pub struct CommandSender {
    tx: Sender<Command>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl CommandSender {
    pub fn new(tx: Sender<Command>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self { tx, wake: Arc::new(wake) }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
        (self.wake)();
    }
}

// ------------------------------------------------------------------------------------------------
// Tray icon

pub struct Tray {
    #[cfg(not(target_os = "linux"))]
    icon: tray_icon::TrayIcon,
    updates: usize,
}

const TRAY_PX: u32 = 36;

fn build_tray(commands: CommandSender) -> Result<tray_icon::TrayIcon, String> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let menu = Menu::new();
    menu.append_items(&[
        &MenuItem::with_id("open", "Open GetCraft", true, None),
        &MenuItem::with_id("check", "Check for Updates", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id("quit", "Quit GetCraft", true, None),
    ])
    .map_err(|e| e.to_string())?;

    let menu_commands = commands.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let cmd = match event.id().0.as_str() {
            "open" => Command::Show,
            "check" => Command::CheckNow,
            "quit" => Command::Quit,
            _ => return,
        };
        menu_commands.send(cmd);
    }));
    // On Windows a left click opens the window and the menu lives on the right button; macOS
    // and Linux show the menu on any click.
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let left_click =
            matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. });
        let open = matches!(event, TrayIconEvent::DoubleClick { .. }) || (cfg!(windows) && left_click);
        if open {
            commands.send(Command::Show);
        }
    }));

    let icon = tray_icon::Icon::from_rgba(logo::tray(TRAY_PX), TRAY_PX, TRAY_PX).map_err(|e| e.to_string())?;
    let builder = TrayIconBuilder::new().with_menu(Box::new(menu));
    // macOS template images are tinted by the system to suit light and dark menu bars.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon);
    builder.with_tooltip("GetCraft").with_menu_on_left_click(!cfg!(windows)).build().map_err(|e| e.to_string())
}

impl Tray {
    /// Must be called on the main thread once the event loop runs (i.e. from the app creator).
    pub fn create(commands: CommandSender) -> Option<Self> {
        #[cfg(not(target_os = "linux"))]
        {
            match build_tray(commands) {
                Ok(icon) => Some(Self { icon, updates: 0 }),
                Err(e) => {
                    log::warn!("no tray icon: {e}");
                    None
                }
            }
        }
        // On Linux the tray needs a running GTK main loop, which winit doesn't provide.
        #[cfg(target_os = "linux")]
        {
            std::thread::spawn(move || {
                if let Err(e) = gtk::init() {
                    log::warn!("no tray icon (GTK unavailable): {e}");
                    return;
                }
                match build_tray(commands) {
                    Ok(_icon) => gtk::main(),
                    Err(e) => log::warn!("no tray icon: {e}"),
                }
            });
            Some(Self { updates: 0 })
        }
    }

    /// Shows the number of pending updates next to (macOS) or in the tooltip of the icon.
    pub fn set_updates(&mut self, count: usize) {
        if count == self.updates {
            return;
        }
        self.updates = count;
        #[cfg(not(target_os = "linux"))]
        {
            let tooltip = match count {
                0 => "GetCraft".to_owned(),
                1 => "GetCraft: 1 update available".to_owned(),
                n => format!("GetCraft: {n} updates available"),
            };
            let _ = self.icon.set_tooltip(Some(tooltip));
            #[cfg(target_os = "macos")]
            self.icon.set_title((count > 0).then(|| count.to_string()));
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Launch at login

/// True when running from a real install rather than `cargo run` / a dev bundle in `target/`.
/// Registering a dev build as a login item would point at a path that disappears.
pub fn is_installed_build() -> bool {
    if std::env::var_os("GETCRAFT_TREAT_AS_INSTALLED").is_some() {
        return true;
    }
    std::env::current_exe().is_ok_and(|exe| !exe.components().any(|c| c.as_os_str() == "target"))
}

fn auto_launch() -> Option<auto_launch::AutoLaunch> {
    // An AppImage runs from a temporary mount; the real file is in $APPIMAGE.
    let exe = std::env::var_os("APPIMAGE").map(Into::into).or_else(|| std::env::current_exe().ok())?;
    let name = if cfg!(target_os = "macos") { "net.brnbch.getcraft" } else { "GetCraft" };
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(name)
        .set_app_path(&exe.to_string_lossy())
        .set_args(&["--background"])
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        // Lets System Settings → Login Items show "GetCraft" for the agent.
        .set_bundle_identifiers(&["net.brnbch.getcraft"])
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser)
        .build()
        .inspect_err(|e| log::warn!("launch at login unavailable: {e}"))
        .ok()
}

/// Makes the login item match the setting. Re-registering on every start also follows the app
/// if the user moved it.
pub fn sync_launch_at_login(enabled: bool) -> Result<(), String> {
    if !is_installed_build() {
        return Ok(());
    }
    let Some(launcher) = auto_launch() else { return Err("not supported on this system".into()) };
    let result = if enabled {
        launcher.enable()
    } else if launcher.is_enabled().unwrap_or(false) {
        launcher.disable()
    } else {
        Ok(())
    };
    result.map_err(|e| e.to_string())
}

// ------------------------------------------------------------------------------------------------
// macOS: Dock icon and "reopen"

#[cfg(target_os = "macos")]
mod macos {
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::{ffi, sel};
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;
    use std::sync::OnceLock;

    static ON_REOPEN: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

    /// `-[NSApplicationDelegate applicationShouldHandleReopen:hasVisibleWindows:]`, sent when the
    /// user clicks the Dock icon or opens the app again from Finder while it's already running.
    extern "C-unwind" fn should_handle_reopen(_this: &AnyObject, _cmd: Sel, _app: &AnyObject, _visible: Bool) -> Bool {
        if let Some(callback) = ON_REOPEN.get() {
            callback();
        }
        Bool::YES
    }

    /// winit owns the application delegate (and aborts if it's replaced), but doesn't implement
    /// the reopen callback. So we add that one method to winit's delegate class at runtime.
    pub fn on_reopen(callback: impl Fn() + Send + Sync + 'static) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        if ON_REOPEN.set(Box::new(callback)).is_err() {
            return;
        }
        let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
            log::warn!("no application delegate; Dock clicks won't reopen the window");
            return;
        };
        let object: &AnyObject = delegate.as_ref();
        let class: &AnyClass = object.class();
        // BOOL is `bool` on Apple silicon and `signed char` on Intel.
        let types = if cfg!(target_arch = "aarch64") { c"B@:@B" } else { c"c@:@c" };
        // SAFETY: the function matches the selector's signature and type encoding above, and
        // class_addMethod leaves the class untouched if the method already exists.
        let added = unsafe {
            let imp: Imp = std::mem::transmute(
                should_handle_reopen as extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, Bool) -> Bool,
            );
            ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                sel!(applicationShouldHandleReopen:hasVisibleWindows:),
                imp,
                types.as_ptr(),
            )
        };
        if !added.as_bool() {
            log::warn!("application delegate already handles reopen; leaving it alone");
        }
    }

    /// A hidden GetCraft lives only in the menu bar; a visible one also has a Dock icon.
    pub fn set_dock_visible(visible: bool) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let policy =
            if visible { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
        app.setActivationPolicy(policy);
        if visible {
            #[allow(deprecated)] // `activate` needs macOS 14; this works everywhere.
            app.activateIgnoringOtherApps(true);
        }
    }
}

pub fn on_reopen(commands: CommandSender) {
    #[cfg(target_os = "macos")]
    macos::on_reopen(move || commands.send(Command::Show));
    #[cfg(not(target_os = "macos"))]
    drop(commands);
}

pub fn set_dock_visible(visible: bool) {
    #[cfg(target_os = "macos")]
    macos::set_dock_visible(visible);
    #[cfg(not(target_os = "macos"))]
    let _ = visible;
}
