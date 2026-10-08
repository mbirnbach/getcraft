//! Everything GetCraft remembers between launches: installed tools and user settings.

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// What to do when a new version of an installed tool is released.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdatePolicy {
    /// Show a notification and let the user decide.
    #[default]
    Notify,
    /// Install updates automatically (once the tool isn't running).
    Auto,
    /// Never check this tool.
    Off,
}

impl UpdatePolicy {
    pub const ALL: [UpdatePolicy; 3] = [UpdatePolicy::Auto, UpdatePolicy::Notify, UpdatePolicy::Off];

    pub fn label(self) -> &'static str {
        match self {
            UpdatePolicy::Notify => "Notify me",
            UpdatePolicy::Auto => "Update automatically",
            UpdatePolicy::Off => "Don't check",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_policy: UpdatePolicy,
    /// Per-tool overrides of `default_policy`.
    pub policies: BTreeMap<String, UpdatePolicy>,
    pub check_interval_hours: u32,
    /// Closing the window keeps GetCraft running in the menu bar / tray.
    pub run_in_background: bool,
    pub launch_at_login: bool,
    /// Whether we've told the user that closing the window doesn't quit.
    pub background_hint_shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_policy: UpdatePolicy::Notify,
            policies: BTreeMap::new(),
            check_interval_hours: 6,
            run_in_background: true,
            launch_at_login: true,
            background_hint_shown: false,
        }
    }
}

impl Settings {
    pub fn policy_for(&self, id: &str) -> UpdatePolicy {
        self.policies.get(id).copied().unwrap_or(self.default_policy)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstallRecord {
    pub version: String,
    /// What gets launched: the `.app` bundle on macOS, the executable elsewhere.
    pub path: PathBuf,
    /// Unix seconds.
    pub installed_at: u64,
    /// False when the tool was installed by hand and GetCraft merely found it.
    #[serde(default = "yes")]
    pub managed: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub installed: BTreeMap<String, InstallRecord>,
    pub settings: Settings,
    /// The last version we notified about per tool, so each release notifies once.
    pub notified: BTreeMap<String, String>,
    /// Unix seconds of the last completed update check.
    pub last_check: Option<u64>,
}

impl State {
    /// Loads the state, falling back to defaults if the file is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                log::warn!("ignoring corrupt state file {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }
}

/// Writes via a temporary file and rename so a crash never leaves a truncated file behind.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Where GetCraft keeps its own files.
#[derive(Clone, Debug)]
pub struct Paths {
    pub state_file: PathBuf,
    pub http_cache_file: PathBuf,
    pub downloads_dir: PathBuf,
    /// Scratch space for unpacking (e.g. DMG mount points).
    pub work_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Option<Self> {
        let config = dirs::config_dir()?.join("GetCraft");
        let cache = dirs::cache_dir()?.join("GetCraft");
        Some(Self {
            state_file: config.join("state.json"),
            http_cache_file: cache.join("http-cache.json"),
            downloads_dir: cache.join("downloads"),
            work_dir: cache.join("work"),
        })
    }

    /// All paths under one directory (for tests and portable setups).
    pub fn in_dir(root: &Path) -> Self {
        Self {
            state_file: root.join("state.json"),
            http_cache_file: root.join("http-cache.json"),
            downloads_dir: root.join("downloads"),
            work_dir: root.join("work"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut state = State::default();
        state.settings.policies.insert("photocraft".into(), UpdatePolicy::Auto);
        state.installed.insert(
            "photocraft".into(),
            InstallRecord {
                version: "0.5.0".into(),
                path: "/Applications/PhotoCraft.app".into(),
                installed_at: 1,
                managed: true,
            },
        );
        state.save(&path).unwrap();
        let loaded = State::load(&path);
        assert_eq!(loaded.settings.policy_for("photocraft"), UpdatePolicy::Auto);
        assert_eq!(loaded.settings.policy_for("pdfcraft"), UpdatePolicy::Notify);
        assert_eq!(loaded.installed["photocraft"].version, "0.5.0");
    }

    #[test]
    fn corrupt_state_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"{not json").unwrap();
        assert!(State::load(&path).installed.is_empty());
    }
}
