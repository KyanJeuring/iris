use ratatui::text::{Line, Span};

use crate::theme::Theme;

pub struct Status<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub current_line: usize,
    pub total_lines: usize,
    pub percent: usize,
    pub wrap: bool,
    pub search: Option<(usize, usize)>,
    pub image: bool,
}

pub fn render(status: Status<'_>, theme: &Theme) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!(" {} ", status.name), theme.status_accent),
        Span::styled(format!(" {} ", status.kind), theme.status),
        Span::styled(format!(" {:>3}% ", status.percent.min(100)), theme.status),
        Span::styled(
            format!(
                " Ln {}/{} ",
                status.current_line.min(status.total_lines.max(1)),
                status.total_lines.max(1)
            ),
            theme.status,
        ),
    ];

    if !status.image {
        spans.push(Span::styled(
            if status.wrap {
                " Wrap:on "
            } else {
                " Wrap:off "
            },
            theme.status,
        ));
    }

    if let Some((current, total)) = status.search {
        spans.push(Span::styled(
            format!(" {current}/{total} matches "),
            theme.status_accent,
        ));
        spans.push(Span::styled(" n/N next/prev ", theme.status));
    }

    if status.image {
        spans.push(Span::styled(" h/j/k/l ", theme.status_accent));
        spans.push(Span::styled("Pan ", theme.status));
        spans.push(Span::styled(" +/- ", theme.status_accent));
        spans.push(Span::styled("Zoom ", theme.status));
        spans.push(Span::styled(" 0 ", theme.status_accent));
        spans.push(Span::styled("Fit ", theme.status));
        spans.push(Span::styled(" ? ", theme.status_accent));
        spans.push(Span::styled("Help ", theme.status));
    } else {
        spans.push(Span::styled(" ? Help ", theme.status_accent));
    }

    Line::from(spans)
}
