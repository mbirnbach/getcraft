//! Builds the release index that launchers download instead of querying the GitHub API.
//!
//!     GETCRAFT_GITHUB_TOKEN=... getcraft-index [catalog.toml] > index.json
//!
//! Exits non-zero (and prints nothing) if any lookup fails, so a partial index is never published.

use getcraft_core::catalog::Catalog;
use getcraft_core::engine::now;
use getcraft_core::github::Client;
use getcraft_core::index::{self, Index};
use std::process::ExitCode;

fn main() -> ExitCode {
    let catalog = match std::env::args().nth(1) {
        Some(path) => match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Catalog::parse(&t).map_err(|e| e.to_string()))
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cannot read {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => Catalog::bundled(),
    };

    let collected = index::collect(&Client::new(None), catalog);
    if let Some(e) = &collected.error {
        eprintln!("index not written: {e}");
        return ExitCode::FAILURE;
    }
    for t in &collected.tools {
        let tag = t.release.as_ref().map_or("(no release)", |r| r.tag_name.as_str());
        eprintln!("{:<14} {tag}{}", t.tool.id, if t.tool.discovered { "  [discovered]" } else { "" });
    }
    let index = Index::from_collected(collected, now());
    println!("{}", serde_json::to_string(&index).expect("index serializes"));
    ExitCode::SUCCESS
}
