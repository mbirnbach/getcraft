//! Picks the right release asset for the current platform.
//!
//! The Crafting Apps publish `<id>-<version>-<os>-<arch>.<ext>` (e.g.
//! `photocraft-0.5.0-windows-x64-portable.zip`), but rather than hard-coding that pattern we
//! tokenise asset names and score them, so small naming changes upstream don't break installs.

use crate::platform::{Arch, Os, Platform};
use serde::{Deserialize, Serialize};

/// The package formats GetCraft knows how to install, per OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageKind {
    /// macOS disk image containing a `.app` bundle.
    Dmg,
    /// Windows portable zip (no installer, no admin rights needed).
    PortableZip,
    /// Linux AppImage (single self-contained executable).
    AppImage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Words that mark an asset as something other than the desktop app.
const EXCLUDED: &[&str] = &["cli", "web", "src", "source", "debug", "symbols", "dbg", "sdk"];

fn tokens(name: &str) -> Vec<String> {
    let lower = name
        .to_ascii_lowercase()
        .replace("x86_64", "x64")
        .replace("x86-64", "x64")
        .replace("amd64", "x64")
        .replace("aarch64", "arm64");
    lower.split(['-', '_', '.', ' ']).filter(|t| !t.is_empty()).map(str::to_owned).collect()
}

fn kind_of(name: &str, toks: &[String]) -> Option<(PackageKind, Os)> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".dmg") {
        Some((PackageKind::Dmg, Os::MacOs))
    } else if lower.ends_with(".appimage") {
        Some((PackageKind::AppImage, Os::Linux))
    } else if lower.ends_with(".zip") && toks.iter().any(|t| t == "windows" || t == "win" || t == "win64") {
        Some((PackageKind::PortableZip, Os::Windows))
    } else {
        None
    }
}

/// How well an asset's architecture fits the machine. `None` means it can't run here.
fn arch_score(toks: &[String], Platform { os, arch }: Platform) -> Option<u32> {
    let has = |t: &str| toks.iter().any(|x| x == t);
    let named = if has("universal") || has("universal2") {
        None
    } else if has("arm64") {
        Some(Arch::Arm64)
    } else if has("x64") || has("win64") {
        Some(Arch::X64)
    } else if has("x86") || has("i686") || has("i386") || has("win32") {
        Some(Arch::X86)
    } else {
        None
    };
    match named {
        // Universal or unlabeled builds run anywhere for their OS, but a native build wins.
        None => Some(5),
        Some(a) if a == arch => Some(10),
        // Windows on ARM emulates x64 and x86; x64 Windows runs x86. macOS has Rosetta.
        Some(Arch::X64) if arch == Arch::Arm64 && os != Os::Linux => Some(3),
        Some(Arch::X86) if arch != Arch::X86 && os == Os::Windows => Some(1),
        Some(_) => None,
    }
}

/// Returns the best installable asset for `platform`, if any.
pub fn select(assets: &[Asset], platform: Platform) -> Option<(&Asset, PackageKind)> {
    assets
        .iter()
        .filter_map(|asset| {
            let toks = tokens(&asset.name);
            if toks.iter().any(|t| EXCLUDED.contains(&t.as_str())) {
                return None;
            }
            let (kind, os) = kind_of(&asset.name, &toks)?;
            if os != platform.os {
                return None;
            }
            // On macOS a universal build is as good as native.
            let score = if os == Os::MacOs && toks.iter().any(|t| t.starts_with("universal")) {
                10
            } else {
                arch_score(&toks, platform)?
            };
            Some((score, asset, kind))
        })
        .max_by_key(|(score, ..)| *score)
        .map(|(_, asset, kind)| (asset, kind))
}

/// The checksum file published next to the binaries, if there is one.
pub fn checksum_file(assets: &[Asset]) -> Option<&Asset> {
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n == "sha256sums.txt" || n == "sha256sums" || n.ends_with(".sha256sums")
    })
}

/// Looks up `file` in a `sha256sum`-style listing (`<hex>  <name>` or `<hex> *<name>`).
pub fn find_checksum(listing: &str, file: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == file && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets(names: &[&str]) -> Vec<Asset> {
        names.iter().map(|n| Asset { name: n.to_string(), url: format!("https://x/{n}"), size: 1 }).collect()
    }

    /// The real asset list of photocraft v0.5.0.
    fn photocraft() -> Vec<Asset> {
        assets(&[
            "photocraft-0.5.0-freebsd-x86_64.tar.gz",
            "photocraft-0.5.0-linux-aarch64.AppImage",
            "photocraft-0.5.0-linux-aarch64.AppImage.zsync",
            "photocraft-0.5.0-linux-aarch64.deb",
            "photocraft-0.5.0-linux-aarch64.flatpak",
            "photocraft-0.5.0-linux-aarch64.rpm",
            "photocraft-0.5.0-linux-aarch64.tar.gz",
            "photocraft-0.5.0-linux-x86_64.AppImage",
            "photocraft-0.5.0-linux-x86_64.AppImage.zsync",
            "photocraft-0.5.0-linux-x86_64.deb",
            "photocraft-0.5.0-linux-x86_64.flatpak",
            "photocraft-0.5.0-linux-x86_64.rpm",
            "photocraft-0.5.0-linux-x86_64.tar.gz",
            "photocraft-0.5.0-macos-universal.dmg",
            "photocraft-0.5.0-windows-arm64-portable.zip",
            "photocraft-0.5.0-windows-arm64.msi",
            "photocraft-0.5.0-windows-x64-portable.zip",
            "photocraft-0.5.0-windows-x64.msi",
            "photocraft-0.5.0-windows-x86-portable.zip",
            "photocraft-0.5.0-windows-x86.msi",
            "photocraft-cli-0.5.0-macos-universal.zip",
            "photocraft-web-0.5.0.zip",
            "SHA256SUMS.txt",
        ])
    }

    fn pick(list: &[Asset], os: Os, arch: Arch) -> Option<&str> {
        select(list, Platform { os, arch }).map(|(a, _)| a.name.as_str())
    }

    #[test]
    fn picks_native_builds() {
        let list = photocraft();
        assert_eq!(pick(&list, Os::MacOs, Arch::Arm64), Some("photocraft-0.5.0-macos-universal.dmg"));
        assert_eq!(pick(&list, Os::MacOs, Arch::X64), Some("photocraft-0.5.0-macos-universal.dmg"));
        assert_eq!(pick(&list, Os::Windows, Arch::X64), Some("photocraft-0.5.0-windows-x64-portable.zip"));
        assert_eq!(pick(&list, Os::Windows, Arch::Arm64), Some("photocraft-0.5.0-windows-arm64-portable.zip"));
        assert_eq!(pick(&list, Os::Windows, Arch::X86), Some("photocraft-0.5.0-windows-x86-portable.zip"));
        assert_eq!(pick(&list, Os::Linux, Arch::X64), Some("photocraft-0.5.0-linux-x86_64.AppImage"));
        assert_eq!(pick(&list, Os::Linux, Arch::Arm64), Some("photocraft-0.5.0-linux-aarch64.AppImage"));
    }

    #[test]
    fn falls_back_to_emulated_arch() {
        let list = assets(&["tool-1.0.0-windows-x64-portable.zip", "tool-1.0.0-linux-x86_64.AppImage"]);
        assert_eq!(pick(&list, Os::Windows, Arch::Arm64), Some("tool-1.0.0-windows-x64-portable.zip"));
        assert_eq!(pick(&list, Os::Linux, Arch::Arm64), None);
    }

    #[test]
    fn never_picks_cli_or_web_bundles() {
        let list = assets(&["photocraft-cli-0.5.0-macos-universal.dmg", "photocraft-web-0.5.0.zip"]);
        assert_eq!(pick(&list, Os::MacOs, Arch::Arm64), None);
        assert_eq!(pick(&list, Os::Windows, Arch::X64), None);
    }

    #[test]
    fn reads_checksum_listings() {
        let listing = "ff56  short\n\
            f87fa09cb5a0f51e57aaa0383800446d469bbb0ade79c69c6bc4f38899b9b309  photocraft-0.5.0-linux-aarch64.AppImage\n\
            AD9FBE99BFC22259AEA41CF303FFC3735E483E74F8098FD23CDA730C621C1EB9 *photocraft.deb\n";
        assert_eq!(
            find_checksum(listing, "photocraft-0.5.0-linux-aarch64.AppImage").as_deref(),
            Some("f87fa09cb5a0f51e57aaa0383800446d469bbb0ade79c69c6bc4f38899b9b309")
        );
        assert_eq!(
            find_checksum(listing, "photocraft.deb").as_deref(),
            Some("ad9fbe99bfc22259aea41cf303ffc3735e483e74f8098fd23cda730c621c1eb9")
        );
        assert_eq!(find_checksum(listing, "short"), None);
    }
}
