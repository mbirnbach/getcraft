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

    let icon =
        image::load_from_memory(include_bytes!("../../../assets/getcraft-256.png")).expect("bundled icon").into_rgba8();
    let icon = egui::IconData { width: icon.width(), height: icon.height(), rgba: icon.into_raw() };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GetCraft")
            .with_icon(icon)
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
