# Contributing to GetCraft

Thanks for helping! GetCraft is small and young, so bug reports, ideas and pull requests all make
a real difference.

## What belongs where

- **GetCraft** (finding, installing, updating, the launcher itself): issues and pull requests
  here.
- **A Crafting App itself** (PhotoCraft crashes, a missing feature in FilmCraft, …): the app's own
  repository under [github.com/storytold](https://github.com/storytold). GetCraft only installs
  the apps; it doesn't build or change them.
- **Security problems:** please report them privately, see [SECURITY.md](SECURITY.md).

## Adding or describing an app

The list of apps lives in [`catalog.toml`](catalog.toml). New `*craft` apps from the ArtCraft
team show up automatically, but a catalog entry gives them a proper name, description and
category. Changes reach every user through the release index, without a new GetCraft release.

GetCraft only installs from publishers listed in
[`crates/getcraft-core/src/trust.rs`](crates/getcraft-core/src/trust.rs). Adding a publisher is
a security decision and needs a good reason in the pull request.

## Building and testing

You need [Rust](https://rustup.rs) (stable). On Linux, also the GUI libraries listed in
[`ci.yml`](.github/workflows/ci.yml).

```bash
cargo run -p getcraft            # run the app
cargo test --workspace           # unit tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

`cargo run -p getcraft-core --example smoke pdfcraft` installs a real app into a temporary
folder, which is handy for checking installer changes without touching your own apps. More
options are in the [README](README.md#development).

## Pull requests

- Keep each pull request to one change, and describe what it fixes or adds and how you tested it.
- CI (formatting, clippy, tests on macOS, Windows and Linux) and the security checks (dependency
  advisories, CodeQL) must pass before merging. Pull requests are squash-merged.
- Code that downloads, unpacks, installs or launches anything gets extra scrutiny: say what could
  go wrong and how your change prevents it, and add tests.
- Match the style around you: small functions, comments that explain *why*, no `unsafe` unless
  there's no other way (and then explained).

By contributing you agree that your contribution is licensed under the project's
[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) license, at the user's option. The GetCraft
artwork is not open source (see [`assets/LICENSE-icon.txt`](assets/LICENSE-icon.txt)), so please
don't send changes to it.

Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
