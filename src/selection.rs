#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocumentPosition {
    pub line: usize,
    pub byte: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: DocumentPosition,
    pub head: DocumentPosition,
}

impl Selection {
    pub fn new(anchor: DocumentPosition, head: DocumentPosition) -> Self {
        Self { anchor, head }
    }

    pub fn normalized(self) -> (DocumentPosition, DocumentPosition) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }

    pub fn range_for_line(self, line: usize, line_len: usize) -> Option<(usize, usize)> {
        if self.is_empty() {
            return None;
        }

        let (start, end) = self.normalized();
        if line < start.line || line > end.line {
            return None;
        }

        let range_start = if line == start.line {
            start.byte.min(line_len)
        } else {
            0
        };
        let range_end = if line == end.line {
            end.byte.min(line_len)
        } else {
            line_len
        };

        (range_start < range_end).then_some((range_start, range_end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_backward_selection() {
        let selection = Selection::new(
            DocumentPosition { line: 4, byte: 8 },
            DocumentPosition { line: 2, byte: 3 },
        );

        assert_eq!(
            selection.normalized(),
            (
                DocumentPosition { line: 2, byte: 3 },
                DocumentPosition { line: 4, byte: 8 },
            )
        );
    }

    #[test]
    fn returns_ranges_across_multiple_lines() {
        let selection = Selection::new(
            DocumentPosition { line: 1, byte: 2 },
            DocumentPosition { line: 3, byte: 4 },
        );

        assert_eq!(selection.range_for_line(0, 10), None);
        assert_eq!(selection.range_for_line(1, 10), Some((2, 10)));
        assert_eq!(selection.range_for_line(2, 7), Some((0, 7)));
        assert_eq!(selection.range_for_line(3, 10), Some((0, 4)));
        assert_eq!(selection.range_for_line(4, 10), None);
    }
}
