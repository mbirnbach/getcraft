//! The release index: one JSON file describing every tool and its latest release.
//!
//! GitHub allows 60 unauthenticated API requests per hour per IP, and a full check costs one
//! request per tool. So a scheduled GitHub Action (with its own token) builds the index with
//! [`collect`] and publishes it as a static file, and launchers download just that file, which is
//! served from a CDN without API limits. Launchers query the API directly only when the index is
//! unavailable or stale.

use crate::catalog::{Catalog, Category, Tool};
use crate::github::{Client, Release};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const INDEX_URL: &str = "https://raw.githubusercontent.com/mbirnbach/getcraft/index/index.json";

/// Bumped on incompatible format changes; launchers ignore indexes they don't understand.
pub const FORMAT: u32 = 1;

/// An index older than this is treated as broken and the launcher asks GitHub itself.
pub const MAX_AGE_SECS: u64 = 6 * 3600;

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub format: u32,
    /// Unix seconds.
    pub generated_at: u64,
    pub categories: Vec<Category>,
    pub tools: Vec<IndexedTool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexedTool {
    pub tool: Tool,
    pub release: Option<Release>,
}

/// Tools and their latest releases, as gathered from one source.
pub struct Collected {
    pub categories: Vec<Category>,
    pub tools: Vec<IndexedTool>,
    /// Set when some lookups failed; `tools` then holds whatever was gathered.
    pub error: Option<Error>,
}

/// Asks the GitHub API directly: scans for new `*craft` repositories, then fetches the latest
/// release of every tool. Discovered repositories without a release are left out.
pub fn collect(client: &Client, catalog: Catalog) -> Collected {
    let mut tools = catalog.tools.clone();
    for owner in &catalog.discovery.owners {
        match client.repos_of(owner) {
            Ok(repos) => {
                for repo in repos {
                    let id = repo.name.to_ascii_lowercase();
                    if id.ends_with("craft")
                        && !repo.archived
                        && !repo.fork
                        && !catalog.is_known(&repo.name)
                        && !tools.iter().any(|t| t.id == id)
                    {
                        tools.push(Tool::discovered(owner, &repo.name, repo.description.as_deref()));
                    }
                }
            }
            Err(e) => log::warn!("could not scan {owner} for new tools: {e}"),
        }
    }

    let mut out = Vec::new();
    let mut error = None;
    for tool in tools {
        match client.latest_release(&tool.repo) {
            Ok(release) if release.is_some() || !tool.discovered => out.push(IndexedTool { tool, release }),
            Ok(_) => {}
            Err(e) => {
                let stop = matches!(e, Error::RateLimited);
                error.get_or_insert(e);
                if stop {
                    break;
                }
            }
        }
    }
    Collected { categories: catalog.categories, tools: out, error }
}

/// The index location; `GETCRAFT_INDEX_URL` overrides it for forks and local testing.
pub fn url() -> String {
    std::env::var("GETCRAFT_INDEX_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| INDEX_URL.to_owned())
}

/// Downloads the published index, rejecting unknown formats and stale files.
pub fn fetch(client: &Client, now: u64) -> Result<Index> {
    let index: Index = serde_json::from_str(&client.fetch_text(&url())?)?;
    if index.format != FORMAT {
        return Err(Error::Parse(format!("unsupported index format {}", index.format)));
    }
    if now.saturating_sub(index.generated_at) > MAX_AGE_SECS {
        return Err(Error::Parse("index is stale".into()));
    }
    Ok(index)
}

impl Index {
    pub fn from_collected(collected: Collected, generated_at: u64) -> Self {
        Self { format: FORMAT, generated_at, categories: collected.categories, tools: collected.tools }
    }
}
