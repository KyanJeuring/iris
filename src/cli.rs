use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};

use crate::options::IconMode;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum FormatArg {
    Auto,
    Text,
    Markdown,
}

#[derive(Debug, Parser)]
#[command(
    name = "iris",
    version,
    about = "Terminal-native viewing for documents and data",
    long_about = None
)]
pub struct Cli {
    /// File to open. If omitted, IRIS reads piped stdin.
    pub input: Option<PathBuf>,

    /// Force the input format (useful for stdin).
    #[arg(long, value_enum, default_value_t = FormatArg::Auto)]
    pub format: FormatArg,

    /// Theme name from the IRIS themes directory or a path to a theme TOML file.
    #[arg(long)]
    pub theme: Option<String>,

    /// List available themes and exit.
    #[arg(long)]
    pub list_themes: bool,

    /// Icon set used by alerts, task lists, and image placeholders.
    #[arg(long, value_enum)]
    pub icons: Option<IconMode>,

    /// Enable text wrapping for prose.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_wrap")]
    pub wrap: bool,

    /// Disable text wrapping for prose.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "wrap")]
    pub no_wrap: bool,
}
