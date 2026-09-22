use ratatui::text::Line;

use super::{NodeId, RenderedDocument, RenderedLine};
use crate::{
    renderer::markdown::wrap::{StyledPiece, wrap_pieces},
    search::{SearchMatch, SearchMatcher},
    theme::Theme,
};

#[derive(Debug, Clone)]
pub struct TextLine {
    pub id: NodeId,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct TextDocument {
    lines: Vec<TextLine>,
}

impl TextDocument {
    pub fn new(source: &str) -> Self {
        let lines = source
            .split('\n')
            .enumerate()
            .map(|(index, content)| TextLine {
                id: index + 1,
                content: content.trim_end_matches('\r').to_string(),
            })
            .collect();
        Self { lines }
    }

    pub fn search(&self, matcher: &SearchMatcher) -> Vec<SearchMatch> {
        let mut output = Vec::new();
        for line in &self.lines {
            for occurrence in 0..matcher.count(&line.content) {
                output.push(SearchMatch {
                    node_id: line.id,
                    occurrence,
                });
            }
        }
        output
    }
}

pub fn render(
    document: &TextDocument,
    width: u16,
    theme: &Theme,
    wrap: bool,
    tab_width: usize,
) -> RenderedDocument {
    let mut output = RenderedDocument::default();
    let width = width.max(1) as usize;

    for source in &document.lines {
        let pieces = vec![StyledPiece::new(source.content.clone(), theme.document)];
        let wrapped = wrap_pieces(&pieces, width, wrap, tab_width);
        for line in wrapped {
            output.lines.push(RenderedLine {
                line: Line::from(line.spans),
                plain: line.plain,
                source_id: Some(source.id),
            });
        }
    }

    if output.lines.is_empty() {
        output.lines.push(RenderedLine::empty());
    }
    output.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_still_a_document() {
        let doc = TextDocument::new("");
        assert_eq!(doc.lines.len(), 1);
    }
}
