mod alert;
mod code;
mod extensions;
mod html;
pub mod model;
mod parser;
mod render;
mod table;
pub(crate) mod wrap;

pub use model::Document;

use std::path::Path;

use crate::{image::ImageManager, renderer::RenderedDocument, theme::Theme};

pub fn parse(source: &str) -> Document {
    parser::parse(source)
}

pub fn render(
    document: &Document,
    width: u16,
    theme: &Theme,
    wrap: bool,
    tab_width: usize,
    images: &mut ImageManager,
    base_dir: Option<&Path>,
) -> RenderedDocument {
    render::render(document, width, theme, wrap, tab_width, images, base_dir)
}
