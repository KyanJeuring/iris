use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{cli::Cli, options::IconMode};

const DEFAULT_CONFIG: &str = r#"# IRIS configuration
#
# This file is created automatically by IRIS.
#
# All settings are commented out by default.
# Uncomment a setting to override IRIS's built-in default.
#
# IRIS uses TOML syntax even though this file uses the .conf extension.


# ==================================================
# Appearance
# ==================================================

# Theme used by IRIS.
#
# The built-in default theme is:
#   ember
#
# Custom themes can be placed in:
#   ~/.config/iris/themes/
#
# Default:
# theme = "ember"


# Icon set used for alerts, task lists, image placeholders,
# and other interface elements.
#
# Available values:
#   "nerd-font" - Nerd Font icons
#   "unicode"   - portable Unicode symbols
#
# Default:
# icons = "nerd-font"


# ==================================================
# Rendering
# ==================================================

# Wrap long prose lines to the available terminal width.
#
# When disabled, long lines can be viewed using horizontal scrolling.
#
# Default:
# wrap = true


# Number of spaces used when rendering tab characters.
#
# Allowed range:
#   1 - 16
#
# Default:
# tab_width = 4
"#;

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

        if path.is_file() {
            return Self::load_from_path(&path);
        }

        // Keep supporting the v1.0 configuration indefinitely. If it exists,
        if let Some(legacy_path) = legacy_config_path()
            && legacy_path.is_file()
        {
            return Self::load_from_path(&legacy_path);
        }

        ensure_config_exists(&path);

        if path.is_file() {
            Self::load_from_path(&path)
        } else {
            Ok(Self::default())
        }
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)
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
    dirs::config_dir().map(|dir| dir.join("iris").join("iris.conf"))
}

fn legacy_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("iris").join("config.toml"))
}

fn ensure_config_exists(path: &Path) {
    if path.exists() {
        return;
    }

    let Some(parent) = path.parent() else {
        return;
    };

    if let Err(error) = fs::create_dir_all(parent) {
        eprintln!(
            "iris: warning: could not create config directory '{}': {error}",
            parent.display()
        );
        return;
    }

    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return,
        Err(error) => {
            eprintln!(
                "iris: warning: could not create config '{}': {error}",
                path.display()
            );
            return;
        }
    };

    if let Err(error) = file.write_all(DEFAULT_CONFIG.as_bytes()) {
        eprintln!(
            "iris: warning: could not write config '{}': {error}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_config_uses_built_in_defaults_when_fully_commented() {
        let config: Config = toml::from_str(DEFAULT_CONFIG).expect("default config should parse");

        assert_eq!(config.theme, "ember");
        assert!(config.wrap);
        assert_eq!(config.tab_width, 4);
        assert_eq!(config.icons, IconMode::NerdFont);
    }
}
