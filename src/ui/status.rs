use ratatui::text::{Line, Span};

use crate::{pdf::PdfLayoutMode, theme::Theme};

pub struct Status<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub current_line: usize,
    pub total_lines: usize,
    pub percent: usize,
    pub wrap: bool,
    pub search: Option<(usize, usize)>,
    pub image: bool,
    pub selection: bool,
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
    } else if status.selection {
        spans.push(Span::styled(" y/Ctrl+C ", theme.status_accent));
        spans.push(Span::styled("Copy ", theme.status));
        spans.push(Span::styled(" Esc ", theme.status_accent));
        spans.push(Span::styled("Clear ", theme.status));
        spans.push(Span::styled(" ? Help ", theme.status_accent));
    } else {
        spans.push(Span::styled(" ? Help ", theme.status_accent));
    }

    Line::from(spans)
}

pub fn render_pdf(
    name: &str,
    pages: &[usize],
    page_count: usize,
    layout_mode: PdfLayoutMode,
    zoom_percent: u16,
    search: Option<(usize, usize)>,
    theme: &Theme,
) -> Line<'static> {
    let page_label = match pages {
        [] => format!(" Page 1/{} ", page_count.max(1)),
        [page] => format!(" Page {}/{} ", page + 1, page_count.max(1)),
        pages => {
            let first = pages.first().copied().unwrap_or(0) + 1;
            let last = pages.last().copied().unwrap_or(0) + 1;
            format!(" Pages {first}-{last}/{} ", page_count.max(1))
        }
    };

    let mut spans = vec![
        Span::styled(format!(" {name} "), theme.status_accent),
        Span::styled(" PDF ", theme.status),
        Span::styled(page_label, theme.status),
        Span::styled(" Layout:", theme.status),
        Span::styled(format!("{} ", layout_mode.label()), theme.status_accent),
        Span::styled(" Zoom:", theme.status),
        Span::styled(
            if zoom_percent == 100 {
                "fit ".to_string()
            } else {
                format!("{zoom_percent}% ")
            },
            theme.status_accent,
        ),
    ];

    if let Some((current, total)) = search {
        spans.push(Span::styled(
            format!(" {current}/{total} matches "),
            theme.status_accent,
        ));
        spans.push(Span::styled(" n/N next/prev ", theme.status));
    }

    spans.extend([
        Span::styled(" / ", theme.status_accent),
        Span::styled("Search ", theme.status),
        Span::styled(" 1/2/a ", theme.status_accent),
        Span::styled("Layout ", theme.status),
        Span::styled(" +/- ", theme.status_accent),
        Span::styled("Zoom ", theme.status),
        Span::styled(" 0 ", theme.status_accent),
        Span::styled("Fit ", theme.status),
        Span::styled(" p/P ", theme.status_accent),
        Span::styled("Next/Prev pages ", theme.status),
        Span::styled(" ? ", theme.status_accent),
        Span::styled("Help ", theme.status),
    ]);

    Line::from(spans)
}
