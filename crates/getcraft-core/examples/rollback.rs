//! Checks keeping and switching back to a previous version with real releases, on any system:
//! installs PhotoCraft 0.3.0, updates to 0.5.0 keeping 0.3.0, switches back and forth, then
//! checks that a failed update restores the current copy.
//! `cargo run -p getcraft-core --example rollback`
//! Everything goes to a temporary directory; nothing touches your real Applications folder.

use getcraft_core::assets::{self, PackageKind};
use getcraft_core::catalog::Catalog;
use getcraft_core::github::{Client, Release};
use getcraft_core::install::Installer;
use getcraft_core::platform::Platform;
use getcraft_core::state::Paths;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const TOOL: &str = "photocraft";
const OLD: &str = "v0.3.0";
const NEW: &str = "v0.5.0";

fn main() {
    let root = std::env::temp_dir().join(format!("getcraft-rollback-{}", std::process::id()));
    let paths = Paths::in_dir(&root);
    let installer = Installer::with_apps_dir(root.join("Applications"), &paths);
    let tool = Catalog::bundled().tools.into_iter().find(|t| t.id == TOOL).expect("tool in catalog");
    let client = Client::new(None);
    let platform = Platform::current().expect("supported platform");

    let (old_pkg, kind) = fetch(&client, &tool.repo, OLD, platform, &paths.downloads_dir);
    let (new_pkg, _) = fetch(&client, &tool.repo, NEW, platform, &paths.downloads_dir);

    let first = installer.install(&tool, &old_pkg, kind, None, true, None).expect("install 0.3.0");
    assert!(first.kept.is_none(), "nothing to keep on a fresh install");
    println!("installed 0.3.0 at {}", first.path.display());
    let marker = mark(&first.path, "old");

    let update = installer.install(&tool, &new_pkg, kind, Some(&first.path), true, None).expect("update to 0.5.0");
    let kept = update.kept.expect("0.3.0 is kept");
    println!("updated to 0.5.0 at {}, kept 0.3.0 at {}", update.path.display(), kept.display());
    assert_eq!(update.path, first.path, "updated in place");
    assert!(kept.starts_with(&paths.previous_dir));
    assert!(has_marker(&kept, &marker), "the kept copy is the old one");
    mark(&update.path, "new");

    let (launch, now_kept) = installer.switch_to_kept(&tool, &update.path, &kept).expect("switch back");
    println!("switched back: {} (kept {})", launch.display(), now_kept.display());
    assert_eq!(launch, update.path);
    assert!(has_marker(&launch, "old"), "0.3.0 is installed again");
    assert!(has_marker(&now_kept, "new"), "0.5.0 is kept");
    version_is(&installer, &launch, "0.3.0");

    let (launch, kept) = installer.switch_to_kept(&tool, &launch, &now_kept).expect("switch forward");
    assert!(has_marker(&launch, "new"), "0.5.0 is installed again");
    assert!(has_marker(&kept, "old"));
    version_is(&installer, &launch, "0.5.0");
    println!("switched forward again");

    // A failed update (a broken package) must leave the current copy in place.
    let broken = paths.downloads_dir.join(format!("broken.{}", ext(kind)));
    std::fs::write(&broken, b"not a package").unwrap();
    assert!(installer.install(&tool, &broken, kind, Some(&launch), true, None).is_err());
    assert!(has_marker(&launch, "new"), "the current copy was restored");
    assert!(has_marker(&kept, "old"), "the kept copy survived the failed update");
    println!("a failed update restored the current copy");

    // A kept copy that was changed afterwards is refused on macOS, where it's signed.
    if cfg!(target_os = "macos") {
        std::fs::write(kept.join("Contents/planted"), b"x").unwrap();
        assert!(installer.switch_to_kept(&tool, &launch, &kept).is_err(), "a modified kept app is refused");
        assert!(has_marker(&launch, "new"), "the current copy stays");
        println!("a modified kept app was refused");
    }

    installer.remove_kept(TOOL).unwrap();
    assert!(!paths.previous_dir.join(TOOL).exists());
    std::fs::remove_dir_all(&root).ok();
    println!("rollback round trip OK");
}

/// Downloads this platform's package of `repo@tag`, checked against the release's checksums.
fn fetch(client: &Client, repo: &str, tag: &str, platform: Platform, dir: &Path) -> (PathBuf, PackageKind) {
    let release: Release = serde_json::from_str(
        &client.fetch_text(&format!("https://api.github.com/repos/{repo}/releases/tags/{tag}")).unwrap(),
    )
    .unwrap();
    let list = release.assets();
    let (asset, kind) = assets::select(&list, platform).expect("a build for this platform");
    let sums = client.fetch_text(&assets::checksum_file(&list).unwrap().url).unwrap();
    let sha = assets::find_checksum(&sums, &asset.name).unwrap();
    let dest = dir.join(&asset.name);
    std::fs::create_dir_all(dir).unwrap();
    println!("downloading {}", asset.name);
    getcraft_core::download::download(
        client.agent(),
        &asset.url,
        &dest,
        &sha,
        asset.size,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    (dest, kind)
}

fn ext(kind: PackageKind) -> &'static str {
    match kind {
        PackageKind::Dmg => "dmg",
        PackageKind::AppImage => "AppImage",
        _ => "zip",
    }
}

/// Drops a marker file into the installed folder so the test can tell the copies apart. Not on
/// macOS: anything added to a bundle breaks its signature; there the bundle version tells.
fn mark(launch: &Path, name: &str) -> String {
    if !cfg!(target_os = "macos") {
        let root = getcraft_core::install::install_root(launch);
        std::fs::write(root.join(format!("getcraft-test-{name}")), b"").unwrap();
    }
    name.into()
}

fn has_marker(path: &Path, name: &str) -> bool {
    if cfg!(target_os = "macos") {
        let version = if name == "old" { "0.3.0" } else { "0.5.0" };
        return bundle_version(path).as_deref() == Some(version);
    }
    let root = if path.is_file() { getcraft_core::install::install_root(path) } else { path };
    root.join(format!("getcraft-test-{name}")).exists()
}

fn bundle_version(app: &Path) -> Option<String> {
    let out = std::process::Command::new("defaults")
        .arg("read")
        .arg(app.join("Contents/Info"))
        .arg("CFBundleShortVersionString")
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn version_is(installer: &Installer, launch: &Path, expected: &str) {
    if cfg!(target_os = "macos") {
        let tool = Catalog::bundled().tools.into_iter().find(|t| t.id == TOOL).unwrap();
        let record = getcraft_core::state::InstallRecord {
            version: String::new(),
            path: launch.to_path_buf(),
            installed_at: 0,
            managed: true,
            msi: false,
            previous: None,
            rolled_back_from: None,
        };
        assert_eq!(installer.installed_version(&tool, &record).as_deref(), Some(expected));
    }
}
