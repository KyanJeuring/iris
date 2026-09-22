use ratatui::{style::Style, text::Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct StyledPiece {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

impl StyledPiece {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub fn linked(text: impl Into<String>, style: Style, destination: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style,
            link: Some(destination.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WrappedLink {
    pub destination: String,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Default)]
pub struct WrappedLine {
    pub spans: Vec<Span<'static>>,
    pub plain: String,
    pub links: Vec<WrappedLink>,
}

pub fn wrap_pieces(
    pieces: &[StyledPiece],
    width: usize,
    wrap: bool,
    tab_width: usize,
) -> Vec<WrappedLine> {
    let width = width.max(1);
    let mut output = Vec::new();
    let mut line = WrappedLine::default();
    let mut line_width = 0usize;

    for piece in pieces {
        for bound in UnicodeSegmentation::split_word_bounds(piece.text.as_str()) {
            let expanded = expand_tabs(bound, tab_width);
            process_bound(
                &expanded,
                piece,
                width,
                wrap,
                &mut line,
                &mut line_width,
                &mut output,
            );
        }
    }

    if !line.spans.is_empty() || output.is_empty() {
        output.push(line);
    }
    output
}

fn process_bound(
    bound: &str,
    piece: &StyledPiece,
    width: usize,
    wrap: bool,
    line: &mut WrappedLine,
    line_width: &mut usize,
    output: &mut Vec<WrappedLine>,
) {
    for segment in bound.split_inclusive('\n') {
        let has_newline = segment.ends_with('\n');
        let text = segment.strip_suffix('\n').unwrap_or(segment);
        if !text.is_empty() {
            push_segment(text, piece, width, wrap, line, line_width, output);
        }
        if has_newline {
            push_line(line, line_width, output);
        }
    }
}

fn push_segment(
    segment: &str,
    piece: &StyledPiece,
    width: usize,
    wrap: bool,
    line: &mut WrappedLine,
    line_width: &mut usize,
    output: &mut Vec<WrappedLine>,
) {
    let whitespace = segment.chars().all(char::is_whitespace);
    let segment_width = UnicodeWidthStr::width(segment);

    if wrap && !whitespace && *line_width > 0 && *line_width + segment_width > width {
        push_line(line, line_width, output);
    }

    if wrap && segment_width > width {
        for grapheme in UnicodeSegmentation::graphemes(segment, true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            if *line_width > 0 && *line_width + grapheme_width > width {
                push_line(line, line_width, output);
            }
            append_text(grapheme, piece, line, line_width);
        }
        return;
    }

    if wrap && whitespace && *line_width == 0 {
        return;
    }

    append_text(segment, piece, line, line_width);
}

fn append_text(text: &str, piece: &StyledPiece, line: &mut WrappedLine, line_width: &mut usize) {
    if text.is_empty() {
        return;
    }
    let start = *line_width;
    let width = UnicodeWidthStr::width(text);
    *line_width += width;
    line.plain.push_str(text);
    line.spans.push(Span::styled(text.to_string(), piece.style));
    if let Some(destination) = &piece.link {
        line.links.push(WrappedLink {
            destination: destination.clone(),
            start_column: start,
            end_column: start + width,
        });
    }
}

fn push_line(line: &mut WrappedLine, line_width: &mut usize, output: &mut Vec<WrappedLine>) {
    output.push(std::mem::take(line));
    *line_width = 0;
}

fn expand_tabs(input: &str, tab_width: usize) -> String {
    if !input.contains('\t') {
        return input.to_string();
    }
    input.replace('\t', &" ".repeat(tab_width.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_display_width() {
        let lines = wrap_pieces(
            &[StyledPiece::new("hello world", Style::default())],
            6,
            true,
            4,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].plain, "hello ");
        assert_eq!(lines[1].plain, "world");
    }

    #[test]
    fn preserves_links() {
        let lines = wrap_pieces(
            &[StyledPiece::linked(
                "IRIS",
                Style::default(),
                "https://example.com",
            )],
            80,
            true,
            4,
        );
        assert_eq!(lines[0].links[0].start_column, 0);
        assert_eq!(lines[0].links[0].end_column, 4);
    }
    #[test]
    fn measures_cjk_and_emoji_by_terminal_display_width() {
        assert_eq!(UnicodeWidthStr::width("你好"), 4);
        assert_eq!(UnicodeWidthStr::width("👨‍💻"), 2);

        let lines = wrap_pieces(
            &[StyledPiece::new("你好世界", Style::default())],
            4,
            true,
            4,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].plain, "你好");
        assert_eq!(lines[1].plain, "世界");
    }
}
