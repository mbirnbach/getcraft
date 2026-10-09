# AGENTS.md

Notes for AI coding agents (and humans) working on GetCraft: what the project is, how it's put
together, how it's built, tested and released, and the non-obvious things that were learned the
hard way. Read this before changing anything; keep it accurate when you change something it
describes.

## What GetCraft is

An independent, open-source (MIT OR Apache-2.0) launcher that installs and updates the
**Crafting Apps** (PhotoCraft, VectorCraft, FilmCraft, … by the ArtCraft team, GitHub org
`storytold`) for non-technical users, in the style of an app store / Creative Cloud. It downloads
the apps' official GitHub release files unchanged and verifies them.

- **Not affiliated with ArtCraft.** The disclaimer must stay visible (README, footer in the app,
  NOTICE). Never use the ArtCraft wordmark or logo; mention ArtCraft only in plain text to identify
  the apps. Their app icons are bundled under their MIT license (notices in ATTRIBUTION.md and
  NOTICE, shipped in every package).
- **Scope:** the `*craft` apps. ArtCraft itself and ArtCraftX are deliberately out of scope.
- **Maintainer:** Marvin Birnbach (GitHub `mbirnbach`), the only person with write access. Not a
  Rust expert; expects the agent to act as lead developer, make technical decisions, verify things
  for real (run it, test it, look at it) and say plainly what was and wasn't tested.
- **Repository:** <https://github.com/mbirnbach/getcraft> (public). Other community launchers exist
  (e.g. CryptoKey98/craft-apps-updater); GetCraft coexists with them.

## Layout

```
apps/getcraft/            desktop app (eframe/egui 0.36): UI, tray, single instance, diagnostics
  src/main.rs             flags, logging, renderer selection + crash guard, window icon
  src/app.rs              all UI (pages, cards, dialogs, toasts), first-run login prompt
  src/background.rs       tray icon, launch at login (auto-launch), macOS Dock/reopen handling
  src/diagnostics.rs      getcraft.log, panic hook, native error dialogs
  src/instance.rs         single instance via a localhost port file
  src/notify.rs           desktop notifications (notify-rust)
  src/icons.rs, theme.rs  bundled app icons, colours/styles
  examples/make_icon.rs   builds every icon format from assets/getcraft-source.png
apps/getcraft-index/      CLI that builds index.json (run by CI)
crates/getcraft-core/     everything UI-independent
  src/engine.rs           the update engine: checks, policies, installs, self-update
  src/trust.rs            what may be installed (publisher allowlist, IDs, URLs, Apple identities)
  src/index.rs            release index (index.json) and direct GitHub collection fallback
  src/github.rs           GitHub client with ETag cache and response size caps
  src/assets.rs           picks the right release file per OS/arch; checksum parsing
  src/download.rs         streaming download with exact size + SHA-256 check
  src/selfupdate.rs       updating GetCraft itself (minisign + platform specifics)
  src/install/            per-OS installers (macos.rs, windows.rs, linux.rs) + archive.rs (safe unzip)
  src/catalog.rs, state.rs, version.rs, platform.rs
  examples/smoke.rs       live end-to-end install into a temp folder
  examples/msi_roundtrip.rs  Windows: find, update and uninstall an MSI-installed app (CI only)
  examples/rollback.rs    keep a previous version, switch back and forth (real releases, temp folder)
catalog.toml              curated list of apps (names, descriptions, categories); also fetched remotely
keys/update-signing.pub   minisign public key compiled into GetCraft (key ID B92D9E0E21D756AC)
packaging/windows/getcraft.iss   Inno Setup installer script
scripts/                  bundle-macos.sh, build-dmg.sh + dmg-settings.py, Windows test scripts,
                          set-apple-secrets.sh, setup-update-signing.sh
assets/                   GetCraft artwork (all rights reserved, see assets/LICENSE-icon.txt),
                          assets/icons/ (Crafting App icons, MIT), assets/dmg/ (DMG background)
.signpath/                SignPath artifact configuration (Windows signing, currently inactive)
docs/                     SIGNING.md (signing + release how-to), VERIFY.md (how users verify downloads)
```

Rust 2024 edition, workspace version in the root `Cargo.toml` (shared by all crates). Formatting:
`rustfmt.toml` (max width 120). Lints: clippy with `-D warnings` in CI.

## How it works

- **Release index.** The *Release index* workflow (`index.yml`, every 30 min) runs
  `getcraft-index` with a GitHub token and publishes `index.json` (all tools, their latest
  releases, and GetCraft's own latest release) to the `index` branch. Launchers fetch
  `https://raw.githubusercontent.com/mbirnbach/getcraft/index/index.json` (one request, no API rate
  limit). They fall back to the GitHub API (60 unauthenticated requests/hour) only if the index is
  missing or older than 6 hours. The raw CDN caches for a few minutes.
- **Discovery.** New `*craft` repos of trusted owners appear automatically ("New on GitHub") once
  they have a release; `catalog.toml` gives them proper names and categories.
- **Trust (`trust.rs`), decided in code, not by remote data:**
  - publishers: only GitHub owner `storytold`; macOS team `DJ6XS33FX8`; bundle IDs
    `ai.storyteller.<id>`
  - GetCraft itself: repo `mbirnbach/getcraft`, team `J829HHBMPW`, bundle ID `net.brnbch.getcraft`
  - tool IDs `[a-z0-9-]` (they become folder names), names validated, URLs must be release assets
    of the tool's own repo on github.com
- **Downloads:** SHA-256 checksum (from the release's `SHA256SUMS.txt`) is mandatory and the size
  must match the release metadata exactly (cap 2 GiB). Every Crafting App publishes checksums.
- **Installers (no admin rights, except when the user updates or removes an MSI copy):**
  - macOS: mount the DMG, copy the single `.app` to `/Applications` (or `~/Applications` if not
    writable), verify `codesign` against the publisher's team + bundle ID, then swap into place.
    Never overwrites an unrelated app with the same name. Hand-installed copies are detected
    (by bundle ID) and adopted. Uninstall moves to the Trash and checks the bundle ID first.
  - Windows: the **portable zip** (not the apps' MSIs), unpacked with limits into
    `%LOCALAPPDATA%\Programs\GetCraft\<id>`, program must be `<id>.exe`, plus a Start menu
    shortcut. Copies installed with the apps' own MSI (per-machine, `Program Files\<Name>`) are
    found through HKLM `App Paths\<id>.exe` plus the uninstall entry (publisher
    "Learning Machines LLC", name, version; both registry views) and recorded with `msi: true`.
    They're updated by running the new checksum-verified `.msi` (`msiexec /i … /passive`, Windows
    asks for admin rights; the desktop-shortcut choice is kept) and removed with `msiexec /x
    {product code}`, only when the user clicks: "Update automatically" falls back to notifying for
    them. msiexec logs go to the work folder.
  - Linux: the AppImage into `~/.local/share/getcraft/apps/<id>` plus an escaped `.desktop` entry.
  - Apps are never updated while running (`install::is_running`).
  - **Previous versions** (setting `keep_previous`, off by default): an update moves the
    replaced bundle/folder to `Paths::previous_dir/<id>/` (`~/Library/Application Support/GetCraft/
    Previous.noindex` on macOS so Spotlight skips it, the local data folder elsewhere; never the
    roaming `%APPDATA%`). One kept copy per app; the one kept before is only dropped once the
    update worked, and a failed update restores both. Recorded as `InstallRecord::previous`.
    "Switch back" swaps the two (so it can be undone), re-checks the kept copy first (macOS:
    `codesign` identity again; elsewhere `<id>.exe`/`<id>.AppImage` present) and sets
    `rolled_back_from`: that version still shows as an update, but update policies skip it (no
    automatic install, no notification); later releases are handled as usual. MSI copies never
    keep one (Windows Installer refuses downgrades). Turning the setting off or uninstalling
    deletes the kept copies. Cross-volume moves fall back to copying (`ditto` on macOS).
- **Update policies per app:** notify (default), automatic, or off. Checks at start and every
  6 hours (configurable). Desktop notifications for: update available, app updated, automatic
  update failed, GetCraft updated, GetCraft self-update failed (once per version).
- **Self-update:** GetCraft downloads its own new release, requires the checksum, a minisign
  signature (`<file>.minisig`, trusted comment must contain `file:<asset name>` → no downgrades)
  from the key in `keys/update-signing.pub`, and on macOS its Developer ID (`codesign` requirement
  with team `J829HHBMPW`). Hidden → installs and restarts silently (`--after-update --background`);
  visible → "Restart now" banner. Dev builds (anything under a `target/` dir) never self-update.
- **Background:** closing the window hides it to the menu bar / tray; the engine keeps running.
  Launch at login is **opt-in** (asked once on first run): LaunchAgent `net.brnbch.getcraft` on
  macOS, `HKCU\…\Run` value `GetCraft` on Windows, XDG autostart on Linux. Second launches hand
  over to the running instance (localhost port in `instance.port`, mode 0600).
- **Rendering:** wgpu first, OpenGL (glow) fallback. On Windows wgpu may only use DirectX 12 and
  OpenGL, **never Vulkan**. Crash guard: `renderer-starting` note in the data folder; if present at
  launch, the previous start crashed in the driver and GetCraft writes `renderer` = `glow` for good.
  `GETCRAFT_RENDERER=glow|wgpu` overrides.
- **Diagnostics:** every run logs to `getcraft.log` (+ `getcraft.old.log`) in the data folder;
  startup failures and main-thread panics show a native error dialog. `--smoke-test` starts
  window, renderer and tray, draws 30 frames and exits 0 (no network).

Data folders: `~/Library/Application Support/GetCraft` + `~/Library/Caches/GetCraft` (macOS),
`%APPDATA%\GetCraft` + `%LOCALAPPDATA%\GetCraft` (Windows), `~/.config/GetCraft` +
`~/.cache/GetCraft` (Linux). `state.json` holds settings and installed apps.

Environment variables: `GETCRAFT_INDEX_URL` (other index), `GETCRAFT_GITHUB_TOKEN` (API token for
dev), `GETCRAFT_TREAT_AS_INSTALLED=1` (lets a dev build register the login item),
`GETCRAFT_RENDERER`, `WGPU_BACKEND`, `RUST_LOG`.

## Build, run, test

```bash
source ~/.cargo/env                    # rustup is installed per user on the maintainer's Mac
cargo run -p getcraft                  # run the app (debug)
cargo test --workspace                 # unit tests (trust, archive, assets, selfupdate, …)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p getcraft-core --example smoke pdfcraft   # real install into a TEMP folder
cargo run -p getcraft-core --example rollback         # keep/switch previous versions, TEMP folder
./target/release/getcraft --smoke-test                # startup test
scripts/bundle-macos.sh debug          # target/bundle/GetCraft.app (ad-hoc sign it to run:
                                       # codesign -s - --force target/bundle/GetCraft.app)
scripts/build-dmg.sh target/bundle/GetCraft.app out.dmg   # needs `pip install dmgbuild`
cargo run --release -p getcraft --example make_icon       # regenerate icons from the source art
```

**Safety rule for tests:** never let a test install into, scan or modify the real `/Applications`
or the maintainer's installed apps. Use `Installer::with_apps_dir(tempdir, …)`, which also turns
off scanning of system folders. (A test once trashed the maintainer's real PdfCraft.app before
this rule existed.) Updates replace the existing copy in place and never remove other copies.

The maintainer has GetCraft and several Crafting Apps installed on the Mac; launching a dev build
hands off to the installed GetCraft if it's running (single instance), so stop or account for it.

## CI, branch protection and releases

- **Workflows:** `ci.yml` (fmt, clippy, tests on macOS/Windows/Linux; smoke tests on Windows x64,
  Windows ARM64 `windows-11-arm`, macOS, Linux under Xvfb), `security.yml` (`cargo-deny`
  advisories on every change and weekly; CodeQL for Rust and Actions), `index.yml` (release
  index), `msi.yml` (Windows MSI round trip with a real PhotoCraft MSI, on installer changes,
  not required), `rollback.yml` (the `rollback` example on all three systems, same triggers, not
  required), `release.yml` (on `v*` tags; manual runs are dry runs that publish nothing).
- **All actions are pinned to commit SHAs** (comment shows the version); Dependabot updates them
  weekly. `appimagetool` 1.9.1 and the AppImage runtime 20251108 are pinned with SHA-256 hashes.
- **Rulesets:** `main` requires a PR, squash merge, resolved threads and 8 checks
  (`check (…)` ×3, `advisories`, `smoke (…)` ×4); the admin can push directly. Release tags `v*`
  can only be created/changed by the admin. `index` can't be deleted (personal repos can't exempt
  GitHub Actions from push rules, so pushes to it aren't restricted).
- **Releasing:** bump `version` in the root `Cargo.toml` (in a PR), merge, then
  `git tag -a vX.Y.Z -m "GetCraft X.Y.Z" && git push origin vX.Y.Z`. The workflow refuses tags that
  don't match `Cargo.toml`, runs the Windows dependency check, smoke test and installer round trip
  (install → start → uninstall), signs and notarizes macOS, minisign-signs every file and
  `SHA256SUMS.txt`, creates build provenance attestations, and publishes. After publishing, run
  the index workflow (`gh workflow run index.yml`) so installed copies see the release right away.
- **Pre-release rule:** without a notarized macOS build a release is published as a pre-release,
  which the self-updater ignores. The release fails if the update signing key is missing (a build
  without the public key could never verify later updates).
- **Secrets:** `APPLE_CERTIFICATE_P12`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_API_KEY`,
  `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID` (macOS, notarization via App Store Connect API key),
  `MINISIGN_SECRET_KEY` (update signing; the maintainer keeps a backup). SignPath secrets aren't
  set: SignPath Foundation declined the project (October 2026), so **Windows builds are unsigned**.
  Never handle secret values yourself; the maintainer runs `scripts/set-apple-secrets.sh` and
  `scripts/setup-update-signing.sh`.
- **Release artifacts:** `getcraft-<v>-macos-universal.dmg` (styled with dmgbuild, volume icon),
  `getcraft-<v>-windows-setup.exe` (Inno Setup, x64 + ARM64 in one, per user, AppId
  `{7CF358B6-E104-44DA-AC2E-E883DB1AA9F1}`, never change it), `getcraft-<v>-windows-<x64|arm64>-portable.zip`,
  `getcraft-<v>-linux-<x86_64|aarch64>.AppImage`, each with `.minisig`, plus `SHA256SUMS.txt`.
- Work on a branch and open a PR; the app/CI then shows the checks. `main` requires branches to
  be up to date: when two PRs are queued, the second goes "BEHIND" after the first merges and
  auto-merge won't move it. Run `gh pr update-branch <n>` and it merges after the re-run. Commit messages and PRs end
  with the attribution trailer the session tells you to use.

## Things learned the hard way

- **Never add files to a macOS `.app`**, not even in its root: codesign then reports "unsealed
  contents" and the signature check fails. Tests tell bundles apart by their version instead.
- **winit owns the macOS app delegate** and aborts if it's replaced. Reopen handling (Dock click
  → show window) adds one method to winit's delegate class at runtime (`class_addMethod` in
  `background.rs`).
- **eframe sets the Dock icon at runtime** from the window icon. Inside the `.app`, pass an empty
  `IconData` so the bundle's `.icns` (with Apple's margins, tile = 80.47 % of the canvas) is used;
  otherwise the Dock icon is too big.
- **notify-rust on macOS** must call `set_application(<bundle id>)` before the first notification,
  or macOS shows a "Where is use_default?" dialog.
- **Windows:** link the C runtime statically (`.cargo/config.toml`); release builds have no
  console. Intel's Vulkan driver (`igvk64.dll` 30.0.101.1692, UHD 620, ThinkPad T490) crashed the
  process, which is why Vulkan is excluded. Notifications use AppUserModelID
  `net.brnbch.getcraft` only when the installer's Start menu shortcut exists (portable copies
  show as "Windows PowerShell").
- **Upstream Crafting App bundles fail `codesign --strict`** (Finder metadata in the DMG); verify
  without `--strict`. GetCraft's own bundle passes `--strict`.
- **Crafting App Windows zips** contain one folder `<id>-<v>-windows-<arch>-portable/` with
  `<id>.exe` and `<id>-cli.exe`. macOS DMGs contain `<Name>.app` (e.g. `CADCraft.app`, so catalog
  names must match the bundle names exactly).
- **Linux CI** needs `libxkbcommon-x11-0` at runtime (winit), `xvfb` and Mesa for the smoke test.
- **egui 0.36 API:** `eframe::App::ui(&mut self, ui, frame)` plus `logic()` (runs while hidden);
  panels are `egui::Panel::top/left/right/bottom(id).show(ui, …)`. Many emoji/arrow glyphs aren't
  in the default fonts; icons like the nav grid are painted by hand.
- **Known dependency notices:** `deny.toml` ignores two "unmaintained" advisories deep in the GUI
  stack (proc-macro-error via gtk 0.18, ttf-parser via winit). A glib 0.18 Dependabot alert
  (VariantStrIter, unused) was dismissed as tolerable risk until tray-icon moves to newer gtk-rs.
- **Measured footprint (macOS, 0.1.3):** 0 % CPU idle, ~108 MB memory whether hidden or not
  (~49 MB are window surfaces). A lighter background mode is issue #7.
- **Maintainer's machine quirks:** macOS with zsh (unquoted `$var` isn't word-split; `/bin/bash`
  is 3.2 without `mapfile`); the sandbox can't read `~/.Trash` or capture windows with
  `screencapture`, so use the computer-use tools for screenshots; German system locale.
- `SECURITY_AUDIT.md` (a private audit) is intentionally kept out of the repo
  (`.git/info/exclude`). Don't commit it.

## Open items

- [#7](https://github.com/mbirnbach/getcraft/issues/7): use less memory in the background (true
  hidden mode without a window).
- Windows code signing (SignPath declined; the pipeline in `release.yml` and `.signpath/` is ready
  if that changes).
- A monochrome menu-bar (template) icon for macOS would look more native than the colour icon.
