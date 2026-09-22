#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencedKind {
    Code,
    Mermaid,
}

pub fn classify_fenced(language: Option<&str>) -> FencedKind {
    match language.map(str::to_ascii_lowercase).as_deref() {
        Some("mermaid") => FencedKind::Mermaid,
        _ => FencedKind::Code,
    }
}
