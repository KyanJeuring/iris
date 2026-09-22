use clap::ValueEnum;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IconMode {
    #[default]
    NerdFont,
    Unicode,
}
