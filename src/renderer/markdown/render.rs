use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::{
    alert, code,
    extensions::{self, FencedKind},
    html,
    model::{Block, Document, Inline, ListItem},
    table,
    wrap::{StyledPiece, WrappedLine, wrap_pieces},
};
use crate::{
    image::ImageManager,
    renderer::{
        NodeId, RenderedDocument, RenderedHeading, RenderedImage, RenderedLine, RenderedLink,
    },
    theme::Theme,
};
use std::path::Path;

pub fn render(
    document: &Document,
    width: u16,
    theme: &Theme,
    wrap: bool,
    tab_width: usize,
    images: &mut ImageManager,
    base_dir: Option<&Path>,
) -> RenderedDocument {
    let mut context = Context {
        width: width.max(1) as usize,
        theme,
        wrap,
        tab_width,
        output: RenderedDocument::default(),
        images,
        base_dir,
    };
    context.render_blocks(&document.blocks);
    while context
        .output
        .lines
        .last()
        .is_some_and(|line| line.plain.is_empty())
    {
        context.output.lines.pop();
    }
    if context.output.lines.is_empty() {
        context.output.lines.push(RenderedLine::empty());
    }
    context.output.finish()
}

struct Context<'a> {
    width: usize,
    theme: &'a Theme,
    wrap: bool,
    tab_width: usize,
    output: RenderedDocument,
    images: &'a mut ImageManager,
    base_dir: Option<&'a Path>,
}

impl Context<'_> {
    fn render_blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            self.render_block(block);
        }
    }

    fn render_block(&mut self, block: &Block) {
        match block {
            Block::Paragraph { id, content } => {
                self.render_inline_flow(*id, content, self.theme.document);
            }
            Block::Heading(heading) => self.render_heading(heading),
            Block::BlockQuote { id, kind, blocks } => self.render_quote(*id, *kind, blocks),
            Block::CodeBlock { id, language, code } => {
                self.render_code(*id, language.as_deref(), code)
            }
            Block::List {
                ordered_start,
                items,
                ..
            } => self.render_list(*ordered_start, items),
            Block::Table(table_model) => self.render_table(table_model),
            Block::HorizontalRule { id } => {
                let rule = "─".repeat(self.width.max(1));
                self.output.lines.push(RenderedLine {
                    line: Line::from(Span::styled(rule.clone(), self.theme.horizontal_rule)),
                    plain: rule,
                    source_id: Some(*id),
                });
                self.push_blank();
            }
            Block::DisplayMath { id, content } => {
                let text = format!("$$ {content} $$");
                let pieces = vec![StyledPiece::new(text, self.theme.inline_code)];
                self.push_wrapped(
                    *id,
                    wrap_pieces(&pieces, self.width, self.wrap, self.tab_width),
                );
                self.push_blank();
            }
            Block::Html { id, content } => {
                if let Some(image) = html::standalone_image(content) {
                    self.render_image(*id, &image.source, &image.alt, image.destination.as_deref());
                } else {
                    let pieces = html::block_pieces(content, self.width, self.theme);
                    if !pieces.is_empty() {
                        self.push_wrapped(
                            *id,
                            wrap_pieces(&pieces, self.width, self.wrap, self.tab_width),
                        );
                        self.push_blank();
                    }
                }
            }
        }
    }

    fn render_heading(&mut self, heading: &super::model::Heading) {
        if !self.output.lines.is_empty()
            && !self
                .output
                .lines
                .last()
                .is_some_and(|line| line.plain.is_empty())
        {
            self.push_blank();
        }

        let title = Inline::plain_text(&heading.content);
        let heading_line = self.output.lines.len();
        self.output.headings.push(RenderedHeading {
            id: heading.id,
            level: heading.level,
            title: title.clone(),
            slug: heading.slug.clone(),
            line: heading_line,
        });

        let pieces = inline_pieces(
            &heading.content,
            self.theme.heading_style(heading.level),
            self.theme,
            None,
        );
        self.push_wrapped(
            heading.id,
            wrap_pieces(&pieces, self.width, self.wrap, self.tab_width),
        );

        if let Some(separator) = self.theme.heading_separator(heading.level) {
            let glyph = match separator {
                "double" => '═',
                "single" => '─',
                other => other.chars().next().unwrap_or('─'),
            };
            let text = glyph.to_string().repeat(self.width.max(1));
            self.output.lines.push(RenderedLine {
                line: Line::from(Span::styled(
                    text.clone(),
                    self.theme.heading_style(heading.level),
                )),
                plain: text,
                source_id: None,
            });
        }
        self.push_blank();
    }

    fn render_quote(
        &mut self,
        id: NodeId,
        kind: Option<super::model::AlertKind>,
        blocks: &[Block],
    ) {
        let start = self.output.lines.len();
        let old_width = self.width;
        self.width = self.width.saturating_sub(2).max(1);

        if let Some(kind) = kind {
            let label = alert::label(kind, self.theme);
            self.output.lines.push(RenderedLine {
                line: Line::from(Span::styled(label.clone(), alert::style(kind, self.theme))),
                plain: label,
                source_id: Some(id),
            });
        }
        self.render_blocks(blocks);
        self.width = old_width;

        while self
            .output
            .lines
            .last()
            .is_some_and(|line| line.plain.is_empty())
            && self.output.lines.len() > start
        {
            self.output.lines.pop();
        }

        let end = self.output.lines.len();
        self.prefix_range(start, end, "│ ", self.theme.quote_border);
        self.push_blank();
    }

    fn render_list(&mut self, ordered_start: Option<u64>, items: &[ListItem]) {
        for (index, item) in items.iter().enumerate() {
            let marker = match (ordered_start, item.checked) {
                (Some(start), Some(checked)) => {
                    format!(
                        "{}. {}",
                        start + index as u64,
                        if checked {
                            self.theme.symbols.task_checked.as_str()
                        } else {
                            self.theme.symbols.task_unchecked.as_str()
                        }
                    )
                }
                (Some(start), None) => format!("{}.", start + index as u64),
                (None, Some(checked)) => {
                    if checked {
                        self.theme.symbols.task_checked.clone()
                    } else {
                        self.theme.symbols.task_unchecked.clone()
                    }
                }
                (None, None) => self.theme.list.bullet.clone(),
            };
            let marker_style = match item.checked {
                Some(true) => self.theme.task_checked,
                Some(false) => self.theme.task_unchecked,
                None => self.theme.list.marker_style,
            };
            let mut prefix = format!("{marker} ");
            let marker_width = UnicodeWidthStr::width(prefix.as_str());
            let prefix_width = marker_width.max(self.theme.list.indent);
            if prefix_width > marker_width {
                prefix.push_str(&" ".repeat(prefix_width - marker_width));
            }

            let start = self.output.lines.len();
            let old_width = self.width;
            self.width = self.width.saturating_sub(prefix_width).max(1);

            if !item.content.is_empty() {
                self.render_inline_flow(item.id, &item.content, self.theme.list.style);
            }

            for block in &item.blocks {
                match block {
                    Block::Paragraph { id, content } => {
                        self.render_inline_flow(*id, content, self.theme.list.style);
                    }
                    _ => self.render_block(block),
                }
            }

            self.width = old_width;

            while self
                .output
                .lines
                .last()
                .is_some_and(|line| line.plain.is_empty())
                && self.output.lines.len() > start
            {
                self.output.lines.pop();
            }

            let end = self.output.lines.len();
            if start == end {
                self.output.lines.push(RenderedLine {
                    line: Line::from(Span::styled(marker.clone(), marker_style)),
                    plain: marker,
                    source_id: Some(item.id),
                });
            } else {
                self.prefix_list_range(start, end, &prefix, marker_style, prefix_width);
            }
        }
        self.push_blank();
    }

    fn render_inline_flow(&mut self, id: NodeId, content: &[Inline], style: Style) {
        let mut segment_start = 0usize;

        for (index, inline) in content.iter().enumerate() {
            let Some(image) = markdown_image(inline) else {
                continue;
            };

            if segment_start < index {
                let pieces = inline_pieces(&content[segment_start..index], style, self.theme, None);
                self.push_wrapped(
                    id,
                    wrap_pieces(&pieces, self.width, self.wrap, self.tab_width),
                );
                self.push_blank();
            }

            self.render_image(id, image.source, &image.alt, image.destination);
            segment_start = index + 1;
        }

        if segment_start < content.len() {
            let pieces = inline_pieces(&content[segment_start..], style, self.theme, None);
            self.push_wrapped(
                id,
                wrap_pieces(&pieces, self.width, self.wrap, self.tab_width),
            );
            self.push_blank();
        } else if content.is_empty() {
            self.push_blank();
        }
    }

    fn render_image(&mut self, id: NodeId, source: &str, alt: &str, destination: Option<&str>) {
        let available_width = self.width.min(u16::MAX as usize) as u16;
        let size = self.images.layout_size(
            source,
            self.base_dir,
            available_width.max(1),
            self.theme.image_max_height.max(1),
        );
        let height = size.map(|size| size.height).unwrap_or(1).max(1);
        let line = self.output.lines.len();
        let label = html::image_label(source, alt, self.theme);

        self.output.lines.push(RenderedLine {
            line: Line::from(Span::styled(label.clone(), self.theme.image)),
            plain: label,
            source_id: Some(id),
        });
        for _ in 1..height {
            self.output.lines.push(RenderedLine {
                line: Line::from(Span::raw(" ")),
                plain: " ".to_string(),
                source_id: Some(id),
            });
        }
        self.output.images.push(RenderedImage {
            source_id: id,
            source: source.to_string(),
            alt: alt.to_string(),
            destination: destination.map(ToOwned::to_owned),
            line,
            column: 0,
            height,
        });
        self.push_blank();
    }

    fn render_code(&mut self, id: NodeId, language: Option<&str>, source: &str) {
        let expanded = source.replace('\t', &" ".repeat(self.tab_width.max(1)));
        let kind = extensions::classify_fenced(language);
        let display_language = match kind {
            FencedKind::Mermaid => Some("mermaid"),
            FencedKind::Code => language,
        };
        let highlighted = code::highlight(&expanded, language, self.theme);
        let max_code_width = expanded
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let title = display_language.unwrap_or("code");
        let title_segment = format!("─ {title} ");
        let inner_width = (max_code_width.max(self.width.saturating_sub(4)).max(1) + 2)
            .max(UnicodeWidthStr::width(title_segment.as_str()));
        let content_width = inner_width.saturating_sub(2).max(1);
        let top = format!(
            "┌{}{}┐",
            title_segment,
            "─".repeat(inner_width.saturating_sub(UnicodeWidthStr::width(title_segment.as_str())))
        );
        self.output.lines.push(RenderedLine {
            line: Line::from(Span::styled(top.clone(), self.theme.code_border)),
            plain: top,
            source_id: None,
        });

        let plain_lines = expanded.lines().collect::<Vec<_>>();
        for (line_index, spans) in highlighted.into_iter().enumerate() {
            let plain = plain_lines
                .get(line_index)
                .copied()
                .unwrap_or("")
                .to_string();
            let plain_width = UnicodeWidthStr::width(plain.as_str());
            let mut line_spans = vec![Span::styled("│ ".to_string(), self.theme.code_border)];
            line_spans.extend(spans);
            if content_width > plain_width {
                line_spans.push(Span::styled(
                    " ".repeat(content_width - plain_width),
                    self.theme.code,
                ));
            }
            line_spans.push(Span::styled(" │".to_string(), self.theme.code_border));
            let rendered_plain = format!(
                "│ {plain}{} │",
                " ".repeat(content_width.saturating_sub(plain_width))
            );
            self.output.lines.push(RenderedLine {
                line: Line::from(line_spans),
                plain: rendered_plain,
                source_id: Some(id),
            });
        }

        let bottom = format!("└{}┘", "─".repeat(inner_width));
        self.output.lines.push(RenderedLine {
            line: Line::from(Span::styled(bottom.clone(), self.theme.code_border)),
            plain: bottom,
            source_id: None,
        });
        self.push_blank();
    }

    fn render_table(&mut self, model: &super::model::Table) {
        let header = model
            .header
            .iter()
            .map(|cell| inline_pieces(cell, self.theme.table_header, self.theme, None))
            .collect::<Vec<_>>();
        let rows = model
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| inline_pieces(cell, self.theme.table, self.theme, None))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut table_output = table::render(
            model.id,
            header,
            rows,
            &model.alignments,
            self.width,
            self.theme,
            self.tab_width,
        );
        let base = self.output.lines.len();
        for link in &mut table_output.links {
            link.line += base;
        }
        self.output.lines.extend(table_output.lines);
        self.output.links.extend(table_output.links);
        self.push_blank();
    }

    fn push_wrapped(&mut self, source_id: NodeId, lines: Vec<WrappedLine>) {
        for wrapped in lines {
            let line_index = self.output.lines.len();
            for link in wrapped.links {
                self.output.links.push(RenderedLink {
                    source_id,
                    destination: link.destination,
                    line: line_index,
                    start_column: link.start_column,
                    end_column: link.end_column,
                });
            }
            self.output.lines.push(RenderedLine {
                line: Line::from(wrapped.spans),
                plain: wrapped.plain,
                source_id: Some(source_id),
            });
        }
    }

    fn push_blank(&mut self) {
        if !self
            .output
            .lines
            .last()
            .is_some_and(|line| line.plain.is_empty())
        {
            self.output.lines.push(RenderedLine::empty());
        }
    }

    fn prefix_range(&mut self, start: usize, end: usize, prefix: &str, style: Style) {
        let prefix_width = UnicodeWidthStr::width(prefix);
        for line in self.output.lines.iter_mut().take(end).skip(start) {
            line.line
                .spans
                .insert(0, Span::styled(prefix.to_string(), style));
            line.plain.insert_str(0, prefix);
        }
        for link in &mut self.output.links {
            if (start..end).contains(&link.line) {
                link.start_column += prefix_width;
                link.end_column += prefix_width;
            }
        }
        for image in &mut self.output.images {
            if (start..end).contains(&image.line) {
                image.column += prefix_width;
            }
        }
    }

    fn prefix_list_range(
        &mut self,
        start: usize,
        end: usize,
        prefix: &str,
        style: Style,
        width: usize,
    ) {
        let continuation = " ".repeat(width);
        for (offset, line) in self
            .output
            .lines
            .iter_mut()
            .take(end)
            .skip(start)
            .enumerate()
        {
            let text = if offset == 0 {
                prefix
            } else {
                continuation.as_str()
            };
            let line_style = if offset == 0 {
                style
            } else {
                self.theme.list.style
            };
            line.line
                .spans
                .insert(0, Span::styled(text.to_string(), line_style));
            line.plain.insert_str(0, text);
        }
        for link in &mut self.output.links {
            if (start..end).contains(&link.line) {
                link.start_column += width;
                link.end_column += width;
            }
        }
        for image in &mut self.output.images {
            if (start..end).contains(&image.line) {
                image.column += width;
            }
        }
    }
}

struct MarkdownImage<'a> {
    source: &'a str,
    alt: String,
    destination: Option<&'a str>,
}

fn markdown_image(inline: &Inline) -> Option<MarkdownImage<'_>> {
    match inline {
        Inline::Image { alt, source } => Some(MarkdownImage {
            source,
            alt: Inline::plain_text(alt),
            destination: None,
        }),
        Inline::Link { text, destination } => match text.as_slice() {
            [Inline::Image { alt, source }] => Some(MarkdownImage {
                source,
                alt: Inline::plain_text(alt),
                destination: Some(destination.as_str()),
            }),
            _ => None,
        },
        _ => None,
    }
}

fn inline_pieces(
    items: &[Inline],
    inherited: Style,
    theme: &Theme,
    inherited_link: Option<&str>,
) -> Vec<StyledPiece> {
    let mut output = Vec::new();
    let mut html_state = html::InlineHtmlState::default();
    inline_pieces_into(
        items,
        inherited,
        theme,
        inherited_link,
        &mut html_state,
        &mut output,
    );
    output
}

fn inline_pieces_into(
    items: &[Inline],
    inherited: Style,
    theme: &Theme,
    inherited_link: Option<&str>,
    html_state: &mut html::InlineHtmlState,
    output: &mut Vec<StyledPiece>,
) {
    for item in items {
        match item {
            Inline::Text(text) => {
                let link = html_state.link(inherited_link);
                push_piece(
                    output,
                    text.clone(),
                    html_state.style(inherited),
                    link.as_deref(),
                );
            }
            Inline::Strong(inner) => inline_pieces_into(
                inner,
                inherited.patch(theme.strong),
                theme,
                inherited_link,
                html_state,
                output,
            ),
            Inline::Emphasis(inner) => inline_pieces_into(
                inner,
                inherited.patch(theme.emphasis),
                theme,
                inherited_link,
                html_state,
                output,
            ),
            Inline::Strikethrough(inner) => inline_pieces_into(
                inner,
                inherited.patch(theme.strikethrough),
                theme,
                inherited_link,
                html_state,
                output,
            ),
            Inline::Code(code) => {
                let link = html_state.link(inherited_link);
                push_piece(
                    output,
                    code.clone(),
                    html_state.style(inherited.patch(theme.inline_code)),
                    link.as_deref(),
                );
            }
            Inline::Link { text, destination } => inline_pieces_into(
                text,
                inherited.patch(theme.link),
                theme,
                Some(destination),
                html_state,
                output,
            ),
            Inline::Image { alt, source } => {
                let alt = Inline::plain_text(alt);
                let label = html::image_label(source, &alt, theme);
                push_piece(output, label, inherited.patch(theme.image), Some(source));
            }
            Inline::InlineMath(math) => {
                let link = html_state.link(inherited_link);
                push_piece(
                    output,
                    format!("${math}$"),
                    html_state.style(inherited.patch(theme.inline_code)),
                    link.as_deref(),
                );
            }
            Inline::RawHtml(raw) => match html_state.consume_tag(raw, theme) {
                html::HtmlAction::None => {}
                html::HtmlAction::Break => {
                    let link = html_state.link(inherited_link);
                    push_piece(
                        output,
                        "\n".to_string(),
                        html_state.style(inherited),
                        link.as_deref(),
                    );
                }
                html::HtmlAction::Rule => {
                    let link = html_state.link(inherited_link);
                    push_piece(
                        output,
                        "────────".to_string(),
                        theme.horizontal_rule,
                        link.as_deref(),
                    );
                }
                html::HtmlAction::Image { source, alt } => {
                    let label = html::image_label(&source, &alt, theme);
                    push_piece(output, label, inherited.patch(theme.image), Some(&source));
                }
            },
            Inline::SoftBreak => {
                let link = html_state.link(inherited_link);
                push_piece(
                    output,
                    " ".to_string(),
                    html_state.style(inherited),
                    link.as_deref(),
                );
            }
            Inline::HardBreak => {
                let link = html_state.link(inherited_link);
                push_piece(
                    output,
                    "\n".to_string(),
                    html_state.style(inherited),
                    link.as_deref(),
                );
            }
        }
    }
}

fn push_piece(output: &mut Vec<StyledPiece>, text: String, style: Style, link: Option<&str>) {
    output.push(match link {
        Some(destination) => StyledPiece::linked(text, style, destination.to_string()),
        None => StyledPiece::new(text, style),
    });
}
