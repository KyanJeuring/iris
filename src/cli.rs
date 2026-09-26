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
    pub input: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = FormatArg::Auto)]
    pub format: FormatArg,

    #[arg(long)]
    pub theme: Option<String>,

    #[arg(long)]
    pub list_themes: bool,

    #[arg(long, value_enum)]
    pub icons: Option<IconMode>,

    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_wrap")]
    pub wrap: bool,

    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "wrap")]
    pub no_wrap: bool,
}
