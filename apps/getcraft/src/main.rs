// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod background;
mod icons;
mod instance;
mod notify;
mod theme;

use getcraft_core::state::Paths;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // `--background` is how the login item starts us: no window, just the menu bar / tray icon.
    let hidden = std::env::args().any(|a| a == "--background");
    let paths = Paths::new().expect("no home directory");
    let data_dir = paths.state_file.parent().unwrap().to_path_buf();
    if std::env::args().any(|a| a == "--after-update") {
        instance::wait_for_previous(&data_dir);
    }
    let listener = match instance::acquire(&data_dir) {
        instance::Instance::AlreadyRunning => {
            log::info!("GetCraft is already running; asked it to show its window");
            return Ok(());
        }
        instance::Instance::Primary(listener) => listener,
    };

    notify::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GetCraft")
            .with_icon(window_icon())
            .with_visible(!hidden)
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GetCraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::GetCraftApp::new(cc, paths, listener, hidden)))),
    )
}

/// The icon eframe applies to the window and, on macOS, to the Dock.
fn window_icon() -> egui::IconData {
    // Inside GetCraft.app, leave the Dock to the bundle's .icns, which macOS draws at the same
    // size as every other app. An empty IconData tells eframe not to replace it at runtime.
    if cfg!(target_os = "macos")
        && std::env::current_exe().is_ok_and(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"))
    {
        return egui::IconData::default();
    }
    // Unbundled runs have no .icns. On macOS the image becomes the Dock icon, so it needs Apple's
    // margins; elsewhere it's a small full-bleed title bar / taskbar icon.
    let png: &[u8] = if cfg!(target_os = "macos") {
        include_bytes!("../../../assets/getcraft-macos-256.png")
    } else {
        include_bytes!("../../../assets/getcraft-256.png")
    };
    let png = image::load_from_memory(png).expect("bundled icon").into_rgba8();
    egui::IconData { width: png.width(), height: png.height(), rgba: png.into_raw() }
}
