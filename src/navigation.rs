use std::path::PathBuf;

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::title::truncate_end;

const SEPARATOR: &str = " › ";
const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationEntry {
    pub path: PathBuf,
    pub display_name: String,
    pub vertical_scroll: usize,
    pub horizontal_scroll: usize,
}

impl NavigationEntry {
    pub fn new(path: PathBuf, display_name: String) -> Self {
        Self {
            path,
            display_name,
            vertical_scroll: 0,
            horizontal_scroll: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryDirection {
    Back,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationHistory {
    entries: Vec<NavigationEntry>,
    current: usize,
}

impl NavigationHistory {
    pub fn new(initial: NavigationEntry) -> Self {
        Self {
            entries: vec![initial],
            current: 0,
        }
    }

    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn current(&self) -> &NavigationEntry {
        &self.entries[self.current]
    }

    pub fn record_viewport(&mut self, vertical_scroll: usize, horizontal_scroll: usize) {
        let entry = &mut self.entries[self.current];
        entry.vertical_scroll = vertical_scroll;
        entry.horizontal_scroll = horizontal_scroll;
    }

    pub fn push(&mut self, entry: NavigationEntry) {
        self.entries.truncate(self.current + 1);
        self.entries.push(entry);
        self.current = self.entries.len() - 1;
    }

    pub fn target(&self, direction: HistoryDirection) -> Option<(usize, &NavigationEntry)> {
        let index = match direction {
            HistoryDirection::Back => self.current.checked_sub(1)?,
            HistoryDirection::Forward => {
                let next = self.current + 1;
                (next < self.entries.len()).then_some(next)?
            }
        };
        Some((index, &self.entries[index]))
    }

    pub fn set_current(&mut self, index: usize) {
        debug_assert!(index < self.entries.len());
        self.current = index.min(self.entries.len().saturating_sub(1));
    }
}

pub fn breadcrumb_line(
    history: &NavigationHistory,
    max_width: usize,
    normal_style: Style,
    current_style: Style,
) -> Line<'static> {
    if max_width == 0 {
        return Line::default();
    }

    let entries = history.entries();
    let current = history.current_index();
    let Some((start, end)) = best_window(entries, current, max_width) else {
        return Line::from(Span::styled(
            truncate_end(&entries[current].display_name, max_width),
            current_style,
        ));
    };

    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled(ELLIPSIS, normal_style));
        spans.push(Span::styled(SEPARATOR, normal_style));
    }
    for (index, entry) in entries.iter().enumerate().take(end + 1).skip(start) {
        if index > start {
            spans.push(Span::styled(SEPARATOR, normal_style));
        }
        let style = if index == current {
            current_style
        } else {
            normal_style
        };
        spans.push(Span::styled(entry.display_name.clone(), style));
    }
    if end + 1 < entries.len() {
        spans.push(Span::styled(SEPARATOR, normal_style));
        spans.push(Span::styled(ELLIPSIS, normal_style));
    }

    Line::from(spans)
}

fn best_window(
    entries: &[NavigationEntry],
    current: usize,
    max_width: usize,
) -> Option<(usize, usize)> {
    let mut best = None::<(usize, usize, usize, usize)>;
    let mut prefix_widths = Vec::with_capacity(entries.len() + 1);
    prefix_widths.push(0usize);
    for entry in entries {
        let width = UnicodeWidthStr::width(entry.display_name.as_str());
        prefix_widths.push(
            prefix_widths
                .last()
                .copied()
                .unwrap_or(0)
                .saturating_add(width),
        );
    }

    for start in 0..=current {
        for end in current..entries.len() {
            let width = window_width(&prefix_widths, entries.len(), start, end);
            if width > max_width {
                continue;
            }

            let count = end - start + 1;
            let left = current - start;
            let right = end - current;
            let imbalance = left.abs_diff(right);
            let score = (count, usize::MAX - imbalance, left);
            if best.is_none_or(|(_, _, best_count, best_balance)| {
                score.0 > best_count || score.0 == best_count && score.1 > best_balance
            }) {
                best = Some((start, end, score.0, score.1));
            }
        }
    }

    best.map(|(start, end, _, _)| (start, end))
}

fn window_width(prefix_widths: &[usize], entry_count: usize, start: usize, end: usize) -> usize {
    let entry_width = prefix_widths[end + 1].saturating_sub(prefix_widths[start]);
    let separators =
        end.saturating_sub(start) + usize::from(start > 0) + usize::from(end + 1 < entry_count);
    let ellipses = usize::from(start > 0) + usize::from(end + 1 < entry_count);

    entry_width
        + separators.saturating_mul(UnicodeWidthStr::width(SEPARATOR))
        + ellipses.saturating_mul(UnicodeWidthStr::width(ELLIPSIS))
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier};

    use super::*;

    fn entry(name: &str) -> NavigationEntry {
        NavigationEntry::new(PathBuf::from(name), name.to_string())
    }

    fn text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn history_appends_moves_and_truncates_forward_branch() {
        let mut history = NavigationHistory::new(entry("README.md"));
        history.record_viewport(300, 4);
        history.push(entry("guide.md"));
        history.record_viewport(20, 2);
        history.push(entry("advanced.md"));

        assert_eq!(history.entries().len(), 3);
        assert_eq!(history.target(HistoryDirection::Forward), None);
        let (back, guide) = history.target(HistoryDirection::Back).unwrap();
        assert_eq!(
            (back, guide.vertical_scroll, guide.horizontal_scroll),
            (1, 20, 2)
        );
        history.set_current(back);
        let (forward, advanced) = history.target(HistoryDirection::Forward).unwrap();
        assert_eq!(forward, 2);
        assert_eq!(advanced.display_name, "advanced.md");

        history.push(entry("other.md"));
        assert_eq!(
            history
                .entries()
                .iter()
                .map(|entry| entry.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["README.md", "guide.md", "other.md"]
        );
        assert_eq!(history.target(HistoryDirection::Forward), None);
    }

    #[test]
    fn history_allows_repeated_paths_and_noops_at_ends() {
        let mut history = NavigationHistory::new(entry("README.md"));
        assert_eq!(history.target(HistoryDirection::Back), None);
        history.push(entry("guide.md"));
        history.push(entry("README.md"));
        assert_eq!(history.entries().len(), 3);
        assert_eq!(history.target(HistoryDirection::Forward), None);
    }

    #[test]
    fn breadcrumb_styles_current_entry_and_keeps_forward_history() {
        let normal = Style::default().fg(Color::White);
        let current = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let mut history = NavigationHistory::new(entry("README.md"));
        history.push(entry("guide.md"));
        history.push(entry("advanced.md"));
        history.set_current(1);

        let line = breadcrumb_line(&history, 80, normal, current);

        assert_eq!(text(&line), "README.md › guide.md › advanced.md");
        let guide = line
            .spans
            .iter()
            .find(|span| span.content == "guide.md")
            .unwrap();
        assert_eq!(guide.style, current);
    }

    #[test]
    fn breadcrumb_truncation_preserves_current_unicode_filename() {
        let normal = Style::default();
        let current_style = Style::default().add_modifier(Modifier::BOLD);
        let mut history = NavigationHistory::new(entry("README.md"));
        history.push(entry("design.md"));
        history.push(entry("当前文档.md"));
        history.push(entry("renderer.md"));
        history.push(entry("internals.md"));
        history.set_current(2);

        let line = breadcrumb_line(&history, 31, normal, current_style);
        let rendered = text(&line);

        assert!(rendered.contains("当前文档.md"));
        assert!(rendered.contains('…'));
        assert!(UnicodeWidthStr::width(rendered.as_str()) <= 31);
    }

    #[test]
    fn single_entry_breadcrumb_truncates_safely() {
        let history = NavigationHistory::new(entry("非常に長い名前🙂.md"));
        let line = breadcrumb_line(
            &history,
            10,
            Style::default(),
            Style::default().add_modifier(Modifier::BOLD),
        );
        let rendered = text(&line);

        assert!(rendered.ends_with('…'));
        assert!(UnicodeWidthStr::width(rendered.as_str()) <= 10);
    }
}
