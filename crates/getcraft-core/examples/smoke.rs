//! End-to-end check against live GitHub: `cargo run -p getcraft-core --example smoke [tool-id]`.
//! Everything goes to a temporary directory; nothing touches your real Applications folder.

use getcraft_core::engine::{Engine, Job};
use getcraft_core::install::Installer;
use getcraft_core::state::Paths;
use std::time::Duration;

fn main() {
    env_logger_lite();
    let root = std::env::temp_dir().join(format!("getcraft-smoke-{}", std::process::id()));
    let paths = Paths::in_dir(&root);
    let installer = Installer::with_apps_dir(root.join("Applications"), &paths);
    let engine = Engine::new(paths, installer, None, || {}, |event| println!("event: {event:?}"));

    engine.refresh();
    wait(|| !engine.snapshot().checking);
    let snap = engine.snapshot();
    if let Some(e) = &snap.last_error {
        println!("check error: {e}");
    }
    for t in &snap.tools {
        let pkg = t.latest.as_ref().and_then(|l| l.package.as_ref()).map(|(a, _)| a.name.as_str());
        let ver = t.latest.as_ref().map(|l| l.version.as_str());
        println!("{:<12} {:<8} {}", t.tool.id, ver.unwrap_or("-"), pkg.unwrap_or("(no build for this platform)"));
    }

    let Some(id) = std::env::args().nth(1) else { return };
    engine.install(&id);
    let mut last = 0;
    wait(|| {
        let s = engine.snapshot();
        let e = s.tools.iter().find(|t| t.tool.id == id).unwrap();
        if let Some(Job::Downloading { done, total: Some(total) }) = e.job {
            let pct = done * 100 / total.max(1);
            if pct >= last + 20 {
                println!("  {pct}%");
                last = pct;
            }
        }
        e.job.is_none()
    });
    let s = engine.snapshot();
    let e = s.tools.iter().find(|t| t.tool.id == id).unwrap();
    println!("installed: {:?}\nerror: {:?}", e.installed, e.error);
    println!("state dir: {}", root.display());
}

fn wait(mut done: impl FnMut() -> bool) {
    std::thread::sleep(Duration::from_millis(200));
    while !done() {
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn env_logger_lite() {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                eprintln!("[{}] {}", r.level(), r.args());
            }
        }
        fn flush(&self) {}
    }
    let _ = log::set_logger(&L).map(|()| log::set_max_level(log::LevelFilter::Info));
}
