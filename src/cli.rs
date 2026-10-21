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

    /// Override the title shown in the viewer border.
    #[arg(long, value_name = "TITLE")]
    pub title: Option<String>,

    #[arg(long)]
    pub list_themes: bool,

    #[arg(long, value_enum)]
    pub icons: Option<IconMode>,

    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_wrap")]
    pub wrap: bool,

    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "wrap")]
    pub no_wrap: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_explicit_title() {
        let cli = Cli::try_parse_from(["iris", "--title", "SSH logs", "-"]).unwrap();

        assert_eq!(cli.title.as_deref(), Some("SSH logs"));
        assert_eq!(cli.input.as_deref(), Some(std::path::Path::new("-")));
    }
}
