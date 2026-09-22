pub mod markdown;
pub mod text;

use std::path::Path;

use ratatui::text::Line;

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
        self.max_width = self
            .lines
            .iter()
            .map(|line| unicode_width::UnicodeWidthStr::width(line.plain.as_str()))
            .max()
            .unwrap_or(0);
        self
    }
}

pub enum ViewDocument {
    Text(text::TextDocument),
    Markdown(markdown::Document),
}

impl ViewDocument {
    pub fn from_input(input: &Input) -> Self {
        match input.kind {
            InputKind::Text => Self::Text(text::TextDocument::new(&input.content)),
            InputKind::Markdown => Self::Markdown(markdown::parse(&input.content)),
        }
    }

    pub fn render(
        &self,
        width: u16,
        theme: &Theme,
        wrap: bool,
        tab_width: usize,
        images: &mut ImageManager,
        base_dir: Option<&Path>,
    ) -> RenderedDocument {
        match self {
            Self::Text(document) => text::render(document, width, theme, wrap, tab_width),
            Self::Markdown(document) => {
                markdown::render(document, width, theme, wrap, tab_width, images, base_dir)
            }
        }
    }

    pub fn search(&self, matcher: &SearchMatcher) -> Vec<SearchMatch> {
        match self {
            Self::Text(document) => document.search(matcher),
            Self::Markdown(document) => document.search(matcher),
        }
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self, Self::Markdown(_))
    }
}
