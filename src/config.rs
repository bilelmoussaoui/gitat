use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Override the project name shown in the report.
    #[serde(default)]
    pub name: Option<String>,
    /// Chart keys to omit from the web report.
    #[serde(default)]
    pub hide: Vec<String>,
    /// Extra name/email substrings identifying bot accounts, on top of the built-in ones.
    #[serde(default)]
    pub bots: Vec<String>,
    /// Company per email address or email domain, overriding the built-in domain table.
    /// An empty company marks the address/domain as personal (no affiliation).
    #[serde(default)]
    pub affiliations: HashMap<String, String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file '{}'", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("failed to parse config file '{}'", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_options() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gitat.toml");
        fs::write(
            &path,
            r#"
name = "GNOME"
hide = ["bus_factor"]
bots = ["damned-lies"]

[affiliations]
"jane@gmail.com" = "Red Hat"
"example.com" = ""
"#,
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.name.as_deref(), Some("GNOME"));
        assert_eq!(config.hide, ["bus_factor"]);
        assert_eq!(config.bots, ["damned-lies"]);
        assert_eq!(config.affiliations["jane@gmail.com"], "Red Hat");
        assert_eq!(config.affiliations["example.com"], "");
    }

    #[test]
    fn everything_is_optional() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gitat.toml");
        fs::write(&path, "").unwrap();
        let config = Config::load(&path).unwrap();
        assert!(config.name.is_none() && config.hide.is_empty() && config.affiliations.is_empty());
    }

    #[test]
    fn reports_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gitat.toml");
        fs::write(&path, "hide = \"not a list\"").unwrap();
        assert!(Config::load(&path).is_err());
        assert!(Config::load(&dir.path().join("missing.toml")).is_err());
    }
}
