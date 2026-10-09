// Release builds on Windows have no console window; see `diagnostics` for where output goes.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod background;
mod diagnostics;
mod icons;
mod instance;
mod notify;
mod theme;

use getcraft_core::state::Paths;
use std::net::TcpListener;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

/// Command-line switches.
#[derive(Clone, Copy, Debug, Default)]
pub struct Flags {
    /// `--background`: how the login item starts us; no window, just the menu bar / tray icon.
    pub hidden: bool,
    /// `--smoke-test`: start everything except network access, draw a few frames and quit.
    /// Release builds run this in CI to prove the app starts on a clean system.
    pub smoke_test: bool,
    /// `--after-update`: started by the self-updater, so announce the new version.
    pub after_update: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let flags =
        Flags { hidden: has("--background"), smoke_test: has("--smoke-test"), after_update: has("--after-update") };

    let Some(paths) = Paths::new() else {
        diagnostics::show_error("GetCraft can't start", "Couldn't find your user folders.");
        return ExitCode::FAILURE;
    };
    let data_dir = paths.state_file.parent().unwrap().to_path_buf();
    diagnostics::init_logging(&data_dir);
    // No dialogs in smoke tests: nobody is there to close them, so CI would hang.
    diagnostics::install_panic_hook(&data_dir, !flags.smoke_test);

    if has("--after-update") {
        instance::wait_for_previous(&data_dir);
    }
    let listener = if flags.smoke_test {
        None
    } else {
        match instance::acquire(&data_dir) {
            instance::Instance::AlreadyRunning => {
                log::info!("GetCraft is already running; asked it to show its window");
                return ExitCode::SUCCESS;
            }
            instance::Instance::Primary(listener) => listener,
        }
    };
    notify::init();

    match run(paths, listener, flags) {
        Ok(()) => {
            if flags.smoke_test {
                log::info!("smoke test passed");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            if !flags.smoke_test {
                diagnostics::show_error(
                    "GetCraft can't start",
                    &format!(
                        "GetCraft couldn't open its window: {e}\n\nDetails were written to:\n{}",
                        diagnostics::log_path(&data_dir).display()
                    ),
                );
            }
            ExitCode::FAILURE
        }
    }
}

/// Opens the window, trying the graphics backends in turn: wgpu first, plain OpenGL ("glow") as
/// the fallback. `GETCRAFT_RENDERER=glow|wgpu` picks one directly.
///
/// A broken graphics driver can crash the process outright instead of returning an error, which
/// no fallback can catch. So a note naming the renderer being started is left in the data folder
/// and removed once the app is up ([`renderer_started`]). If it's still there on the next launch,
/// that renderer crashed, and GetCraft sticks to OpenGL from then on.
fn run(paths: Paths, listener: Option<TcpListener>, flags: Flags) -> Result<(), String> {
    let data_dir = paths.state_file.parent().unwrap().to_path_buf();
    let starting = data_dir.join("renderer-starting");
    let preference = data_dir.join("renderer");
    if let Ok(crashed) = std::fs::read_to_string(&starting) {
        log::warn!("the last start crashed while starting the {crashed} renderer; using OpenGL from now on");
        let _ = std::fs::write(&preference, "glow");
        let _ = std::fs::remove_file(&starting);
    }
    let preferred = std::env::var("GETCRAFT_RENDERER")
        .ok()
        .or_else(|| std::fs::read_to_string(&preference).ok().map(|s| s.trim().to_owned()));
    let renderers = match preferred.as_deref() {
        Some("glow") => vec![eframe::Renderer::Glow, eframe::Renderer::Wgpu],
        Some("wgpu") => vec![eframe::Renderer::Wgpu],
        _ => vec![eframe::Renderer::Wgpu, eframe::Renderer::Glow],
    };

    // The app is created only once a renderer works, so keep its inputs until then.
    let startup = Arc::new(Mutex::new(Some((paths, listener))));
    let mut last_error = String::new();
    for renderer in renderers {
        log::info!("starting with the {renderer:?} renderer");
        let _ = std::fs::write(&starting, format!("{renderer:?}"));
        let app_inputs = startup.clone();
        let options = eframe::NativeOptions {
            renderer,
            wgpu_options: wgpu_options(),
            viewport: egui::ViewportBuilder::default()
                .with_title("GetCraft")
                .with_icon(window_icon())
                .with_visible(!flags.hidden)
                .with_inner_size([1280.0, 820.0])
                .with_min_inner_size([900.0, 560.0]),
            ..Default::default()
        };
        let result = eframe::run_native(
            "GetCraft",
            options,
            Box::new(move |cc| {
                let (paths, listener) = app_inputs.lock().unwrap().take().ok_or("GetCraft was already started")?;
                Ok(Box::new(app::GetCraftApp::new(cc, paths, listener, flags)))
            }),
        );
        let _ = std::fs::remove_file(&starting);
        match result {
            Ok(()) => return Ok(()),
            // Once the app was created, the error came from the running app, not the renderer.
            Err(e) if startup.lock().unwrap().is_none() => return Err(e.to_string()),
            Err(e) => {
                log::warn!("the {renderer:?} renderer failed: {e}");
                last_error = e.to_string();
            }
        }
    }
    Err(last_error)
}

/// Called by the app once it's created, i.e. once the renderer started without crashing.
pub fn renderer_started(data_dir: &std::path::Path) {
    let _ = std::fs::remove_file(data_dir.join("renderer-starting"));
}

/// Which graphics APIs wgpu may use. On Windows that's DirectX 12 and OpenGL, but not Vulkan:
/// Windows' Vulkan drivers are a separate, much less tested path, and older Intel ones crash
/// outright (seen with igvk64.dll 30.0.101.1692 on UHD Graphics 620). `WGPU_BACKEND` overrides it.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut options = eframe::egui_wgpu::WgpuConfiguration::default();
    if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
        let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
        options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
    }
    options
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
