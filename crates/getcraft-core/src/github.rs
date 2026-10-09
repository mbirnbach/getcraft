//! A small GitHub REST client with ETag caching.
//!
//! Unauthenticated clients get 60 API requests per hour. Responses are cached with their ETag and
//! revalidated with `If-None-Match`, so routine update checks mostly come back as cheap
//! `304 Not Modified` responses.

use crate::assets::Asset;
use crate::{Error, Result, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

const API: &str = "https://api.github.com";

/// Upper bounds for small downloads, so a broken or hostile server can't exhaust memory.
const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

impl Release {
    pub fn assets(&self) -> Vec<Asset> {
        self.assets
            .iter()
            .map(|a| Asset { name: a.name.clone(), url: a.browser_download_url.clone(), size: a.size })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Repo {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct HttpCache {
    entries: HashMap<String, CacheEntry>,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    etag: String,
    body: String,
}

pub struct Client {
    agent: ureq::Agent,
    token: Option<String>,
    cache: Mutex<HttpCache>,
    cache_path: Option<PathBuf>,
}

impl Client {
    /// `cache_path` persists ETags between launches; pass `None` for an in-memory cache.
    pub fn new(cache_path: Option<PathBuf>) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build()
            .into();
        let cache = cache_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            agent,
            // Lets developers (and CI) lift the rate limit; regular users never need this.
            token: std::env::var("GETCRAFT_GITHUB_TOKEN").ok().filter(|t| !t.is_empty()),
            cache: Mutex::new(cache),
            cache_path,
        }
    }

    pub fn agent(&self) -> &ureq::Agent {
        &self.agent
    }

    /// GETs a GitHub API URL, revalidating against the cache. `Ok(None)` means 404.
    fn api_get(&self, url: &str) -> Result<Option<String>> {
        let etag = self.cache.lock().unwrap().entries.get(url).map(|e| e.etag.clone());
        let mut req = self.agent.get(url).header("Accept", "application/vnd.github+json");
        if let Some(etag) = &etag {
            req = req.header("If-None-Match", etag);
        }
        if let Some(token) = &self.token {
            req = req.header("Authorization", &format!("Bearer {token}"));
        }
        let mut resp = req.call()?;
        let status = resp.status().as_u16();
        let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
        match status {
            200 => {
                let new_etag = header("etag");
                let body = resp.body_mut().read_to_string()?;
                if let Some(etag) = new_etag {
                    self.cache.lock().unwrap().entries.insert(url.to_owned(), CacheEntry { etag, body: body.clone() });
                }
                Ok(Some(body))
            }
            304 => Ok(self.cache.lock().unwrap().entries.get(url).map(|e| e.body.clone())),
            404 => Ok(None),
            403 | 429 if header("x-ratelimit-remaining").as_deref() == Some("0") || status == 429 => {
                Err(Error::RateLimited)
            }
            _ => Err(Error::Http(format!("GitHub returned HTTP {status} for {url}"))),
        }
    }

    /// The newest non-prerelease, non-draft release of `owner/repo`.
    pub fn latest_release(&self, repo: &str) -> Result<Option<Release>> {
        match self.api_get(&format!("{API}/repos/{repo}/releases/latest"))? {
            Some(body) => Ok(Some(serde_json::from_str(&body)?)),
            None => Ok(None),
        }
    }

    /// The release of `owner/repo` tagged `tag`, if there is one.
    pub fn release_by_tag(&self, repo: &str, tag: &str) -> Result<Option<Release>> {
        match self.api_get(&format!("{API}/repos/{repo}/releases/tags/{tag}"))? {
            Some(body) => Ok(Some(serde_json::from_str(&body)?)),
            None => Ok(None),
        }
    }

    pub fn repos_of(&self, owner: &str) -> Result<Vec<Repo>> {
        // Organisations and users have different endpoints; try the org one first.
        for kind in ["orgs", "users"] {
            if let Some(body) = self.api_get(&format!("{API}/{kind}/{owner}/repos?per_page=100&sort=created"))? {
                return Ok(serde_json::from_str(&body)?);
            }
        }
        Ok(Vec::new())
    }

    /// Plain (non-API) GET for small text files such as the index, checksum lists or signatures.
    /// Responses over `MAX_TEXT_BYTES` are rejected.
    pub fn fetch_text(&self, url: &str) -> Result<String> {
        let mut resp = self.agent.get(url).call()?;
        if resp.status().as_u16() != 200 {
            return Err(Error::Http(format!("HTTP {} for {url}", resp.status().as_u16())));
        }
        Ok(resp.body_mut().with_config().limit(MAX_TEXT_BYTES).read_to_string()?)
    }

    /// GET for small binary files such as icons; responses over `MAX_BYTES` are rejected.
    pub fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let mut resp = self.agent.get(url).call()?;
        if resp.status().as_u16() != 200 {
            return Err(Error::Http(format!("HTTP {} for {url}", resp.status().as_u16())));
        }
        Ok(resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec()?)
    }

    /// Writes the ETag cache to disk.
    pub fn save_cache(&self) {
        let Some(path) = &self.cache_path else { return };
        let json = serde_json::to_vec(&*self.cache.lock().unwrap());
        if let Err(e) = json.map_err(Error::from).and_then(|j| crate::state::write_atomic(path, &j)) {
            log::warn!("could not save HTTP cache: {e}");
        }
    }
}
