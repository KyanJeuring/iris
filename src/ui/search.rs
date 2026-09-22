use ratatui::text::{Line, Span};

use crate::{search::SearchState, theme::Theme};

pub fn prompt(search: &SearchState, theme: &Theme) -> Line<'static> {
    let count = if search.query.is_empty() {
        String::new()
    } else {
        format!("  {} matches", search.matches.len())
    };
    Line::from(vec![
        Span::styled("/", theme.search_prompt),
        Span::styled(search.query.clone(), theme.document),
        Span::styled("_", theme.search_prompt),
        Span::styled(count, theme.status),
    ])
}
