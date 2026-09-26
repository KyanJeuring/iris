use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::theme::Theme;

const HELP_WIDTH: u16 = 76;
const HELP_HORIZONTAL_PADDING: u16 = 2;
const HELP_VERTICAL_PADDING: u16 = 1;
const HELP_SCREEN_MARGIN: u16 = 1;

#[derive(Debug, Clone, Copy)]
pub struct HelpRenderResult {
    pub area: Rect,
    pub scroll: usize,
    pub max_scroll: usize,
    pub page_height: usize,
}

pub fn render(
    frame: &mut Frame,
    theme: &Theme,
    markdown: bool,
    image: bool,
    pdf: bool,
    scroll: usize,
) -> HelpRenderResult {
    let lines = help_lines(theme, markdown, image, pdf);
    let content_height = lines.len().min(usize::from(u16::MAX)) as u16;
    let decoration_height = 2 + HELP_VERTICAL_PADDING.saturating_mul(2);
    let desired_height = content_height.saturating_add(decoration_height);
    let area = centered(frame.area(), HELP_WIDTH, desired_height);

    render_opaque_background(frame, area, theme.help);

    let block = Block::default()
        .title(" IRIS Help ")
        .borders(Borders::ALL)
        .border_style(theme.help_border)
        .style(theme.help)
        .padding(Padding::new(
            HELP_HORIZONTAL_PADDING,
            HELP_HORIZONTAL_PADDING,
            HELP_VERTICAL_PADDING,
            HELP_VERTICAL_PADDING,
        ));

    let content_area = block.inner(area);
    let page_height = usize::from(content_area.height).max(1);
    let max_scroll = lines.len().saturating_sub(page_height);
    let scroll = scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((scroll.min(usize::from(u16::MAX)) as u16, 0)),
        area,
    );

    HelpRenderResult {
        area,
        scroll,
        max_scroll,
        page_height,
    }
}

fn help_lines(theme: &Theme, markdown: bool, image: bool, pdf: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    section(&mut lines, "Navigation", theme);
    shortcut(&mut lines, "j / ↓", "Scroll down", theme);
    shortcut(&mut lines, "k / ↑", "Scroll up", theme);
    if markdown {
        shortcut(&mut lines, "← / →", "Scroll horizontally", theme);
    } else {
        shortcut(&mut lines, "h / ←", "Scroll left", theme);
        shortcut(&mut lines, "l / →", "Scroll right", theme);
    }
    shortcut(&mut lines, "Ctrl+D / Ctrl+U", "Half page down / up", theme);
    shortcut(&mut lines, "PgDn / PgUp", "Page down / up", theme);
    shortcut(&mut lines, "g / G", "Top / bottom", theme);

    if pdf {
        section(&mut lines, "PDF", theme);
        shortcut(&mut lines, "p / P", "Next / previous page row", theme);
        shortcut(&mut lines, "+ / =", "Zoom in", theme);
        shortcut(&mut lines, "-", "Zoom out", theme);
        shortcut(&mut lines, "0", "Reset zoom to fit", theme);
        shortcut(&mut lines, "1", "Force one page per row", theme);
        shortcut(&mut lines, "2", "Force two pages per row", theme);
        shortcut(&mut lines, "a", "Return to automatic page layout", theme);
        shortcut(
            &mut lines,
            "Resize",
            "Reflow automatic layout to fit",
            theme,
        );
    } else if !image {
        section(&mut lines, "Search", theme);
        shortcut(&mut lines, "/", "Search", theme);
        shortcut(&mut lines, "n / N", "Next / previous match", theme);
        shortcut(&mut lines, "Enter", "Accept search", theme);
        shortcut(&mut lines, "Esc", "Clear search / highlights", theme);
    }

    if markdown {
        section(&mut lines, "Headings & links", theme);
        shortcut(&mut lines, "h / H", "Next / previous heading", theme);
        shortcut(&mut lines, "Left click", "Open a link or image", theme);
    }

    if !image && !pdf {
        section(&mut lines, "Selection", theme);
        shortcut(&mut lines, "Mouse drag", "Select rendered text", theme);
        shortcut(&mut lines, "y / Ctrl+C", "Copy selection", theme);
        shortcut(&mut lines, "Esc", "Clear selection", theme);
    }

    if image {
        section(&mut lines, "Image", theme);
        shortcut(&mut lines, "+ / =", "Zoom in", theme);
        shortcut(&mut lines, "-", "Zoom out", theme);
        shortcut(&mut lines, "0", "Reset zoom to fit", theme);
    }

    section(&mut lines, "Display & mouse", theme);
    if !image && !pdf {
        shortcut(&mut lines, "w", "Toggle prose wrapping", theme);
    }
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

    lines
}

fn render_opaque_background(frame: &mut Frame, area: Rect, style: Style) {
    let row = " ".repeat(usize::from(area.width));
    let lines = (0..area.height)
        .map(|_| Line::from(Span::styled(row.clone(), style)))
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(lines), area);
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
    let horizontal_margin = HELP_SCREEN_MARGIN.saturating_mul(2);
    let vertical_margin = HELP_SCREEN_MARGIN.saturating_mul(2);
    let width = desired_width
        .min(area.width.saturating_sub(horizontal_margin))
        .max(1);
    let height = desired_height
        .min(area.height.saturating_sub(vertical_margin))
        .max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}
