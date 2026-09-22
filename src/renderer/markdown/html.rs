use std::collections::HashMap;

use ratatui::style::Style;

use super::wrap::StyledPiece;
use crate::theme::Theme;

#[derive(Debug, Clone)]
struct HtmlFrame {
    name: String,
    style: Style,
    link: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct InlineHtmlState {
    stack: Vec<HtmlFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlAction {
    None,
    Break,
    Rule,
    Image { source: String, alt: String },
}

impl InlineHtmlState {
    pub fn style(&self, base: Style) -> Style {
        self.stack
            .iter()
            .fold(base, |style, frame| style.patch(frame.style))
    }

    pub fn link(&self, inherited: Option<&str>) -> Option<String> {
        self.stack
            .iter()
            .rev()
            .find_map(|frame| frame.link.clone())
            .or_else(|| inherited.map(ToOwned::to_owned))
    }

    pub fn consume_tag(&mut self, raw: &str, theme: &Theme) -> HtmlAction {
        let Some(tag) = parse_tag(raw) else {
            return HtmlAction::None;
        };

        if tag.closing {
            if let Some(index) = self.stack.iter().rposition(|frame| frame.name == tag.name) {
                self.stack.truncate(index);
            }
            return HtmlAction::None;
        }

        match tag.name.as_str() {
            "br" => return HtmlAction::Break,
            "hr" => return HtmlAction::Rule,
            "img" => {
                let source = tag.attrs.get("src").cloned().unwrap_or_default();
                let alt = tag.attrs.get("alt").cloned().unwrap_or_default();
                if !source.is_empty() {
                    return HtmlAction::Image { source, alt };
                }
                return HtmlAction::None;
            }
            _ => {}
        }

        let style = style_for_tag(&tag.name, theme);
        let link = if tag.name == "a" {
            tag.attrs.get("href").cloned()
        } else {
            None
        };

        if style != Style::default() || link.is_some() {
            self.stack.push(HtmlFrame {
                name: tag.name,
                style,
                link,
            });
        }

        HtmlAction::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandaloneHtmlImage {
    pub source: String,
    pub alt: String,
    pub destination: Option<String>,
}

pub fn standalone_image(content: &str) -> Option<StandaloneHtmlImage> {
    let mut image = None;
    let mut link = None;

    for token in tokenize(content) {
        match token {
            Token::Text(text) if text.trim().is_empty() => {}
            Token::Tag(raw) => {
                let tag = parse_tag(raw)?;
                if tag.name == "img" && !tag.closing {
                    if image.is_some() {
                        return None;
                    }
                    image = Some(StandaloneHtmlImage {
                        source: tag.attrs.get("src")?.clone(),
                        alt: tag.attrs.get("alt").cloned().unwrap_or_default(),
                        destination: link.clone(),
                    });
                    continue;
                }

                if tag.name == "a" {
                    if tag.closing {
                        continue;
                    }
                    link = tag.attrs.get("href").cloned();
                    continue;
                }

                // Common README wrappers around a single image should not stop IRIS from
                // rendering the image itself. Presentation-only attributes such as `align`
                // are intentionally ignored in the terminal.
                if matches!(tag.name.as_str(), "p" | "div" | "center" | "figure") {
                    continue;
                }

                return None;
            }
            _ => return None,
        }
    }
    image
}

pub fn block_pieces(content: &str, width: usize, theme: &Theme) -> Vec<StyledPiece> {
    let mut output = Vec::new();
    let mut state = InlineHtmlState::default();
    let mut hidden: Option<String> = None;
    let mut table_cells_in_row = 0usize;

    for token in tokenize(content) {
        match token {
            Token::Text(text) => {
                if hidden.is_some() {
                    continue;
                }
                let text = html_escape::decode_html_entities(text).into_owned();
                if !text.is_empty() {
                    push_piece(
                        &mut output,
                        text,
                        state.style(theme.document),
                        state.link(None),
                    );
                }
            }
            Token::Tag(raw) => {
                let Some(tag) = parse_tag(raw) else {
                    continue;
                };

                if let Some(hidden_name) = &hidden {
                    if tag.closing && &tag.name == hidden_name {
                        hidden = None;
                    }
                    continue;
                }

                if !tag.closing && matches!(tag.name.as_str(), "script" | "style") {
                    hidden = Some(tag.name);
                    continue;
                }

                if tag.name == "tr" {
                    if !tag.closing {
                        push_break(&mut output, theme.document);
                        table_cells_in_row = 0;
                    } else {
                        push_break(&mut output, theme.document);
                    }
                } else if !tag.closing && matches!(tag.name.as_str(), "td" | "th") {
                    if table_cells_in_row > 0 {
                        push_piece(&mut output, " │ ".to_string(), theme.table_border, None);
                    }
                    table_cells_in_row += 1;
                } else if !tag.closing && is_block_tag(&tag.name) {
                    push_break(&mut output, theme.document);
                }

                if !tag.closing && tag.name == "li" {
                    push_piece(
                        &mut output,
                        format!("{} ", theme.list.bullet),
                        theme.list.marker_style,
                        None,
                    );
                }

                match state.consume_tag(raw, theme) {
                    HtmlAction::None => {}
                    HtmlAction::Break => push_break(&mut output, theme.document),
                    HtmlAction::Rule => {
                        push_break(&mut output, theme.document);
                        push_piece(
                            &mut output,
                            "─".repeat(width.max(1)),
                            theme.horizontal_rule,
                            None,
                        );
                        push_break(&mut output, theme.document);
                    }
                    HtmlAction::Image { source, alt } => {
                        let label = image_label(&source, &alt, theme);
                        push_piece(&mut output, label, theme.image, Some(source));
                    }
                }

                if tag.closing
                    && is_block_tag(&tag.name)
                    && !matches!(tag.name.as_str(), "tr" | "td" | "th")
                {
                    push_break(&mut output, theme.document);
                }
            }
        }
    }

    while output.last().is_some_and(|piece| piece.text == "\n") {
        output.pop();
    }
    output
}

pub fn plain_text(content: &str) -> String {
    let mut output = String::new();
    let mut hidden: Option<String> = None;

    for token in tokenize(content) {
        match token {
            Token::Text(text) if hidden.is_none() => {
                output.push_str(&html_escape::decode_html_entities(text));
            }
            Token::Tag(raw) => {
                let Some(tag) = parse_tag(raw) else {
                    continue;
                };
                if let Some(hidden_name) = &hidden {
                    if tag.closing && &tag.name == hidden_name {
                        hidden = None;
                    }
                    continue;
                }
                if !tag.closing && matches!(tag.name.as_str(), "script" | "style") {
                    hidden = Some(tag.name);
                    continue;
                }
                if tag.name == "img" && !tag.closing {
                    if let Some(alt) = tag.attrs.get("alt") {
                        output.push_str(alt);
                    }
                } else if matches!(tag.name.as_str(), "br" | "p" | "div" | "li") {
                    output.push(' ');
                }
            }
            _ => {}
        }
    }

    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn image_label(source: &str, alt: &str, theme: &Theme) -> String {
    let description = if alt.trim().is_empty() { source } else { alt };
    format!("{} {description}", theme.symbols.image)
}

fn style_for_tag(name: &str, theme: &Theme) -> Style {
    match name {
        "b" | "strong" | "summary" | "th" => theme.strong,
        "em" | "i" => theme.emphasis,
        "del" | "s" | "strike" => theme.strikethrough,
        "code" => theme.inline_code,
        "kbd" => theme.html_kbd,
        "mark" => theme.html_mark,
        "u" | "ins" => theme.html_underline,
        "small" | "sub" | "sup" => theme.html_subtle,
        "a" => theme.link,
        "blockquote" => theme.quote,
        "pre" => theme.code,
        "h1" => theme.heading_style(1),
        "h2" => theme.heading_style(2),
        "h3" => theme.heading_style(3),
        "h4" => theme.heading_style(4),
        "h5" => theme.heading_style(5),
        "h6" => theme.heading_style(6),
        _ => Style::default(),
    }
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "details"
            | "div"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "li"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "tfoot"
            | "thead"
            | "ul"
    )
}

fn push_piece(output: &mut Vec<StyledPiece>, text: String, style: Style, link: Option<String>) {
    if text.is_empty() {
        return;
    }
    output.push(match link {
        Some(destination) => StyledPiece::linked(text, style, destination),
        None => StyledPiece::new(text, style),
    });
}

fn push_break(output: &mut Vec<StyledPiece>, style: Style) {
    if output.is_empty()
        || output
            .last()
            .is_some_and(|piece| piece.text.ends_with('\n'))
    {
        return;
    }
    output.push(StyledPiece::new("\n", style));
}

#[derive(Debug, Clone)]
struct TagInfo {
    name: String,
    closing: bool,
    attrs: HashMap<String, String>,
}

fn parse_tag(raw: &str) -> Option<TagInfo> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('<') || !trimmed.ends_with('>') {
        return None;
    }

    let mut inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.starts_with("!--") || inner.starts_with('!') || inner.starts_with('?') {
        return None;
    }

    let closing = inner.starts_with('/');
    if closing {
        inner = inner[1..].trim_start();
    }
    inner = inner.trim_end_matches('/').trim_end();

    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }

    let attrs = if closing {
        HashMap::new()
    } else {
        parse_attributes(&inner[name_end..])
    };

    Some(TagInfo {
        name,
        closing,
        attrs,
    })
}

fn parse_attributes(mut input: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();

    while !input.trim_start().is_empty() {
        input = input.trim_start();
        if input.starts_with('/') {
            break;
        }

        let key_end = input
            .find(|ch: char| ch.is_whitespace() || ch == '=')
            .unwrap_or(input.len());
        let key = input[..key_end].trim().to_ascii_lowercase();
        if key.is_empty() {
            break;
        }
        input = &input[key_end..];
        input = input.trim_start();

        let mut value = String::new();
        if let Some(rest) = input.strip_prefix('=') {
            input = rest.trim_start();
            if let Some(quote) = input.chars().next().filter(|ch| matches!(ch, '\'' | '"')) {
                input = &input[quote.len_utf8()..];
                if let Some(end) = input.find(quote) {
                    value = html_escape::decode_html_entities(&input[..end]).into_owned();
                    input = &input[end + quote.len_utf8()..];
                } else {
                    value = html_escape::decode_html_entities(input).into_owned();
                    input = "";
                }
            } else {
                let end = input.find(char::is_whitespace).unwrap_or(input.len());
                value = html_escape::decode_html_entities(&input[..end]).into_owned();
                input = &input[end..];
            }
        }

        attrs.insert(key, value);
    }

    attrs
}

enum Token<'a> {
    Text(&'a str),
    Tag(&'a str),
}

fn tokenize(input: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut cursor = 0usize;

    while cursor < input.len() {
        let Some(relative_start) = input[cursor..].find('<') else {
            tokens.push(Token::Text(&input[cursor..]));
            break;
        };
        let start = cursor + relative_start;
        if start > cursor {
            tokens.push(Token::Text(&input[cursor..start]));
        }

        if input[start..].starts_with("<!--") {
            if let Some(relative_end) = input[start + 4..].find("-->") {
                cursor = start + 4 + relative_end + 3;
                continue;
            }
            break;
        }

        let Some(relative_end) = input[start..].find('>') else {
            tokens.push(Token::Text(&input[start..]));
            break;
        };
        let end = start + relative_end + 1;
        tokens.push(Token::Tag(&input[start..end]));
        cursor = end;
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_standalone_image() {
        let image = standalone_image(r#"<img src="docs/iris.png" alt="IRIS">"#).unwrap();
        assert_eq!(image.source, "docs/iris.png");
        assert_eq!(image.alt, "IRIS");
        assert!(image.destination.is_none());
    }

    #[test]
    fn extracts_image_inside_common_readme_wrapper() {
        let image =
            standalone_image(r#"<p align="center"><img src="docs/iris.png" alt="IRIS"></p>"#)
                .unwrap();
        assert_eq!(image.source, "docs/iris.png");
        assert_eq!(image.alt, "IRIS");
        assert!(image.destination.is_none());
    }

    #[test]
    fn keeps_link_destination_for_wrapped_image() {
        let image = standalone_image(
            r#"<a href="https://example.com"><img src="docs/iris.png" alt="IRIS"></a>"#,
        )
        .unwrap();
        assert_eq!(image.source, "docs/iris.png");
        assert_eq!(image.destination.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn strips_tags_from_plain_text() {
        assert_eq!(
            plain_text("<strong>Hello</strong> <em>IRIS</em>"),
            "Hello IRIS"
        );
    }

    #[test]
    fn renders_html_table_cells_readably() {
        let theme = Theme::ember(crate::options::IconMode::Unicode).unwrap();
        let pieces = block_pieces(
            "<table><tr><th>Name</th><th>Value</th></tr><tr><td>IRIS</td><td>1</td></tr></table>",
            80,
            &theme,
        );
        let plain = pieces
            .into_iter()
            .map(|piece| piece.text)
            .collect::<String>();
        assert!(plain.contains("Name │ Value"));
        assert!(plain.contains("IRIS │ 1"));
    }

    #[test]
    fn parses_quoted_attributes() {
        let tag = parse_tag(r#"<a href="https://example.com" title='Example'>"#).unwrap();
        assert_eq!(tag.attrs.get("href").unwrap(), "https://example.com");
        assert_eq!(tag.attrs.get("title").unwrap(), "Example");
    }
}
