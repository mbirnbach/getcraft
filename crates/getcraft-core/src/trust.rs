//! What GetCraft is willing to install, decided in code rather than by remote data.
//!
//! The catalog and release index are fetched from the network and could be tampered with. They
//! may describe tools, but they can't widen what is trusted: tools must come from a publisher
//! listed here, every download must come from that tool's own GitHub releases, and on macOS the
//! installed app must carry the publisher's Apple signature.

/// A GitHub owner whose releases GetCraft installs, with the identity its macOS apps are signed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Publisher {
    pub github_owner: &'static str,
    /// Apple Developer Team ID that signs the macOS apps.
    pub macos_team: &'static str,
    /// macOS bundle identifiers are this prefix plus the tool id.
    pub bundle_prefix: &'static str,
    /// The `Manufacturer` of their Windows Installer packages, as shown in Settings → Apps.
    pub windows_manufacturer: &'static str,
}

/// The ArtCraft team, publisher of the Crafting Apps.
pub const PUBLISHERS: &[Publisher] = &[Publisher {
    github_owner: "storytold",
    macos_team: "DJ6XS33FX8",
    bundle_prefix: "ai.storyteller.",
    windows_manufacturer: "Learning Machines LLC",
}];

/// GetCraft itself, for self-updates.
pub const GETCRAFT_REPO: &str = "mbirnbach/getcraft";
pub const GETCRAFT_MACOS_TEAM: &str = "J829HHBMPW";
pub const GETCRAFT_BUNDLE_ID: &str = "net.brnbch.getcraft";

/// What a downloaded macOS app must be signed as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacIdentity {
    pub bundle_id: String,
    pub team: String,
}

/// Tool ids end up in file and folder names, so only allow plain lowercase names.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

/// Display names end up in file names (shortcuts, `.app` names) and desktop entries.
pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 64
        && !name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        && name != "."
        && name != ".."
}

/// Removes control characters (newlines included) from text shown or written anywhere.
pub fn clean_text(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The publisher of `owner/repo`, if it's a trusted one and the name is well-formed.
pub fn publisher_for(repo: &str) -> Option<&'static Publisher> {
    let (owner, name) = repo.split_once('/')?;
    if name.is_empty() || name.contains('/') || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return None;
    }
    PUBLISHERS.iter().find(|p| p.github_owner.eq_ignore_ascii_case(owner))
}

/// True only for files attached to a release of `repo` on github.com itself.
pub fn is_release_asset_of(url: &str, repo: &str) -> bool {
    let prefix = format!("https://github.com/{repo}/releases/download/");
    url.len() > prefix.len()
        && url[..prefix.len()].eq_ignore_ascii_case(&prefix)
        && !url[prefix.len()..].contains("..")
        && !url.contains(['?', '#', '@', '\\'])
}

/// The signature a tool's macOS app must have.
pub fn mac_identity(tool_id: &str, repo: &str) -> Option<MacIdentity> {
    let publisher = publisher_for(repo)?;
    Some(MacIdentity { bundle_id: format!("{}{tool_id}", publisher.bundle_prefix), team: publisher.macos_team.into() })
}

pub fn getcraft_identity() -> MacIdentity {
    MacIdentity { bundle_id: GETCRAFT_BUNDLE_ID.into(), team: GETCRAFT_MACOS_TEAM.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cannot_escape_paths() {
        assert!(valid_id("photocraft"));
        assert!(valid_id("new-craft2"));
        for bad in ["", "../x", "a/b", "a\\b", "..", "Photo", "-x", "a b", "a\nb", &"x".repeat(65)] {
            assert!(!valid_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn names_are_plain() {
        assert!(valid_name("CADCraft"));
        assert!(valid_name("Photo Craft"));
        for bad in ["", "  ", "a/b", "..", "Evil\nExec=rm", "x:y"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        assert_eq!(clean_text("a\nb\tc"), "a b c");
    }

    #[test]
    fn only_trusted_publishers() {
        assert!(publisher_for("storytold/photocraft").is_some());
        assert!(publisher_for("StoryTold/photocraft").is_some());
        assert!(publisher_for("evil/photocraft").is_none());
        assert!(publisher_for("storytold/../evil").is_none());
        assert!(publisher_for("storytold").is_none());
    }

    #[test]
    fn asset_urls_must_belong_to_the_repo() {
        let repo = "storytold/photocraft";
        assert!(is_release_asset_of(
            "https://github.com/storytold/photocraft/releases/download/v0.5.0/photocraft-0.5.0-macos-universal.dmg",
            repo
        ));
        for bad in [
            "https://github.com/storytold/photocraft2/releases/download/v1/x.dmg",
            "https://github.com/evil/photocraft/releases/download/v1/x.dmg",
            "http://github.com/storytold/photocraft/releases/download/v1/x.dmg",
            "https://github.com.evil.com/storytold/photocraft/releases/download/v1/x.dmg",
            "https://github.com/storytold/photocraft/releases/download/../../../evil/x.dmg",
            "https://github.com/storytold/photocraft/releases/download/",
        ] {
            assert!(!is_release_asset_of(bad, repo), "{bad}");
        }
    }

    #[test]
    fn identities() {
        let id = mac_identity("photocraft", "storytold/photocraft").unwrap();
        assert_eq!(id.bundle_id, "ai.storyteller.photocraft");
        assert_eq!(id.team, "DJ6XS33FX8");
        assert!(mac_identity("x", "evil/x").is_none());
    }
}
