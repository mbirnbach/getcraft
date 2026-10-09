//! Windows only, for CI or a throwaway machine: checks that GetCraft finds a Crafting App that
//! was installed with its own `.msi`, updates it by running the newer `.msi`, and uninstalls it.
//! This changes the real system (installs for all users) and needs admin rights. Install an
//! older `.msi` first, then:
//! `cargo run -p getcraft-core --example msi_roundtrip <tool-id> <installed-version>`

use getcraft_core::engine::Engine;
use getcraft_core::install::Installer;
use getcraft_core::state::Paths;
use std::time::Duration;

fn main() {
    if !cfg!(windows) {
        eprintln!("Windows only");
        return;
    }
    env_logger_lite();
    let mut args = std::env::args().skip(1);
    let (Some(id), Some(old)) = (args.next(), args.next()) else {
        eprintln!("usage: msi_roundtrip <tool-id> <installed-version>");
        std::process::exit(2);
    };
    let root = std::env::temp_dir().join(format!("getcraft-msi-{}", std::process::id()));
    let paths = Paths::in_dir(&root);
    // The real installer, so it looks at the system's MSI registrations.
    let installer = Installer::new(&paths);
    let engine = Engine::new(paths, installer, None, || {}, |event| println!("event: {event:?}"));
    let entry = || engine.snapshot().tools.into_iter().find(|t| t.tool.id == id).expect("tool in catalog");

    engine.refresh();
    wait(|| !engine.snapshot().checking);
    let found = entry().installed.expect("the MSI copy is found");
    println!("found: {found:?}");
    assert!(found.msi && !found.managed, "recorded as an MSI copy");
    assert_eq!(found.version, old);
    assert!(found.path.starts_with(std::env::var("ProgramFiles").unwrap()), "installed in Program Files");
    let latest = entry().latest.expect("a release");
    println!("latest: {} ({:?})", latest.version, latest.msi.as_ref().map(|a| &a.name));
    assert!(entry().update_available(), "an update is offered");

    engine.install(&id);
    wait(|| entry().job.is_none());
    let e = entry();
    println!("updated: {:?}, error: {:?}", e.installed, e.error);
    assert_eq!(e.error, None);
    let updated = e.installed.expect("still installed");
    assert!(updated.msi, "still the MSI copy");
    assert!(updated.path.is_file());
    assert!(!updated.path.starts_with(&root), "no portable copy was made");
    engine.refresh();
    wait(|| !engine.snapshot().checking);
    let e = entry();
    assert_eq!(e.installed.as_ref().map(|i| i.version.as_str()), Some(latest.version.split('-').next().unwrap()));
    assert!(!e.update_available());

    engine.uninstall(&id);
    wait(|| entry().job.is_none());
    let e = entry();
    println!("after uninstall: {:?}, error: {:?}", e.installed, e.error);
    assert_eq!(e.error, None);
    assert!(e.installed.is_none());
    assert!(!updated.path.exists(), "the program is gone");
    engine.refresh();
    wait(|| !engine.snapshot().checking);
    assert!(entry().installed.is_none(), "not found again after uninstalling");
    println!("MSI round trip OK");
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
