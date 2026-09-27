use std::path::Path;

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::{
    image::ImageManager,
    renderer::{RenderedDocument, RenderedImage, RenderedLine},
    theme::Theme,
};

const ZOOM_STEP: u16 = 25;
const MIN_ZOOM: u16 = 25;
const MAX_ZOOM: u16 = 400;
const FIT_ZOOM: u16 = 100;

#[derive(Debug, Clone)]
pub struct ImageDocument {
    source: String,
    zoom_percent: u16,
}

impl ImageDocument {
    pub fn new(path: &Path) -> Self {
        Self {
            source: path.to_string_lossy().into_owned(),
            zoom_percent: FIT_ZOOM,
        }
    }

    pub fn zoom_in(&mut self) -> bool {
        self.set_zoom(self.zoom_percent.saturating_add(ZOOM_STEP).min(MAX_ZOOM))
    }

    pub fn zoom_out(&mut self) -> bool {
        self.set_zoom(self.zoom_percent.saturating_sub(ZOOM_STEP).max(MIN_ZOOM))
    }

    pub fn reset_zoom(&mut self) -> bool {
        self.set_zoom(FIT_ZOOM)
    }

    pub fn zoom_percent(&self) -> u16 {
        self.zoom_percent
    }

    fn set_zoom(&mut self, zoom_percent: u16) -> bool {
        if self.zoom_percent == zoom_percent {
            return false;
        }

        self.zoom_percent = zoom_percent;
        true
    }
}

pub fn render(
    document: &ImageDocument,
    width: u16,
    height: u16,
    theme: &Theme,
    images: &mut ImageManager,
) -> RenderedDocument {
    let width = width.max(1);
    let height = height.max(1);

    let Some(size) =
        images.layout_size_zoomed(&document.source, None, width, height, document.zoom_percent)
    else {
        return render_error(document, width, height, theme);
    };

    let top = height.saturating_sub(size.height) / 2;
    let left = width.saturating_sub(size.width) / 2;
    let document_height = height.max(top.saturating_add(size.height));
    let mut output = RenderedDocument {
        lines: (0..document_height)
            .map(|_| RenderedLine::empty())
            .collect(),
        ..RenderedDocument::default()
    };

    output.images.push(RenderedImage {
        source_id: 0,
        source: document.source.clone(),
        alt: "image".to_string(),
        destination: None,
        line: usize::from(top),
        column: usize::from(left),
        width: size.width,
        height: size.height,
    });

    output.finish()
}

fn render_error(
    document: &ImageDocument,
    width: u16,
    height: u16,
    theme: &Theme,
) -> RenderedDocument {
    let name = Path::new(&document.source)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let message = format!("{} Could not render image: {name}", theme.symbols.image);
    let message_width = UnicodeWidthStr::width(message.as_str());
    let left = usize::from(width).saturating_sub(message_width) / 2;
    let row = usize::from(height / 2);

    let mut lines = (0..height)
        .map(|_| RenderedLine::empty())
        .collect::<Vec<_>>();
    let plain = format!("{}{}", " ".repeat(left), message);
    lines[row] = RenderedLine {
        line: Line::from(vec![
            Span::raw(" ".repeat(left)),
            Span::styled(message, theme.image),
        ]),
        plain,
        source_id: None,
    };

    RenderedDocument {
        lines,
        ..RenderedDocument::default()
    }
    .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_image_zoom_uses_bounded_steps() {
        let mut document = ImageDocument::new(Path::new("image.png"));
        assert_eq!(document.zoom_percent(), 100);

        assert!(document.zoom_in());
        assert_eq!(document.zoom_percent(), 125);

        assert!(document.zoom_out());
        assert_eq!(document.zoom_percent(), 100);

        for _ in 0..20 {
            document.zoom_out();
        }
        assert_eq!(document.zoom_percent(), MIN_ZOOM);
        assert!(!document.zoom_out());

        for _ in 0..30 {
            document.zoom_in();
        }
        assert_eq!(document.zoom_percent(), MAX_ZOOM);
        assert!(!document.zoom_in());

        assert!(document.reset_zoom());
        assert_eq!(document.zoom_percent(), FIT_ZOOM);
    }
}
