use crate::Result;
use serde::{Deserialize, Serialize};

/// The catalog shipped inside the binary, used until (or if) the remote one can be fetched.
pub const BUNDLED: &str = include_str!("../../../catalog.toml");

/// The live catalog. Editing `catalog.toml` on the main branch updates every launcher.
pub const REMOTE_URL: &str = "https://raw.githubusercontent.com/mbirnbach/getcraft/main/catalog.toml";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub discovery: Discovery,
    #[serde(default, rename = "category")]
    pub categories: Vec<Category>,
    #[serde(default, rename = "tool")]
    pub tools: Vec<Tool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Discovery {
    /// GitHub users/organisations scanned for new `*craft` repositories.
    #[serde(default)]
    pub owners: Vec<String>,
    /// Repository names that are never treated as installable tools.
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tool {
    pub id: String,
    pub name: String,
    /// `owner/repo` on GitHub.
    pub repo: String,
    #[serde(default = "other")]
    pub category: String,
    /// Short label such as "Image editor".
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    /// True for tools found by scanning GitHub rather than listed in the catalog.
    #[serde(default)]
    pub discovered: bool,
}

fn other() -> String {
    "other".into()
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Self> {
        let catalog: Catalog = toml::from_str(text)?;
        Ok(catalog)
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED).expect("bundled catalog.toml is valid")
    }

    pub fn is_known(&self, repo_name: &str) -> bool {
        let name = repo_name.to_ascii_lowercase();
        self.discovery.ignore.iter().any(|i| i.eq_ignore_ascii_case(&name))
            || self.tools.iter().any(|t| t.id == name || t.repo_name().eq_ignore_ascii_case(&name))
    }
}

impl Tool {
    pub fn repo_name(&self) -> &str {
        self.repo.rsplit('/').next().unwrap_or(&self.repo)
    }

    pub fn repo_url(&self) -> String {
        format!("https://github.com/{}", self.repo)
    }

    pub fn releases_url(&self) -> String {
        format!("https://github.com/{}/releases", self.repo)
    }

    /// Every Crafting App keeps its icon at the same path, which lets discovered tools get a
    /// real icon too.
    pub fn icon_url(&self) -> String {
        format!(
            "https://raw.githubusercontent.com/{}/HEAD/assets/app-icon/hicolor/256x256/apps/ai.storyteller.{}.png",
            self.repo, self.id
        )
    }

    /// Builds an entry for a repository found by discovery: `gridcraft` becomes "GridCraft".
    pub fn discovered(owner: &str, repo: &str, description: Option<&str>) -> Self {
        let id = repo.to_ascii_lowercase();
        let stem = id.strip_suffix("craft").unwrap_or(&id);
        let mut name: String =
            stem.chars().enumerate().map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c }).collect();
        if id.ends_with("craft") {
            name.push_str("Craft");
        }
        Self {
            name,
            repo: format!("{owner}/{repo}"),
            category: other(),
            kind: "New".into(),
            description: description.unwrap_or_default().to_owned(),
            discovered: true,
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_parses() {
        let c = Catalog::bundled();
        assert!(c.tools.len() >= 12);
        assert!(c.is_known("photocraft"));
        assert!(c.is_known("ArtCraftX"));
        assert!(!c.is_known("newcraft"));
        for tool in &c.tools {
            assert!(c.categories.iter().any(|cat| cat.id == tool.category), "{}", tool.id);
        }
    }

    #[test]
    fn names_discovered_tools() {
        let t = Tool::discovered("storytold", "gridcraft", Some("Spreadsheets"));
        assert_eq!(t.name, "GridCraft");
        assert_eq!(t.repo, "storytold/gridcraft");
        assert!(t.discovered);
    }
}
