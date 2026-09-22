use pulldown_cmark::Alignment;

use crate::{
    renderer::{HeadingId, NodeId},
    search::{SearchMatch, SearchMatcher},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

#[derive(Debug, Clone)]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Code(String),
    Link {
        text: Vec<Inline>,
        destination: String,
    },
    Image {
        alt: Vec<Inline>,
        source: String,
    },
    InlineMath(String),
    RawHtml(String),
    SoftBreak,
    HardBreak,
}

impl Inline {
    pub fn plain_text(items: &[Inline]) -> String {
        let mut output = String::new();
        for item in items {
            match item {
                Inline::Text(text) | Inline::Code(text) | Inline::InlineMath(text) => {
                    output.push_str(text)
                }
                Inline::RawHtml(_) => {}
                Inline::Strong(inner) | Inline::Emphasis(inner) | Inline::Strikethrough(inner) => {
                    output.push_str(&Inline::plain_text(inner))
                }
                Inline::Link { text, .. } => output.push_str(&Inline::plain_text(text)),
                Inline::Image { alt, .. } => output.push_str(&Inline::plain_text(alt)),
                Inline::SoftBreak => output.push(' '),
                Inline::HardBreak => output.push('\n'),
            }
        }
        output
    }
}

#[derive(Debug, Clone)]
pub struct Heading {
    pub id: HeadingId,
    pub level: u8,
    pub slug: String,
    pub content: Vec<Inline>,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub id: NodeId,
    pub checked: Option<bool>,
    pub content: Vec<Inline>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub id: NodeId,
    pub alignments: Vec<Alignment>,
    pub header: Vec<Vec<Inline>>,
    pub rows: Vec<Vec<Vec<Inline>>>,
}

#[derive(Debug, Clone)]
pub enum Block {
    Paragraph {
        id: NodeId,
        content: Vec<Inline>,
    },
    Heading(Heading),
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
    List {
        ordered_start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table(Table),
    HorizontalRule {
        id: NodeId,
    },
    DisplayMath {
        id: NodeId,
        content: String,
    },
    Html {
        id: NodeId,
        content: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn search(&self, matcher: &SearchMatcher) -> Vec<SearchMatch> {
        let mut matches = Vec::new();
        search_blocks(&self.blocks, matcher, &mut matches);
        matches
    }
}

fn search_blocks(blocks: &[Block], matcher: &SearchMatcher, matches: &mut Vec<SearchMatch>) {
    for block in blocks {
        match block {
            Block::Heading(heading) => {
                push_text_matches(
                    heading.id,
                    &Inline::plain_text(&heading.content),
                    matcher,
                    matches,
                );
            }
            Block::Paragraph { id, content } => {
                push_text_matches(*id, &Inline::plain_text(content), matcher, matches);
            }
            Block::CodeBlock { id, code, .. } | Block::DisplayMath { id, content: code } => {
                push_text_matches(*id, code, matcher, matches);
            }
            Block::Html { id, content } => {
                push_text_matches(*id, &super::html::plain_text(content), matcher, matches);
            }
            Block::BlockQuote { blocks, .. } => {
                search_blocks(blocks, matcher, matches);
            }
            Block::List { items, .. } => {
                for item in items {
                    if !item.content.is_empty() {
                        push_text_matches(
                            item.id,
                            &Inline::plain_text(&item.content),
                            matcher,
                            matches,
                        );
                    }
                    search_blocks(&item.blocks, matcher, matches);
                }
            }
            Block::Table(table) => {
                let mut text = String::new();
                for cell in &table.header {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(&Inline::plain_text(cell));
                }
                for row in &table.rows {
                    for cell in row {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(&Inline::plain_text(cell));
                    }
                }
                push_text_matches(table.id, &text, matcher, matches);
            }
            Block::HorizontalRule { .. } => {}
        }
    }
}

fn push_text_matches(
    node_id: NodeId,
    text: &str,
    matcher: &SearchMatcher,
    output: &mut Vec<SearchMatch>,
) {
    for occurrence in 0..matcher.count(text) {
        output.push(SearchMatch {
            node_id,
            occurrence,
        });
    }
}
