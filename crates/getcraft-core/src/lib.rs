//! Core of the GetCraft launcher: the tool catalog, GitHub release lookup, platform-specific
//! installers and the update engine. Everything here is UI-agnostic so the same engine can drive
//! the desktop window and (later) a headless background updater.

pub mod assets;
pub mod catalog;
pub mod download;
pub mod engine;
pub mod github;
pub mod index;
pub mod install;
pub mod platform;
pub mod state;
pub mod version;

/// Sent with every HTTP request so the ArtCraft team can identify (and contact) us in their logs.
pub const USER_AGENT: &str =
    concat!("GetCraft/", env!("CARGO_PKG_VERSION"), " (+https://github.com/mbirnbach/getcraft)");

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(String),
    #[error("GitHub rate limit reached, try again in a few minutes")]
    RateLimited,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    Parse(String),
    #[error("download is corrupted (checksum mismatch)")]
    Checksum { expected: String, actual: String },
    #[error("{0}")]
    Install(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Http(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Parse(e.to_string())
    }
}
