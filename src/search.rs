use regex::{Regex, RegexBuilder};

use crate::renderer::NodeId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SearchRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center_x(self) -> f32 {
        self.x + self.width / 2.0
    }

    pub fn center_y(self) -> f32 {
        self.y + self.height / 2.0
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(x, y, right - x, bottom - y)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchLocation {
    Text {
        node_id: NodeId,
        occurrence: usize,
    },
    Pdf {
        page_index: usize,
        rects: Vec<SearchRect>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchMatch {
    pub location: SearchLocation,
}

impl SearchMatch {
    pub fn text(node_id: NodeId, occurrence: usize) -> Self {
        Self {
            location: SearchLocation::Text {
                node_id,
                occurrence,
            },
        }
    }

    pub fn pdf(page_index: usize, rects: Vec<SearchRect>) -> Self {
        Self {
            location: SearchLocation::Pdf { page_index, rects },
        }
    }

    pub fn text_location(&self) -> Option<(NodeId, usize)> {
        match &self.location {
            SearchLocation::Text {
                node_id,
                occurrence,
            } => Some((*node_id, *occurrence)),
            SearchLocation::Pdf { .. } => None,
        }
    }

    pub fn pdf_location(&self) -> Option<(usize, &[SearchRect])> {
        match &self.location {
            SearchLocation::Pdf { page_index, rects } => Some((*page_index, rects)),
            SearchLocation::Text { .. } => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct SearchState {
    pub active: bool,
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current: Option<usize>,
}

impl SearchState {
    pub fn open(&mut self) {
        self.active = true;
    }

    pub fn cancel_input(&mut self) {
        self.active = false;
    }

    pub fn accept_input(&mut self) {
        self.active = false;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = None;
    }

    pub fn set_matches(&mut self, matches: Vec<SearchMatch>) {
        self.matches = matches;
        self.current = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn next(&mut self) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        self.current = Some(match self.current {
            Some(index) => (index + 1) % self.matches.len(),
            None => 0,
        });
    }

    pub fn previous(&mut self) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        self.current = Some(match self.current {
            Some(0) | None => self.matches.len() - 1,
            Some(index) => index - 1,
        });
    }

    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current.and_then(|index| self.matches.get(index))
    }

    pub fn position_label(&self) -> Option<(usize, usize)> {
        self.current.map(|index| (index + 1, self.matches.len()))
    }
}

#[derive(Debug, Clone)]
pub struct SearchMatcher {
    query: String,
    case_sensitive: bool,
    regex: Regex,
}

impl SearchMatcher {
    pub fn new(query: &str) -> Option<Self> {
        if query.is_empty() {
            return None;
        }

        let case_sensitive = query.chars().any(char::is_uppercase);
        let regex = RegexBuilder::new(&regex::escape(query))
            .case_insensitive(!case_sensitive)
            .build()
            .expect("escaped search queries always compile");
        Some(Self {
            query: query.to_string(),
            case_sensitive,
            regex,
        })
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub fn count(&self, text: &str) -> usize {
        self.regex.find_iter(text).count()
    }

    pub fn ranges(&self, text: &str) -> Vec<(usize, usize)> {
        self.regex
            .find_iter(text)
            .map(|m| (m.start(), m.end()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_uses_smart_case() {
        let matcher = SearchMatcher::new("markdown").unwrap();
        assert_eq!(matcher.count("Markdown markdown MARKDOWN"), 3);
        assert!(!matcher.case_sensitive());

        let matcher = SearchMatcher::new("Markdown").unwrap();
        assert_eq!(matcher.count("Markdown markdown MARKDOWN"), 1);
        assert!(matcher.case_sensitive());
    }

    #[test]
    fn navigation_wraps() {
        let mut state = SearchState::default();
        state.set_matches(vec![SearchMatch::text(1, 0), SearchMatch::text(2, 0)]);
        state.next();
        assert_eq!(state.current, Some(1));
        state.next();
        assert_eq!(state.current, Some(0));
        state.previous();
        assert_eq!(state.current, Some(1));
    }

    #[test]
    fn rect_union_covers_both_rectangles() {
        let union =
            SearchRect::new(0.1, 0.2, 0.1, 0.1).union(SearchRect::new(0.18, 0.18, 0.2, 0.15));
        assert!((union.x - 0.1).abs() < f32::EPSILON);
        assert!((union.y - 0.18).abs() < f32::EPSILON);
        assert!((union.right() - 0.38).abs() < f32::EPSILON);
        assert!((union.bottom() - 0.33).abs() < f32::EPSILON);
    }
}
