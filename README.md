# GetCraft

**One place to install and update the open-source Crafting Apps** (PhotoCraft, VectorCraft,
FilmCraft, LightCraft, PdfCraft and friends). No GitHub, no hunting for the right download, no
manual updates.

> GetCraft is an independent, community-made project. It is **not affiliated with or endorsed by
> the ArtCraft team**. All apps are downloaded straight from their official GitHub releases.

## What it does

- Lists every Crafting App with its icon, description and latest version.
- Installs the right build for your computer (macOS universal, Windows x64/ARM64/x86, Linux
  x86_64/ARM64) with one click, without needing admin rights, and checks each download against
  the project's published SHA-256 checksums.
- Finds apps you already installed by hand and keeps them up to date too.
- Checks for updates regularly and, per app, either **notifies you** or **updates automatically**.
  Apps are never replaced while they're open.
- Picks up new `*craft` apps from the `storytold` GitHub organisation automatically.
- Keeps running in the menu bar / system tray when its window is closed, starts at login (hidden),
  and updates itself.

## How it works

```
apps/getcraft          the desktop app (Rust + egui, like the Crafting Apps themselves)
apps/getcraft-index    builds index.json, run by CI every 30 minutes
crates/getcraft-core   catalog, GitHub client, asset matching, installers, update engine
catalog.toml           curated tool list: names, descriptions, categories
```

The GitHub API allows only 60 unauthenticated requests per hour per IP. So instead of every
launcher querying every repository, the `Release index` workflow builds one `index.json` and
publishes it to the `index` branch. Launchers fetch that single file and query GitHub directly
only if it's unavailable or stale.

To add or describe a tool, edit `catalog.toml`. The change reaches every launcher through the
index; no new GetCraft release is needed.

Installed apps go to:

| OS      | Location                                                      | Menu entry          |
|---------|---------------------------------------------------------------|---------------------|
| macOS   | `/Applications` (or `~/Applications` without admin rights)    | Launchpad/Spotlight |
| Windows | `%LOCALAPPDATA%\Programs\GetCraft\<app>` (portable build)     | Start Menu          |
| Linux   | `~/.local/share/getcraft/apps/<app>` (AppImage)               | `.desktop` entry    |

## Development

```bash
cargo run -p getcraft                                 # the app
cargo test --workspace                                # unit tests
cargo run -p getcraft-core --example smoke pdfcraft   # live end-to-end install into a temp dir
scripts/bundle-macos.sh debug                         # build target/bundle/GetCraft.app
cargo run --release -p getcraft --example render_icon # regenerate the app icons in assets/
```

Releases are built by pushing a `v*` tag (see `.github/workflows/release.yml`). Until the signing
secrets are configured, releases are ad-hoc signed and published as pre-releases, which the
self-updater ignores.

Useful environment variables:

- `GETCRAFT_INDEX_URL` points at a different `index.json` (e.g. a local one).
- `GETCRAFT_GITHUB_TOKEN` is used for direct API calls, which lifts the rate limit while developing.
- `GETCRAFT_TREAT_AS_INSTALLED=1` lets a development build register the login item.
- `RUST_LOG=debug` turns on verbose logging.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), like the Crafting Apps.
App names and icons belong to their respective owners and are used only to identify the apps.
