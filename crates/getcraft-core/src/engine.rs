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
use crate::selfupdate::Location;
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
    /// Detached minisign signature of `package` (`<file>.minisig`); GetCraft's own releases have one.
    pub signature: Option<Asset>,
    /// The Windows Installer package, for updating copies installed with one.
    pub msi: Option<Asset>,
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
            (Some(i), Some(l)) => {
                let installable = if i.msi { l.msi.is_some() } else { l.package.is_some() };
                installable && version::is_newer(&l.version, &i.version)
            }
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
    /// A newer GetCraft, if one was released and this install can update itself.
    pub launcher_update: Option<LauncherUpdate>,
    /// True while any app is downloading, installing or being removed.
    pub busy: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LauncherUpdate {
    pub version: String,
    pub html_url: String,
    /// Downloaded and verified; [`Engine::apply_launcher_update`] can install it.
    pub ready: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Event {
    UpdateAvailable {
        tool: String,
        version: String,
    },
    Installed {
        tool: String,
        version: String,
        updated: bool,
    },
    Failed {
        tool: String,
        error: String,
        /// The install was started by an update policy, not by the user in the window.
        automatic: bool,
    },
    /// A GetCraft update has been downloaded and can be applied with a restart.
    LauncherReady {
        version: String,
    },
    /// Downloading or verifying a GetCraft update failed (reported once per version).
    LauncherFailed {
        version: String,
        error: String,
    },
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
    launcher: Option<LauncherUpdate>,
    /// The downloaded GetCraft update, once `launcher` is ready.
    launcher_package: Option<PathBuf>,
    /// The GetCraft version whose failed update we already reported, so retries stay quiet.
    launcher_failure_reported: Option<String>,
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
    /// Where this GetCraft is installed; `None` disables self-updates (e.g. dev builds).
    self_location: Option<Location>,
    on_change: Callback<()>,
    on_event: Callback<Event>,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

/// Drops tools whose id, name or repository isn't acceptable, and strips control characters from
/// the free text.
fn sanitize_tool(mut tool: Tool) -> Option<Tool> {
    use crate::trust::{clean_text, publisher_for, valid_id, valid_name};
    if !valid_id(&tool.id) || !valid_name(&tool.name) || publisher_for(&tool.repo).is_none() {
        log::warn!("ignoring untrusted or malformed tool entry {:?} ({})", tool.id, tool.repo);
        return None;
    }
    tool.kind = clean_text(&tool.kind);
    tool.description = clean_text(&tool.description);
    tool.category = clean_text(&tool.category);
    Some(tool)
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Engine {
    pub fn new(
        paths: Paths,
        installer: Installer,
        self_location: Option<Location>,
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
            launcher: None,
            launcher_package: None,
            launcher_failure_reported: None,
        };
        if let Some(location) = &self_location {
            location.clean_up();
        }
        Self {
            inner: Arc::new(Inner {
                model: Mutex::new(model),
                client: Client::new(Some(paths.http_cache_file.clone())),
                installer,
                paths,
                platform: Platform::current(),
                self_location,
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
            launcher_update: m.launcher.clone(),
            busy: m.entries.iter().any(|e| e.job.is_some()),
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
            Ok(index) => {
                Collected { categories: index.categories, tools: index.tools, launcher: index.launcher, error: None }
            }
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
        if let Some(release) = collected.launcher {
            self.consider_launcher_release(self.to_latest(release, crate::trust::GETCRAFT_REPO));
        }
        let mut releases: HashMap<String, Option<LatestRelease>> = HashMap::new();
        let mut tools = Vec::new();
        for t in collected.tools {
            // Remote data describes tools but can't widen what's trusted (see `trust`).
            let Some(tool) = sanitize_tool(t.tool) else { continue };
            releases.insert(tool.id.clone(), t.release.map(|r| self.to_latest(r, &tool.repo)));
            tools.push(tool);
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
                    if let Some(v) = self.inner.installer.installed_version(tool, &record) {
                        record.version = v;
                    }
                    Some(record)
                }
                _ => self.inner.installer.find_existing(tool).map(|found| InstallRecord {
                    version: found.version,
                    path: found.path,
                    installed_at: now(),
                    managed: false,
                    msi: found.msi,
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

    /// Only assets hosted in `repo`'s own GitHub releases are considered.
    fn to_latest(&self, release: Release, repo: &str) -> LatestRelease {
        let assets: Vec<Asset> =
            release.assets().into_iter().filter(|a| crate::trust::is_release_asset_of(&a.url, repo)).collect();
        let package = self.inner.platform.and_then(|p| assets::select(&assets, p)).map(|(a, k)| (a.clone(), k));
        let signature = package
            .as_ref()
            .and_then(|(p, _)| assets.iter().find(|a| a.name == format!("{}.minisig", p.name)))
            .cloned();
        LatestRelease {
            version: version::parse_tag(&release.tag_name).map_or(release.tag_name.clone(), |v| v.to_string()),
            package,
            checksums: assets::checksum_file(&assets).cloned(),
            signature,
            msi: self.inner.platform.and_then(|p| assets::select_msi(&assets, p)).cloned(),
            html_url: release.html_url,
            published_at: release.published_at,
            notes: release.body.unwrap_or_default(),
        }
    }

    /// Downloads a newer GetCraft in the background so it's ready to apply.
    fn consider_launcher_release(&self, latest: LatestRelease) {
        let Some(location) = &self.inner.self_location else { return };
        if !version::is_newer(&latest.version, env!("CARGO_PKG_VERSION")) {
            return;
        }
        let Some((asset, kind)) = latest.package.clone() else { return };
        if kind != location.package_kind() {
            log::warn!("GetCraft {} has no {:?} build for this install", latest.version, location.package_kind());
            return;
        }
        {
            let mut m = self.lock();
            if m.launcher.as_ref().is_some_and(|l| l.version == latest.version && l.error.is_none()) {
                return;
            }
            m.launcher = Some(LauncherUpdate {
                version: latest.version.clone(),
                html_url: latest.html_url.clone(),
                ready: false,
                error: None,
            });
            m.launcher_package = None;
        }
        self.changed();

        let this = self.clone();
        thread::spawn(move || {
            let client = &this.inner.client;
            let file = this.inner.paths.downloads_dir.join(&asset.name);
            let result = (|| {
                let sums =
                    latest.checksums.as_ref().ok_or_else(|| Error::Install("the update has no checksums".into()))?;
                let expected = assets::find_checksum(&client.fetch_text(&sums.url)?, &asset.name)
                    .ok_or_else(|| Error::Install("the update has no checksum".into()))?;
                let signature = latest
                    .signature
                    .as_ref()
                    .ok_or_else(|| Error::Install("the update isn't signed; not installing it".into()))?;
                let signature = client.fetch_text(&signature.url)?;
                let never = AtomicBool::new(false);
                download::download(client.agent(), &asset.url, &file, &expected, asset.size, &never, |_, _| {})?;
                if let Err(e) = crate::selfupdate::verify_signature(&file, &asset.name, &signature) {
                    let _ = fs::remove_file(&file);
                    return Err(e);
                }
                Ok(())
            })();
            let mut m = this.lock();
            let Some(update) = m.launcher.as_mut().filter(|l| l.version == latest.version) else { return };
            match result {
                Ok(()) => {
                    update.ready = true;
                    m.launcher_package = Some(file);
                    drop(m);
                    log::info!("GetCraft {} is downloaded and ready", latest.version);
                    this.emit(Event::LauncherReady { version: latest.version.clone() });
                }
                Err(e) => {
                    log::warn!("downloading GetCraft {} failed: {e}", latest.version);
                    update.error = Some(e.to_string());
                    let first = m.launcher_failure_reported.as_deref() != Some(latest.version.as_str());
                    m.launcher_failure_reported = Some(latest.version.clone());
                    drop(m);
                    if first {
                        this.emit(Event::LauncherFailed { version: latest.version.clone(), error: e.to_string() });
                    }
                }
            }
            this.changed();
        });
    }

    /// Installs the downloaded GetCraft update over this installation and returns what to
    /// launch. The caller should then call [`crate::selfupdate::relaunch`] and exit.
    pub fn apply_launcher_update(&self) -> Result<PathBuf> {
        let location =
            self.inner.self_location.as_ref().ok_or_else(|| Error::Install("this copy can't update itself".into()))?;
        let package = {
            let m = self.lock();
            if m.entries.iter().any(|e| e.job.is_some()) {
                return Err(Error::Install("wait for app installs to finish first".into()));
            }
            m.launcher_package.clone().ok_or_else(|| Error::Install("no update downloaded".into()))?
        };
        let version = self.lock().launcher.as_ref().map(|l| l.version.clone()).unwrap_or_default();
        let target = location.apply(&package, &self.inner.paths.work_dir, &version)?;
        let _ = fs::remove_file(&package);
        Ok(target)
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
                let policy = match settings.policy_for(&entry.tool.id) {
                    // Updating an MSI copy asks for admin rights, so it only happens on request.
                    UpdatePolicy::Auto if entry.installed.as_ref().is_some_and(|i| i.msi) => UpdatePolicy::Notify,
                    policy => policy,
                };
                match policy {
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
                self.install_as(&id, true);
            }
        }
    }

    /// Installs or updates a tool in the background, at the user's request.
    pub fn install(&self, id: &str) {
        self.install_as(id, false);
    }

    /// `automatic`: started by an update policy rather than the user (affects how failures are
    /// reported).
    fn install_as(&self, id: &str, automatic: bool) {
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
                    Some(Event::Failed { tool: tool.name.clone(), error: e.to_string(), automatic })
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
        // A copy installed with the app's own `.msi` is updated with the new `.msi`.
        let msi = previous.is_some_and(|p| p.msi);
        let (asset, kind) =
            if msi { latest.msi.clone().map(|a| (a, PackageKind::Msi)) } else { latest.package.clone() }
                .ok_or_else(|| Error::Install("no build for this system".into()))?;
        if previous.is_some_and(|p| install::is_running(&p.path)) {
            return Err(Error::Install(format!("Quit {} to update it", tool.name)));
        }
        let client = &self.inner.client;
        // Every Crafting App publishes SHA256SUMS.txt; a release without one isn't installed.
        let sums = latest
            .checksums
            .as_ref()
            .ok_or_else(|| Error::Install(format!("{} {} has no published checksums", tool.name, latest.version)))?;
        let expected = assets::find_checksum(&client.fetch_text(&sums.url)?, &asset.name)
            .ok_or_else(|| Error::Install(format!("{} has no checksum for {}", tool.name, asset.name)))?;

        let file = self.inner.paths.downloads_dir.join(&asset.name);
        download::download(client.agent(), &asset.url, &file, &expected, asset.size, cancel, |done, total| {
            self.set_job(&tool.id, Job::Downloading { done, total });
        })?;
        self.set_job(&tool.id, Job::Installing);

        let icon = if cfg!(target_os = "linux") { client.fetch_bytes(&tool.icon_url()).ok() } else { None };
        let previous_path = previous.map(|p| p.path.as_path());
        let result = self.inner.installer.install(tool, &file, kind, previous_path, icon.as_deref());
        let _ = fs::remove_file(&file);
        let path = result?;
        Ok(InstallRecord { version: latest.version.clone(), path, installed_at: now(), managed: !msi, msi })
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
                this.inner.installer.uninstall(&tool, &record)
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
