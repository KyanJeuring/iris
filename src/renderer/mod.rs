pub mod image;
pub mod markdown;
pub mod text;

use std::path::Path;

use anyhow::Result;
use ratatui::{layout::Size, text::Line};

use crate::{
    image::ImageManager,
    input::{Input, InputKind},
    search::{SearchMatch, SearchMatcher},
    theme::Theme,
};

pub type NodeId = usize;
pub type HeadingId = NodeId;

#[derive(Debug, Clone)]
pub struct RenderedLine {
    pub line: Line<'static>,
    pub plain: String,
    pub source_id: Option<NodeId>,
}

impl RenderedLine {
    pub fn empty() -> Self {
        Self {
            line: Line::default(),
            plain: String::new(),
            source_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderedHeading {
    pub id: HeadingId,
    pub level: u8,
    pub title: String,
    pub slug: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct RenderedLink {
    pub source_id: NodeId,
    pub destination: String,
    pub line: usize,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone)]
pub struct RenderedImage {
    pub source_id: NodeId,
    pub source: String,
    pub alt: String,
    pub destination: Option<String>,
    pub line: usize,
    pub column: usize,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Default)]
pub struct RenderedDocument {
    pub lines: Vec<RenderedLine>,
    pub headings: Vec<RenderedHeading>,
    pub links: Vec<RenderedLink>,
    pub images: Vec<RenderedImage>,
    pub max_width: usize,
}

impl RenderedDocument {
    pub fn finish(mut self) -> Self {
        let text_width = self
            .lines
            .iter()
            .map(|line| unicode_width::UnicodeWidthStr::width(line.plain.as_str()))
            .max()
            .unwrap_or(0);
        let image_width = self
            .images
            .iter()
            .map(|image| image.column.saturating_add(usize::from(image.width)))
            .max()
            .unwrap_or(0);
        self.max_width = text_width.max(image_width);
        self
    }
}

pub enum ViewDocument {
    Text(text::TextDocument),
    Markdown(markdown::Document),
    Image(image::ImageDocument),
    Pdf(Box<crate::pdf::PdfDocument>),
}

impl ViewDocument {
    pub fn from_input(input: &Input) -> Result<Self> {
        Ok(match input.kind {
            InputKind::Text => Self::Text(text::TextDocument::new(
                input.text().expect("text input should contain text"),
            )),
            InputKind::Markdown => Self::Markdown(markdown::parse(
                input.text().expect("Markdown input should contain text"),
            )),
            InputKind::Image => Self::Image(image::ImageDocument::new(
                input
                    .file_path()
                    .expect("image input should contain a file path"),
            )),
            InputKind::Pdf => Self::Pdf(Box::new(crate::pdf::PdfDocument::open(
                input
                    .file_path()
                    .expect("PDF input should contain a file path"),
            )?)),
        })
    }

    pub fn render(
        &self,
        size: Size,
        theme: &Theme,
        wrap: bool,
        tab_width: usize,
        images: &mut ImageManager,
        base_dir: Option<&Path>,
    ) -> RenderedDocument {
        match self {
            Self::Text(document) => text::render(document, size.width, theme, wrap, tab_width),
            Self::Markdown(document) => markdown::render(
                document, size.width, theme, wrap, tab_width, images, base_dir,
            ),
            Self::Image(document) => {
                image::render(document, size.width, size.height, theme, images)
            }
            Self::Pdf(_) => RenderedDocument::default(),
        }
    }

    pub fn search(&self, matcher: &SearchMatcher) -> Vec<SearchMatch> {
        match self {
            Self::Text(document) => document.search(matcher),
            Self::Markdown(document) => document.search(matcher),
            Self::Image(_) | Self::Pdf(_) => Vec::new(),
        }
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self, Self::Markdown(_))
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    pub fn is_pdf(&self) -> bool {
        matches!(self, Self::Pdf(_))
    }

    pub fn image_zoom_percent(&self) -> Option<u16> {
        match self {
            Self::Image(document) => Some(document.zoom_percent()),
            _ => None,
        }
    }

    pub fn zoom_image_in(&mut self) -> bool {
        match self {
            Self::Image(document) => document.zoom_in(),
            _ => false,
        }
    }

    pub fn zoom_image_out(&mut self) -> bool {
        match self {
            Self::Image(document) => document.zoom_out(),
            _ => false,
        }
    }

    pub fn reset_image_zoom(&mut self) -> bool {
        match self {
            Self::Image(document) => document.reset_zoom(),
            _ => false,
        }
    }
}
