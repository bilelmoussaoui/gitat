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
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file '{}'", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("failed to parse config file '{}'", path.display()))
    }
}
