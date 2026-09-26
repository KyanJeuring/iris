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
    Image,
    Pdf,
}

impl InputKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Markdown => "Markdown",
            Self::Image => "Image",
            Self::Pdf => "PDF",
        }
    }
}

#[derive(Debug)]
pub enum InputData {
    Text(String),
    File(PathBuf),
}

#[derive(Debug)]
pub struct Input {
    pub data: InputData,
    pub name: String,
    pub kind: InputKind,
    pub base_dir: Option<PathBuf>,
}

impl Input {
    pub fn text(&self) -> Option<&str> {
        match &self.data {
            InputData::Text(content) => Some(content),
            InputData::File(_) => None,
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        match &self.data {
            InputData::Text(_) => None,
            InputData::File(path) => Some(path),
        }
    }
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
    let kind = resolve_kind(Some(path), &bytes, format);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("input"))
        .to_string();

    let data = match kind {
        InputKind::Image | InputKind::Pdf => {
            let path = fs::canonicalize(path)
                .with_context(|| format!("failed to resolve input path '{}'", path.display()))?;
            InputData::File(path)
        }
        InputKind::Text | InputKind::Markdown => {
            let content = String::from_utf8(bytes)
                .map_err(|_| anyhow::anyhow!("input is not valid UTF-8: {}", path.display()))?;
            InputData::Text(content)
        }
    };

    Ok(Input {
        data,
        name,
        kind,
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
        data: InputData::Text(content),
        name: "stdin".to_string(),
        kind: resolve_kind(None, &[], format),
        base_dir: None,
    })
}

fn resolve_kind(path: Option<&Path>, bytes: &[u8], format: FormatArg) -> InputKind {
    match format {
        FormatArg::Text => InputKind::Text,
        FormatArg::Markdown => InputKind::Markdown,
        FormatArg::Auto => path
            .map(|path| detect_kind(path, bytes))
            .unwrap_or(InputKind::Text),
    }
}

fn detect_kind(path: &Path, bytes: &[u8]) -> InputKind {
    if is_pdf_path(path) || looks_like_pdf_bytes(bytes) {
        return InputKind::Pdf;
    }

    if is_markdown_path(path) {
        return InputKind::Markdown;
    }

    if is_image_path(path) || is_supported_raster(bytes) || looks_like_svg_bytes(bytes) {
        return InputKind::Image;
    }

    InputKind::Text
}

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

fn looks_like_pdf_bytes(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|window| window == b"%PDF-")
}

fn is_markdown_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md" | "markdown" | "mdown" | "mkd" | "mkdn")
    )
}

fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg")
    )
}

fn is_supported_raster(bytes: &[u8]) -> bool {
    matches!(
        ::image::guess_format(bytes),
        Ok(::image::ImageFormat::Png
            | ::image::ImageFormat::Jpeg
            | ::image::ImageFormat::Gif
            | ::image::ImageFormat::WebP
            | ::image::ImageFormat::Bmp
            | ::image::ImageFormat::Ico)
    )
}

fn looks_like_svg_bytes(bytes: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let trimmed = prefix.trim_start_matches('\u{feff}').trim_start();

    trimmed.starts_with("<svg")
        || trimmed.starts_with("<?xml") && trimmed.contains("<svg")
        || trimmed.starts_with("<!--") && trimmed.contains("<svg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markdown_extensions() {
        assert_eq!(
            detect_kind(&PathBuf::from("README.md"), b"# IRIS"),
            InputKind::Markdown
        );
        assert_eq!(
            detect_kind(&PathBuf::from("notes.txt"), b"plain text"),
            InputKind::Text
        );
    }

    #[test]
    fn detects_pdf_extension_and_signature() {
        assert_eq!(detect_kind(Path::new("report.pdf"), b""), InputKind::Pdf);
        assert_eq!(
            detect_kind(Path::new("report"), b"%PDF-1.7\n"),
            InputKind::Pdf
        );
    }

    #[test]
    fn detects_supported_image_extensions() {
        for name in [
            "image.png",
            "image.jpg",
            "image.jpeg",
            "image.gif",
            "image.webp",
            "image.bmp",
            "image.ico",
            "image.svg",
        ] {
            assert_eq!(detect_kind(&PathBuf::from(name), b""), InputKind::Image);
        }
    }

    #[test]
    fn detects_extensionless_png_by_signature() {
        let bytes = fs::read("tests/fixtures/assets/iris.png").expect("PNG fixture should exist");
        assert_eq!(detect_kind(Path::new("image"), &bytes), InputKind::Image);
    }

    #[test]
    fn detects_extensionless_svg_by_content() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert_eq!(detect_kind(Path::new("image"), svg), InputKind::Image);
    }
}
