use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;

pub fn render(frame: &mut Frame, theme: &Theme, markdown: bool) {
    let area = centered(frame.area(), 76, if markdown { 38 } else { 30 });
    frame.render_widget(Clear, area);

    let mut lines = Vec::new();
    section(&mut lines, "Navigation", theme);
    shortcut(&mut lines, "j / ↓", "Scroll down", theme);
    shortcut(&mut lines, "k / ↑", "Scroll up", theme);
    shortcut(&mut lines, "h / ←", "Scroll left", theme);
    shortcut(&mut lines, "l / →", "Scroll right", theme);
    shortcut(&mut lines, "PgDn / PgUp", "Page down / up", theme);
    shortcut(&mut lines, "g / G", "Top / bottom", theme);

    section(&mut lines, "Search", theme);
    shortcut(&mut lines, "/", "Search", theme);
    shortcut(&mut lines, "n / N", "Next / previous match", theme);
    shortcut(&mut lines, "Enter", "Accept search", theme);
    shortcut(&mut lines, "Esc", "Clear search / highlights", theme);

    if markdown {
        section(&mut lines, "Headings & links", theme);
        shortcut(&mut lines, "]h / [h", "Next / previous heading", theme);
        shortcut(&mut lines, "Left click", "Open a link or image", theme);
    }

    section(&mut lines, "Display & mouse", theme);
    shortcut(&mut lines, "w", "Toggle prose wrapping", theme);
    shortcut(&mut lines, "Wheel", "Scroll vertically", theme);
    shortcut(&mut lines, "Shift+Wheel", "Scroll horizontally", theme);
    shortcut(
        &mut lines,
        "Trackpad",
        "Scroll vertically / horizontally",
        theme,
    );

    section(&mut lines, "General", theme);
    shortcut(&mut lines, "? / Esc", "Close help", theme);
    shortcut(&mut lines, "q", "Quit IRIS", theme);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press ? or Esc to close",
        theme.status,
    )));

    let block = Block::default()
        .title(" IRIS Help ")
        .borders(Borders::ALL)
        .border_style(theme.help_border)
        .style(theme.help);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn section(lines: &mut Vec<Line<'static>>, title: &str, theme: &Theme) {
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        title.to_string(),
        theme.help_heading,
    )));
}

fn shortcut(lines: &mut Vec<Line<'static>>, key: &str, description: &str, theme: &Theme) {
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{key:<18}"), theme.help_key),
        Span::styled(description.to_string(), theme.help),
    ]));
}

fn centered(area: Rect, desired_width: u16, desired_height: u16) -> Rect {
    let width = desired_width.min(area.width.saturating_sub(2)).max(1);
    let height = desired_height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}
