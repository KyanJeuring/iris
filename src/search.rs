use regex::{Regex, RegexBuilder};

use crate::renderer::NodeId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub node_id: NodeId,
    pub occurrence: usize,
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
        Some(Self { regex })
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

        let matcher = SearchMatcher::new("Markdown").unwrap();
        assert_eq!(matcher.count("Markdown markdown MARKDOWN"), 1);
    }

    #[test]
    fn navigation_wraps() {
        let mut state = SearchState::default();
        state.set_matches(vec![
            SearchMatch {
                node_id: 1,
                occurrence: 0,
            },
            SearchMatch {
                node_id: 2,
                occurrence: 0,
            },
        ]);
        state.next();
        assert_eq!(state.current, Some(1));
        state.next();
        assert_eq!(state.current, Some(0));
        state.previous();
        assert_eq!(state.current, Some(1));
    }
}
