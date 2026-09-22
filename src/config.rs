use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{cli::Cli, options::IconMode};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub wrap: bool,
    pub tab_width: usize,
    pub icons: IconMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "ember".to_string(),
            wrap: true,
            tab_width: 4,
            icons: IconMode::NerdFont,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };

        if !path.exists() {
            return Ok(Self::default());
        }

        let source = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config '{}'", path.display()))?;
        let mut config: Self = toml::from_str(&source)
            .with_context(|| format!("invalid config '{}'", path.display()))?;
        config.tab_width = config.tab_width.clamp(1, 16);
        Ok(config)
    }

    pub fn apply_cli(mut self, cli: &Cli) -> Self {
        if let Some(theme) = &cli.theme {
            self.theme = theme.clone();
        }
        if cli.wrap {
            self.wrap = true;
        }
        if cli.no_wrap {
            self.wrap = false;
        }
        if let Some(icons) = cli.icons {
            self.icons = icons;
        }
        self
    }
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("iris").join("config.toml"))
}
