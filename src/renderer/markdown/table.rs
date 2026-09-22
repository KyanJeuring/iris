use pulldown_cmark::Alignment;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use super::wrap::{StyledPiece, wrap_pieces};
use crate::{
    renderer::{NodeId, RenderedLine, RenderedLink},
    theme::Theme,
};

#[derive(Debug, Default)]
pub struct TableOutput {
    pub lines: Vec<RenderedLine>,
    pub links: Vec<RenderedLink>,
}

pub fn render(
    source_id: NodeId,
    header: Vec<Vec<StyledPiece>>,
    rows: Vec<Vec<Vec<StyledPiece>>>,
    alignments: &[Alignment],
    width: usize,
    theme: &Theme,
    tab_width: usize,
) -> TableOutput {
    let columns = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return TableOutput::default();
    }

    let mut natural = vec![3usize; columns];
    for (index, cell) in header.iter().enumerate() {
        natural[index] = natural[index].max(piece_width(cell));
    }
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            natural[index] = natural[index].max(piece_width(cell));
        }
    }

    let border_width = columns + 1;
    let padding_width = columns * 2;
    let available = width
        .saturating_sub(border_width + padding_width)
        .max(columns * 3);
    shrink_columns(&mut natural, available);

    let mut output = TableOutput::default();
    output
        .lines
        .push(border_line(source_id, '┌', '┬', '┐', &natural, theme));

    if !header.is_empty() {
        append_row(
            &mut output,
            source_id,
            &header,
            &natural,
            alignments,
            theme,
            tab_width,
        );
        output
            .lines
            .push(border_line(source_id, '├', '┼', '┤', &natural, theme));
    }

    for (row_index, row) in rows.iter().enumerate() {
        append_row(
            &mut output,
            source_id,
            row,
            &natural,
            alignments,
            theme,
            tab_width,
        );
        if row_index + 1 < rows.len() {
            output
                .lines
                .push(border_line(source_id, '├', '┼', '┤', &natural, theme));
        }
    }

    output
        .lines
        .push(border_line(source_id, '└', '┴', '┘', &natural, theme));
    output
}

fn append_row(
    output: &mut TableOutput,
    source_id: NodeId,
    cells: &[Vec<StyledPiece>],
    widths: &[usize],
    alignments: &[Alignment],
    theme: &Theme,
    tab_width: usize,
) {
    let rendered_cells = widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            let pieces = cells.get(index).cloned().unwrap_or_default();
            wrap_pieces(&pieces, *width, true, tab_width)
        })
        .collect::<Vec<_>>();
    let height = rendered_cells.iter().map(Vec::len).max().unwrap_or(1);

    for row_line in 0..height {
        let mut spans = Vec::new();
        let mut plain = String::new();
        let mut column = 0usize;

        spans.push(Span::styled("│".to_string(), theme.table_border));
        plain.push('│');
        column += 1;

        for (index, width) in widths.iter().enumerate() {
            spans.push(Span::styled(" ".to_string(), theme.table));
            plain.push(' ');
            column += 1;

            let line = rendered_cells[index]
                .get(row_line)
                .cloned()
                .unwrap_or_default();
            let alignment = alignments.get(index).copied().unwrap_or(Alignment::None);
            let content_width = UnicodeWidthStr::width(line.plain.as_str());
            let remaining = width.saturating_sub(content_width);
            let (left_pad, right_pad) = match alignment {
                Alignment::Right => (remaining, 0),
                Alignment::Center => (remaining / 2, remaining - (remaining / 2)),
                Alignment::Left | Alignment::None => (0, remaining),
            };

            if left_pad > 0 {
                let pad = " ".repeat(left_pad);
                spans.push(Span::styled(pad.clone(), theme.table));
                plain.push_str(&pad);
                column += left_pad;
            }

            let content_start = column;
            for span in line.spans {
                spans.push(Span::styled(span.content.into_owned(), span.style));
            }
            plain.push_str(&line.plain);
            column += content_width;

            for link in line.links {
                output.links.push(RenderedLink {
                    source_id,
                    destination: link.destination,
                    line: output.lines.len(),
                    start_column: content_start + link.start_column,
                    end_column: content_start + link.end_column,
                });
            }

            if right_pad > 0 {
                let pad = " ".repeat(right_pad);
                spans.push(Span::styled(pad.clone(), theme.table));
                plain.push_str(&pad);
                column += right_pad;
            }

            spans.push(Span::styled(" ".to_string(), theme.table));
            spans.push(Span::styled("│".to_string(), theme.table_border));
            plain.push(' ');
            plain.push('│');
            column += 2;
        }

        output.lines.push(RenderedLine {
            line: ratatui::text::Line::from(spans),
            plain,
            source_id: Some(source_id),
        });
    }
}

fn border_line(
    _source_id: NodeId,
    left: char,
    joint: char,
    right: char,
    widths: &[usize],
    theme: &Theme,
) -> RenderedLine {
    let mut text = String::new();
    text.push(left);
    for (index, width) in widths.iter().enumerate() {
        text.push_str(&"─".repeat(width + 2));
        text.push(if index + 1 == widths.len() {
            right
        } else {
            joint
        });
    }
    RenderedLine {
        line: ratatui::text::Line::from(Span::styled(text.clone(), theme.table_border)),
        plain: text,
        source_id: None,
    }
}

fn piece_width(pieces: &[StyledPiece]) -> usize {
    pieces
        .iter()
        .map(|piece| UnicodeWidthStr::width(piece.text.as_str()))
        .sum::<usize>()
        .clamp(3, 80)
}

fn shrink_columns(widths: &mut [usize], available: usize) {
    let minimum = 3usize;
    while widths.iter().sum::<usize>() > available {
        let Some((index, _)) = widths
            .iter()
            .enumerate()
            .filter(|(_, width)| **width > minimum)
            .max_by_key(|(_, width)| **width)
        else {
            break;
        };
        widths[index] -= 1;
    }
}
