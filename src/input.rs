use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::cli::FormatArg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Markdown,
}

impl InputKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Markdown => "Markdown",
        }
    }
}

#[derive(Debug)]
pub struct Input {
    pub content: String,
    pub name: String,
    pub kind: InputKind,
    pub base_dir: Option<PathBuf>,
}

pub fn read_input(path: Option<&Path>, format: FormatArg) -> Result<Input> {
    match path {
        Some(path) if path != Path::new("-") => read_file(path, format),
        Some(_) => read_stdin(format),
        None if !io::stdin().is_terminal() => read_stdin(format),
        None => bail!("no input provided (try 'iris <file>' or pipe data into iris)"),
    }
}

fn read_file(path: &Path, format: FormatArg) -> Result<Input> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }
    if path.is_dir() {
        bail!("cannot read directory: {}", path.display());
    }

    let bytes = fs::read(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    let content = String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("input is not valid UTF-8: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("input"))
        .to_string();

    Ok(Input {
        content,
        name,
        kind: resolve_kind(Some(path), format),
        base_dir: path.parent().map(Path::to_path_buf),
    })
}

fn read_stdin(format: FormatArg) -> Result<Input> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .read_to_end(&mut bytes)
        .context("failed to read stdin")?;
    let content =
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("stdin is not valid UTF-8"))?;

    Ok(Input {
        content,
        name: "stdin".to_string(),
        kind: resolve_kind(None, format),
        base_dir: None,
    })
}

fn resolve_kind(path: Option<&Path>, format: FormatArg) -> InputKind {
    match format {
        FormatArg::Text => InputKind::Text,
        FormatArg::Markdown => InputKind::Markdown,
        FormatArg::Auto => path.map(detect_kind).unwrap_or(InputKind::Text),
    }
}

fn detect_kind(path: &Path) -> InputKind {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("md" | "markdown" | "mdown" | "mkd" | "mkdn") => InputKind::Markdown,
        _ => InputKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markdown_extensions() {
        assert_eq!(
            detect_kind(&std::path::PathBuf::from("README.md")),
            InputKind::Markdown
        );
        assert_eq!(
            detect_kind(&std::path::PathBuf::from("notes.txt")),
            InputKind::Text
        );
    }
}
