//! The update engine: owns the catalog, release info and install state, runs downloads and
//! installs on background threads, and applies each tool's update policy.
//!
//! The engine is UI-agnostic. Front ends read [`Engine::snapshot`] and get told to redraw through
//! the `on_change` callback; user-facing happenings (an update to announce, a failed install) go
//! through `on_event`.

use crate::assets::{self, Asset, PackageKind};
use crate::catalog::{Catalog, Category, REMOTE_URL, Tool};
use crate::github::{Client, Release};
use crate::index::{self, Collected};
use crate::install::{self, Installer};
use crate::platform::Platform;
use crate::state::{InstallRecord, Paths, Settings, State, UpdatePolicy};
use crate::{Error, Result, download, version};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct LatestRelease {
    /// Normalised version, e.g. `0.5.0`.
    pub version: String,
    pub html_url: String,
    pub published_at: Option<String>,
    pub notes: String,
    /// The package to install on this platform, if the release has one.
    pub package: Option<(Asset, PackageKind)>,
    pub checksums: Option<Asset>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Downloading { done: u64, total: Option<u64> },
    Installing,
    Removing,
}

#[derive(Clone, Debug)]
pub struct ToolEntry {
    pub tool: Tool,
    pub latest: Option<LatestRelease>,
    pub installed: Option<InstallRecord>,
    pub job: Option<Job>,
    pub error: Option<String>,
    pub policy: UpdatePolicy,
}

impl ToolEntry {
    fn new(tool: Tool) -> Self {
        Self { tool, latest: None, installed: None, job: None, error: None, policy: UpdatePolicy::default() }
    }

    /// A build for this platform exists.
    pub fn available(&self) -> bool {
        self.latest.as_ref().is_some_and(|l| l.package.is_some())
    }

    pub fn update_available(&self) -> bool {
        match (&self.installed, &self.latest) {
            (Some(i), Some(l)) => l.package.is_some() && version::is_newer(&l.version, &i.version),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub tools: Vec<ToolEntry>,
    pub categories: Vec<Category>,
    pub checking: bool,
    pub last_check: Option<u64>,
    pub last_error: Option<String>,
    pub settings: Settings,
    pub platform: Option<Platform>,
    pub apps_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub enum Event {
    UpdateAvailable { tool: String, version: String },
    Installed { tool: String, version: String, updated: bool },
    Failed { tool: String, error: String },
}

struct Model {
    categories: Vec<Category>,
    entries: Vec<ToolEntry>,
    state: State,
    checking: bool,
    /// Unix seconds of the last check attempt, successful or not.
    last_attempt: u64,
    last_error: Option<String>,
    cancels: HashMap<String, Arc<AtomicBool>>,
}

impl Model {
    fn entry(&mut self, id: &str) -> Option<&mut ToolEntry> {
        self.entries.iter_mut().find(|e| e.tool.id == id)
    }
}

const RETRY_AFTER_SECS: u64 = 15 * 60;

type Callback<T> = Box<dyn Fn(T) + Send + Sync>;

struct Inner {
    model: Mutex<Model>,
    client: Client,
    installer: Installer,
    paths: Paths,
    platform: Option<Platform>,
    on_change: Callback<()>,
    on_event: Callback<Event>,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Engine {
    pub fn new(
        paths: Paths,
        installer: Installer,
        on_change: impl Fn() + Send + Sync + 'static,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Self {
        let state = State::load(&paths.state_file);
        let catalog = Catalog::bundled();
        let mut entries: Vec<ToolEntry> = catalog.tools.iter().cloned().map(ToolEntry::new).collect();
        for entry in &mut entries {
            entry.installed = state.installed.get(&entry.tool.id).cloned();
        }
        let model = Model {
            categories: catalog.categories,
            entries,
            state,
            checking: false,
            last_attempt: 0,
            last_error: None,
            cancels: HashMap::new(),
        };
        Self {
            inner: Arc::new(Inner {
                model: Mutex::new(model),
                client: Client::new(Some(paths.http_cache_file.clone())),
                installer,
                paths,
                platform: Platform::current(),
                on_change: Box::new(move |()| on_change()),
                on_event: Box::new(on_event),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Model> {
        self.inner.model.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn changed(&self) {
        (self.inner.on_change)(());
    }

    fn emit(&self, event: Event) {
        (self.inner.on_event)(event);
    }

    fn save(&self, model: &Model) {
        if let Err(e) = model.state.save(&self.inner.paths.state_file) {
            log::error!("could not save state: {e}");
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let m = self.lock();
        let tools = m
            .entries
            .iter()
            .map(|e| ToolEntry { policy: m.state.settings.policy_for(&e.tool.id), ..e.clone() })
            .collect();
        Snapshot {
            tools,
            categories: m.categories.clone(),
            checking: m.checking,
            last_check: m.state.last_check,
            last_error: m.last_error.clone(),
            settings: m.state.settings.clone(),
            platform: self.inner.platform,
            apps_dir: self.inner.installer.apps_dir.clone(),
        }
    }

    /// Checks GitHub for new tools and releases in the background, then applies update policies.
    pub fn refresh(&self) {
        {
            let mut m = self.lock();
            if m.checking {
                return;
            }
            m.checking = true;
            m.last_attempt = now();
            m.last_error = None;
        }
        self.changed();
        let this = self.clone();
        thread::spawn(move || {
            let result = this.check();
            if let Err(e) = &result {
                log::warn!("update check failed: {e}");
            }
            {
                let mut m = this.lock();
                m.checking = false;
                m.last_error = result.err().map(|e| e.to_string());
            }
            this.changed();
            this.apply_policies();
        });
    }

    fn check(&self) -> Result<()> {
        let client = &self.inner.client;
        let collected = match index::fetch(client, now()) {
            Ok(index) => Collected { categories: index.categories, tools: index.tools, error: None },
            Err(e) => {
                log::info!("release index unavailable ({e}), asking GitHub directly");
                let catalog = match client.fetch_text(REMOTE_URL).and_then(|t| Catalog::parse(&t)) {
                    Ok(c) => c,
                    Err(e) => {
                        log::debug!("using bundled catalog ({e})");
                        Catalog::bundled()
                    }
                };
                index::collect(client, catalog)
            }
        };
        let first_error = collected.error;
        let mut releases: HashMap<String, Option<LatestRelease>> = HashMap::new();
        let mut tools = Vec::new();
        for t in collected.tools {
            releases.insert(t.tool.id.clone(), t.release.map(|r| self.to_latest(r)));
            tools.push(t.tool);
        }
        // Tools whose lookup failed this time keep what we knew about them.
        for entry in &self.lock().entries {
            if !entry.tool.discovered && !tools.iter().any(|t| t.id == entry.tool.id) {
                tools.push(entry.tool.clone());
            }
        }

        // Reconcile what's on disk: drop tools deleted by hand, pick up manual installs/updates.
        let installed = self.lock().state.installed.clone();
        let mut on_disk: HashMap<String, Option<InstallRecord>> = HashMap::new();
        for tool in &tools {
            let current = match installed.get(&tool.id) {
                Some(record) if record.path.exists() => {
                    let mut record = record.clone();
                    if let Some(v) = self.inner.installer.installed_version(&record.path) {
                        record.version = v;
                    }
                    Some(record)
                }
                _ => self.inner.installer.find_existing(tool).map(|(path, version)| InstallRecord {
                    version,
                    path,
                    installed_at: now(),
                    managed: false,
                }),
            };
            on_disk.insert(tool.id.clone(), current);
        }

        let mut m = self.lock();
        let mut old: HashMap<String, ToolEntry> = m.entries.drain(..).map(|e| (e.tool.id.clone(), e)).collect();
        for tool in tools {
            let id = tool.id.clone();
            let mut entry = old.remove(&id).unwrap_or_else(|| ToolEntry::new(tool.clone()));
            entry.tool = tool;
            if let Some(latest) = releases.remove(&id) {
                entry.latest = latest;
            }
            // Leave tools that are mid-install alone; the job will write the record.
            if entry.job.is_none() {
                match on_disk.remove(&id).flatten() {
                    Some(record) => {
                        m.state.installed.insert(id.clone(), record.clone());
                        entry.installed = Some(record);
                    }
                    None => {
                        m.state.installed.remove(&id);
                        entry.installed = None;
                    }
                }
            }
            m.entries.push(entry);
        }
        if !collected.categories.is_empty() {
            m.categories = collected.categories;
        }
        if first_error.is_none() {
            m.state.last_check = Some(now());
        }
        self.save(&m);
        drop(m);
        client.save_cache();
        first_error.map_or(Ok(()), Err)
    }

    fn to_latest(&self, release: Release) -> LatestRelease {
        let assets = release.assets();
        LatestRelease {
            version: version::parse_tag(&release.tag_name).map_or(release.tag_name.clone(), |v| v.to_string()),
            package: self.inner.platform.and_then(|p| assets::select(&assets, p)).map(|(a, k)| (a.clone(), k)),
            checksums: assets::checksum_file(&assets).cloned(),
            html_url: release.html_url,
            published_at: release.published_at,
            notes: release.body.unwrap_or_default(),
        }
    }

    /// Installs automatic updates and announces new versions according to each tool's policy.
    pub fn apply_policies(&self) {
        let mut to_install = Vec::new();
        let mut to_announce = Vec::new();
        {
            let mut m = self.lock();
            let settings = m.state.settings.clone();
            for entry in m.entries.iter().filter(|e| e.update_available() && e.job.is_none()) {
                let version = entry.latest.as_ref().map(|l| l.version.clone()).unwrap_or_default();
                match settings.policy_for(&entry.tool.id) {
                    UpdatePolicy::Auto => {
                        let path = entry.installed.as_ref().map(|i| i.path.clone()).unwrap_or_default();
                        to_install.push((entry.tool.id.clone(), path));
                    }
                    UpdatePolicy::Notify => {
                        if m.state.notified.get(&entry.tool.id) != Some(&version) {
                            to_announce.push((entry.tool.id.clone(), entry.tool.name.clone(), version));
                        }
                    }
                    UpdatePolicy::Off => {}
                }
            }
            for (id, _, version) in &to_announce {
                m.state.notified.insert(id.clone(), version.clone());
            }
            if !to_announce.is_empty() {
                self.save(&m);
            }
        }
        for (_, tool, version) in to_announce {
            self.emit(Event::UpdateAvailable { tool, version });
        }
        for (id, path) in to_install {
            // A running app is retried on the next scheduler tick.
            if !install::is_running(&path) {
                self.install(&id);
            }
        }
    }

    /// Installs or updates a tool in the background.
    pub fn install(&self, id: &str) {
        let (tool, latest, previous, cancel) = {
            let mut m = self.lock();
            let cancel = Arc::new(AtomicBool::new(false));
            let Some(entry) = m.entry(id) else { return };
            if entry.job.is_some() {
                return;
            }
            let Some(latest) = entry.latest.clone().filter(|l| l.package.is_some()) else { return };
            let total = latest.package.as_ref().map(|(a, _)| a.size);
            entry.job = Some(Job::Downloading { done: 0, total });
            entry.error = None;
            let job = (entry.tool.clone(), latest, entry.installed.clone(), cancel.clone());
            m.cancels.insert(id.to_owned(), cancel);
            job
        };
        self.changed();

        let this = self.clone();
        thread::spawn(move || {
            let result = this.run_install(&tool, &latest, previous.as_ref(), &cancel);
            let mut m = this.lock();
            m.cancels.remove(&tool.id);
            let event = match result {
                Ok(record) => {
                    m.state.notified.remove(&tool.id);
                    m.state.installed.insert(tool.id.clone(), record.clone());
                    if let Some(entry) = m.entry(&tool.id) {
                        entry.installed = Some(record);
                    }
                    Some(Event::Installed {
                        tool: tool.name.clone(),
                        version: latest.version.clone(),
                        updated: previous.is_some(),
                    })
                }
                Err(Error::Cancelled) => None,
                Err(e) => {
                    log::error!("installing {} failed: {e}", tool.id);
                    if let Some(entry) = m.entry(&tool.id) {
                        entry.error = Some(e.to_string());
                    }
                    Some(Event::Failed { tool: tool.name.clone(), error: e.to_string() })
                }
            };
            if let Some(entry) = m.entry(&tool.id) {
                entry.job = None;
            }
            this.save(&m);
            drop(m);
            this.changed();
            if let Some(event) = event {
                this.emit(event);
            }
        });
    }

    fn set_job(&self, id: &str, job: Job) {
        if let Some(entry) = self.lock().entry(id) {
            entry.job = Some(job);
        }
        self.changed();
    }

    fn run_install(
        &self,
        tool: &Tool,
        latest: &LatestRelease,
        previous: Option<&InstallRecord>,
        cancel: &AtomicBool,
    ) -> Result<InstallRecord> {
        let (asset, kind) = latest.package.clone().ok_or_else(|| Error::Install("no build for this system".into()))?;
        if previous.is_some_and(|p| install::is_running(&p.path)) {
            return Err(Error::Install(format!("Quit {} to update it", tool.name)));
        }
        let client = &self.inner.client;
        let expected = match &latest.checksums {
            Some(sums) => {
                let found = assets::find_checksum(&client.fetch_text(&sums.url)?, &asset.name);
                if found.is_none() {
                    log::warn!("{} has no checksum entry for {}", tool.id, asset.name);
                }
                found
            }
            None => None,
        };

        let file = self.inner.paths.downloads_dir.join(&asset.name);
        download::download(client.agent(), &asset.url, &file, expected.as_deref(), cancel, |done, total| {
            self.set_job(&tool.id, Job::Downloading { done, total });
        })?;
        self.set_job(&tool.id, Job::Installing);

        let icon = if cfg!(target_os = "linux") { client.fetch_bytes(&tool.icon_url()).ok() } else { None };
        let previous_path = previous.map(|p| p.path.as_path());
        let result = self.inner.installer.install(tool, &file, kind, previous_path, icon.as_deref());
        let _ = fs::remove_file(&file);
        let path = result?;
        Ok(InstallRecord { version: latest.version.clone(), path, installed_at: now(), managed: true })
    }

    pub fn update_all(&self) {
        let ids: Vec<String> =
            self.lock().entries.iter().filter(|e| e.update_available()).map(|e| e.tool.id.clone()).collect();
        for id in ids {
            self.install(&id);
        }
    }

    pub fn cancel(&self, id: &str) {
        if let Some(flag) = self.lock().cancels.get(id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn uninstall(&self, id: &str) {
        let (tool, record) = {
            let mut m = self.lock();
            let Some(entry) = m.entry(id) else { return };
            let Some(record) = entry.installed.clone() else { return };
            if entry.job.is_some() {
                return;
            }
            entry.job = Some(Job::Removing);
            entry.error = None;
            (entry.tool.clone(), record)
        };
        self.changed();
        let this = self.clone();
        thread::spawn(move || {
            let result = if install::is_running(&record.path) {
                Err(Error::Install(format!("Quit {} before uninstalling it", tool.name)))
            } else {
                this.inner.installer.uninstall(&tool, &record.path)
            };
            let mut m = this.lock();
            let ok = result.is_ok();
            if ok {
                m.state.installed.remove(&tool.id);
            }
            if let Some(entry) = m.entry(&tool.id) {
                entry.job = None;
                match result {
                    Ok(()) => entry.installed = None,
                    Err(e) => entry.error = Some(e.to_string()),
                }
            }
            this.save(&m);
            drop(m);
            this.changed();
        });
    }

    pub fn launch(&self, id: &str) {
        let path = self.lock().entry(id).and_then(|e| e.installed.as_ref().map(|i| i.path.clone()));
        if let Some(Err(e)) = path.map(|p| install::launch(&p)) {
            if let Some(entry) = self.lock().entry(id) {
                entry.error = Some(e.to_string());
            }
            self.changed();
        }
    }

    pub fn dismiss_error(&self, id: &str) {
        if let Some(entry) = self.lock().entry(id) {
            entry.error = None;
        }
        self.changed();
    }

    /// `None` resets the tool to the default policy.
    pub fn set_policy(&self, id: &str, policy: Option<UpdatePolicy>) {
        self.update_settings(|s| match policy {
            Some(p) => {
                s.policies.insert(id.to_owned(), p);
            }
            None => {
                s.policies.remove(id);
            }
        });
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        {
            let mut m = self.lock();
            f(&mut m.state.settings);
            m.state.settings.check_interval_hours = m.state.settings.check_interval_hours.clamp(1, 168);
            self.save(&m);
        }
        self.changed();
    }

    /// Checks right away, then again whenever the configured interval has passed.
    pub fn start(&self) {
        self.refresh();
        let this = self.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(60));
                let due = {
                    let m = this.lock();
                    let interval = u64::from(m.state.settings.check_interval_hours) * 3600;
                    // Failed checks retry after a while instead of every tick.
                    let retry_ok = now().saturating_sub(m.last_attempt) >= RETRY_AFTER_SECS.min(interval);
                    retry_ok && m.state.last_check.is_none_or(|t| now().saturating_sub(t) >= interval)
                };
                if due {
                    this.refresh();
                } else {
                    this.apply_policies();
                }
            }
        });
    }
}
