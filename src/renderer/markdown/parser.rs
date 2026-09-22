use std::collections::HashMap;

use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use super::model::{AlertKind, Block, Document, Heading, Inline, ListItem, Table};
use crate::renderer::NodeId;

enum Frame {
    Root(Vec<Block>),
    Paragraph {
        id: NodeId,
        content: Vec<Inline>,
    },
    Heading {
        id: NodeId,
        level: u8,
        explicit_slug: Option<String>,
        content: Vec<Inline>,
    },
    BlockQuote {
        id: NodeId,
        kind: Option<AlertKind>,
        blocks: Vec<Block>,
    },
    CodeBlock {
        id: NodeId,
        language: Option<String>,
        code: String,
    },
    HtmlBlock {
        id: NodeId,
        content: String,
    },
    List {
        ordered_start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item {
        id: NodeId,
        checked: Option<bool>,
        content: Vec<Inline>,
        blocks: Vec<Block>,
    },
    Table {
        id: NodeId,
        alignments: Vec<pulldown_cmark::Alignment>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    TableHead {
        cells: Vec<Vec<Inline>>,
    },
    TableRow {
        cells: Vec<Vec<Inline>>,
    },
    TableCell(Vec<Inline>),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Link {
        destination: String,
        content: Vec<Inline>,
    },
    Image {
        source: String,
        alt: Vec<Inline>,
    },
}

struct Builder {
    frames: Vec<Frame>,
    next_id: NodeId,
    slug_counts: HashMap<String, usize>,
}

impl Builder {
    fn new() -> Self {
        Self {
            frames: vec![Frame::Root(Vec::new())],
            next_id: 1,
            slug_counts: HashMap::new(),
        }
    }

    fn id(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn push_block(&mut self, block: Block) {
        match self.frames.last_mut() {
            Some(Frame::Root(blocks))
            | Some(Frame::BlockQuote { blocks, .. })
            | Some(Frame::Item { blocks, .. }) => blocks.push(block),
            _ => {}
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        match self.frames.last_mut() {
            Some(Frame::Paragraph { content, .. })
            | Some(Frame::Heading { content, .. })
            | Some(Frame::Strong(content))
            | Some(Frame::Emphasis(content))
            | Some(Frame::Strikethrough(content))
            | Some(Frame::TableCell(content))
            | Some(Frame::Item { content, .. }) => content.push(inline),
            Some(Frame::Link { content, .. }) => content.push(inline),
            Some(Frame::Image { alt, .. }) => alt.push(inline),
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        match self.frames.last_mut() {
            Some(Frame::CodeBlock { code, .. }) => code.push_str(text),
            Some(Frame::HtmlBlock { content, .. }) => content.push_str(text),
            _ => self.push_inline(Inline::Text(text.to_string())),
        }
    }

    fn set_task_marker(&mut self, checked: bool) {
        for frame in self.frames.iter_mut().rev() {
            if let Frame::Item { checked: state, .. } = frame {
                *state = Some(checked);
                break;
            }
        }
    }

    fn unique_slug(&mut self, requested: Option<String>, title: &str) -> String {
        let base = requested
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| slugify(title));
        let count = self.slug_counts.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{}", *count)
        };
        *count += 1;
        slug
    }

    fn finish(mut self) -> Document {
        match self.frames.pop() {
            Some(Frame::Root(blocks)) => Document { blocks },
            _ => Document::default(),
        }
    }
}

pub fn parse(source: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let mut builder = Builder::new();

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(tag) => start_tag(&mut builder, tag),
            Event::End(tag) => end_tag(&mut builder, tag),
            Event::Text(text) => builder.text(&text),
            Event::Code(code) => builder.push_inline(Inline::Code(code.into_string())),
            Event::InlineMath(math) => builder.push_inline(Inline::InlineMath(math.into_string())),
            Event::DisplayMath(math) => {
                if accepts_inline(builder.frames.last()) {
                    builder.push_inline(Inline::Code(format!("$${}$$", math)));
                } else {
                    let id = builder.id();
                    builder.push_block(Block::DisplayMath {
                        id,
                        content: math.into_string(),
                    });
                }
            }
            Event::Html(html) => builder.text(&html),
            Event::InlineHtml(html) => builder.push_inline(Inline::RawHtml(html.into_string())),
            Event::FootnoteReference(label) => {
                builder.push_inline(Inline::Text(format!("[^{label}]")))
            }
            Event::SoftBreak => builder.push_inline(Inline::SoftBreak),
            Event::HardBreak => builder.push_inline(Inline::HardBreak),
            Event::Rule => {
                let id = builder.id();
                builder.push_block(Block::HorizontalRule { id });
            }
            Event::TaskListMarker(checked) => builder.set_task_marker(checked),
        }
    }

    builder.finish()
}

fn start_tag(builder: &mut Builder, tag: Tag<'_>) {
    match tag {
        Tag::Paragraph => {
            let id = builder.id();
            builder.frames.push(Frame::Paragraph {
                id,
                content: Vec::new(),
            });
        }
        Tag::Heading { level, id, .. } => {
            let node_id = builder.id();
            builder.frames.push(Frame::Heading {
                id: node_id,
                level: heading_level(level),
                explicit_slug: id.map(|s| s.into_string()),
                content: Vec::new(),
            });
        }
        Tag::BlockQuote(kind) => {
            let id = builder.id();
            builder.frames.push(Frame::BlockQuote {
                id,
                kind: kind.map(alert_kind),
                blocks: Vec::new(),
            });
        }
        Tag::CodeBlock(kind) => {
            let id = builder.id();
            let language = match kind {
                CodeBlockKind::Indented => None,
                CodeBlockKind::Fenced(info) => info
                    .split_whitespace()
                    .next()
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned),
            };
            builder.frames.push(Frame::CodeBlock {
                id,
                language,
                code: String::new(),
            });
        }
        Tag::HtmlBlock => {
            let id = builder.id();
            builder.frames.push(Frame::HtmlBlock {
                id,
                content: String::new(),
            });
        }
        Tag::List(start) => {
            builder.frames.push(Frame::List {
                ordered_start: start,
                items: Vec::new(),
            });
        }
        Tag::Item => {
            let id = builder.id();
            builder.frames.push(Frame::Item {
                id,
                checked: None,
                content: Vec::new(),
                blocks: Vec::new(),
            });
        }
        Tag::Table(alignments) => {
            let id = builder.id();
            builder.frames.push(Frame::Table {
                id,
                alignments,
                header: Vec::new(),
                rows: Vec::new(),
            });
        }
        Tag::TableHead => builder.frames.push(Frame::TableHead { cells: Vec::new() }),
        Tag::TableRow => builder.frames.push(Frame::TableRow { cells: Vec::new() }),
        Tag::TableCell => builder.frames.push(Frame::TableCell(Vec::new())),
        Tag::Strong => builder.frames.push(Frame::Strong(Vec::new())),
        Tag::Emphasis => builder.frames.push(Frame::Emphasis(Vec::new())),
        Tag::Strikethrough => builder.frames.push(Frame::Strikethrough(Vec::new())),
        Tag::Link { dest_url, .. } => builder.frames.push(Frame::Link {
            destination: dest_url.into_string(),
            content: Vec::new(),
        }),
        Tag::Image { dest_url, .. } => builder.frames.push(Frame::Image {
            source: dest_url.into_string(),
            alt: Vec::new(),
        }),
        Tag::FootnoteDefinition(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::Superscript
        | Tag::Subscript
        | Tag::MetadataBlock(_) => {}
    }
}

fn end_tag(builder: &mut Builder, tag: TagEnd) {
    match tag {
        TagEnd::Paragraph => {
            if let Some(Frame::Paragraph { id, content }) = builder.frames.pop() {
                builder.push_block(Block::Paragraph { id, content });
            }
        }
        TagEnd::Heading(_) => {
            if let Some(Frame::Heading {
                id,
                level,
                explicit_slug,
                content,
            }) = builder.frames.pop()
            {
                let title = Inline::plain_text(&content);
                let slug = builder.unique_slug(explicit_slug, &title);
                builder.push_block(Block::Heading(Heading {
                    id,
                    level,
                    slug,
                    content,
                }));
            }
        }
        TagEnd::BlockQuote(_) => {
            if let Some(Frame::BlockQuote { id, kind, blocks }) = builder.frames.pop() {
                builder.push_block(Block::BlockQuote { id, kind, blocks });
            }
        }
        TagEnd::CodeBlock => {
            if let Some(Frame::CodeBlock { id, language, code }) = builder.frames.pop() {
                builder.push_block(Block::CodeBlock {
                    id,
                    language,
                    code: code.trim_end_matches('\n').to_string(),
                });
            }
        }
        TagEnd::HtmlBlock => {
            if let Some(Frame::HtmlBlock { id, content }) = builder.frames.pop() {
                builder.push_block(Block::Html {
                    id,
                    content: content.trim_end_matches('\n').to_string(),
                });
            }
        }
        TagEnd::List(_) => {
            if let Some(Frame::List {
                ordered_start,
                items,
            }) = builder.frames.pop()
            {
                builder.push_block(Block::List {
                    ordered_start,
                    items,
                });
            }
        }
        TagEnd::Item => {
            if let Some(Frame::Item {
                id,
                checked,
                content,
                blocks,
            }) = builder.frames.pop()
                && let Some(Frame::List { items, .. }) = builder.frames.last_mut()
            {
                items.push(ListItem {
                    id,
                    checked,
                    content,
                    blocks,
                });
            }
        }
        TagEnd::Table => {
            if let Some(Frame::Table {
                id,
                alignments,
                header,
                rows,
            }) = builder.frames.pop()
            {
                builder.push_block(Block::Table(Table {
                    id,
                    alignments,
                    header,
                    rows,
                }));
            }
        }
        TagEnd::TableHead => {
            if let Some(Frame::TableHead { cells }) = builder.frames.pop()
                && let Some(Frame::Table { header, .. }) = builder.frames.last_mut()
            {
                *header = cells;
            }
        }
        TagEnd::TableRow => {
            if let Some(Frame::TableRow { cells }) = builder.frames.pop() {
                match builder.frames.last_mut() {
                    Some(Frame::Table { rows, .. }) => rows.push(cells),
                    Some(Frame::TableHead { cells: head }) => *head = cells,
                    _ => {}
                }
            }
        }
        TagEnd::TableCell => {
            if let Some(Frame::TableCell(content)) = builder.frames.pop() {
                match builder.frames.last_mut() {
                    Some(Frame::TableHead { cells }) | Some(Frame::TableRow { cells }) => {
                        cells.push(content)
                    }
                    _ => {}
                }
            }
        }
        TagEnd::Strong => close_inline(builder, Inline::Strong, |frame| match frame {
            Frame::Strong(v) => Some(v),
            _ => None,
        }),
        TagEnd::Emphasis => close_inline(builder, Inline::Emphasis, |frame| match frame {
            Frame::Emphasis(v) => Some(v),
            _ => None,
        }),
        TagEnd::Strikethrough => {
            close_inline(builder, Inline::Strikethrough, |frame| match frame {
                Frame::Strikethrough(v) => Some(v),
                _ => None,
            })
        }
        TagEnd::Link => {
            if let Some(Frame::Link {
                destination,
                content,
            }) = builder.frames.pop()
            {
                builder.push_inline(Inline::Link {
                    text: content,
                    destination,
                });
            }
        }
        TagEnd::Image => {
            if let Some(Frame::Image { source, alt }) = builder.frames.pop() {
                builder.push_inline(Inline::Image { alt, source });
            }
        }
        TagEnd::FootnoteDefinition
        | TagEnd::DefinitionList
        | TagEnd::DefinitionListTitle
        | TagEnd::DefinitionListDefinition
        | TagEnd::Superscript
        | TagEnd::Subscript
        | TagEnd::MetadataBlock(_) => {}
    }
}

fn close_inline<F, X>(builder: &mut Builder, make: F, extract: X)
where
    F: FnOnce(Vec<Inline>) -> Inline,
    X: FnOnce(Frame) -> Option<Vec<Inline>>,
{
    if let Some(frame) = builder.frames.pop()
        && let Some(content) = extract(frame)
    {
        builder.push_inline(make(content));
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn alert_kind(kind: BlockQuoteKind) -> AlertKind {
    match kind {
        BlockQuoteKind::Note => AlertKind::Note,
        BlockQuoteKind::Tip => AlertKind::Tip,
        BlockQuoteKind::Important => AlertKind::Important,
        BlockQuoteKind::Warning => AlertKind::Warning,
        BlockQuoteKind::Caution => AlertKind::Caution,
    }
}

fn accepts_inline(frame: Option<&Frame>) -> bool {
    matches!(
        frame,
        Some(
            Frame::Paragraph { .. }
                | Frame::Heading { .. }
                | Frame::TableCell(_)
                | Frame::Item { .. }
                | Frame::Strong(_)
                | Frame::Emphasis(_)
                | Frame::Strikethrough(_)
                | Frame::Link { .. }
                | Frame::Image { .. }
        )
    )
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;

    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch);
            pending_dash = false;
        } else if !slug.is_empty() {
            pending_dash = true;
        }
    }

    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_lists_tables_and_alerts() {
        let source = r#"
# Title

- [x] done
- [ ] todo

| A | B |
| - | - |
| 1 | 2 |

> [!IMPORTANT]
> Keep this.
"#;
        let document = parse(source);
        assert!(matches!(document.blocks[0], Block::Heading(_)));
        assert!(
            document
                .blocks
                .iter()
                .any(|b| matches!(b, Block::List { .. }))
        );
        assert!(document.blocks.iter().any(|b| matches!(b, Block::Table(_))));
        assert!(document.blocks.iter().any(|b| matches!(
            b,
            Block::BlockQuote {
                kind: Some(AlertKind::Important),
                ..
            }
        )));
    }

    #[test]
    fn deduplicates_heading_slugs() {
        let document = parse("# Same\n\n# Same\n");
        let slugs = document
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading(h) => Some(h.slug.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(slugs, vec!["same", "same-1"]);
    }
}
