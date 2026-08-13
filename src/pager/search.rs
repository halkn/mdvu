//! Search over rendered plain text.
//!
//! Matching runs against what is on screen rather than the Markdown source, so
//! a query finds what the reader can actually see.

use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    /// Byte offsets into the rendered line's text.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Search {
    query: String,
    matches: Vec<Match>,
    current: Option<usize>,
}

impl Search {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    #[cfg(test)]
    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    pub fn current(&self) -> Option<Match> {
        self.current.and_then(|i| self.matches.get(i).copied())
    }

    pub fn count(&self) -> usize {
        self.matches.len()
    }

    /// Matches on a given rendered line, for highlighting.
    pub fn matches_on(&self, line: usize) -> impl Iterator<Item = &Match> {
        self.matches.iter().filter(move |m| m.line == line)
    }

    /// Apply a query. An empty query keeps the previous one, which is what
    /// pressing Enter on an empty search prompt should do.
    pub fn set_query(&mut self, query: &str, lines: &[String]) {
        if !query.is_empty() {
            self.query = query.to_string();
        }
        self.recompute(lines);
    }

    /// Apply a query as typed, including an empty one. Used by the prompt while
    /// it is being edited, where deleting the last character has to clear the
    /// matches rather than fall back to the previous query.
    pub fn replace_query(&mut self, query: &str, lines: &[String]) {
        self.query = query.to_string();
        self.recompute(lines);
    }

    /// Zero-based position of the current match, for the status bar.
    pub fn position(&self) -> Option<usize> {
        self.current
    }

    /// Re-run the search. Called after a re-layout, since rendered line numbers
    /// change with width. The cursor lands on the first match; callers that
    /// want it near the reader follow with `select_from`.
    pub fn recompute(&mut self, lines: &[String]) {
        self.matches.clear();
        if self.query.is_empty() {
            self.current = None;
            return;
        }
        // Smart case, as in moar and vim: a query typed in lower case matches
        // any case, and adding a capital asks for that capital.
        let fold = !self.query.chars().any(char::is_uppercase);
        let needle = if fold {
            self.query.to_lowercase()
        } else {
            self.query.clone()
        };
        for (index, line) in lines.iter().enumerate() {
            let haystack: Cow<'_, str> = if fold {
                Cow::Owned(line.to_lowercase())
            } else {
                Cow::Borrowed(line.as_str())
            };
            // Lowercasing can change byte length, so offsets are only used when
            // the mapping is one to one; otherwise the whole line highlights.
            let same_layout = haystack.len() == line.len();
            let mut from = 0usize;
            while let Some(found) = haystack[from..].find(&needle) {
                let start = from + found;
                let end = start + needle.len();
                self.matches.push(if same_layout {
                    Match {
                        line: index,
                        start,
                        end,
                    }
                } else {
                    Match {
                        line: index,
                        start: 0,
                        end: line.len(),
                    }
                });
                from = end.max(start + 1);
                if from >= haystack.len() {
                    break;
                }
            }
        }
        self.current = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Select the first match at or after `line`, so a search starts from the
    /// current viewport rather than from the top of the document.
    pub fn select_from(&mut self, line: usize) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        self.current = Some(
            self.matches
                .iter()
                .position(|m| m.line >= line)
                .unwrap_or(0),
        );
    }

    pub fn next(&mut self) -> Option<Match> {
        self.step(1)
    }

    pub fn previous(&mut self) -> Option<Match> {
        self.step(-1)
    }

    fn step(&mut self, delta: isize) -> Option<Match> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len() as isize;
        let current = self.current.unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(len) as usize;
        self.current = Some(next);
        self.matches.get(next).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<String> {
        vec![
            "The quick brown fox".to_string(),
            "jumps over".to_string(),
            "the lazy dog, the end".to_string(),
        ]
    }

    #[test]
    fn a_lower_case_query_matches_any_case() {
        let mut s = Search::default();
        s.set_query("the", &lines());
        assert_eq!(s.count(), 3);
        assert_eq!(s.matches()[0].line, 0);
    }

    #[test]
    fn a_capital_in_the_query_asks_for_that_capital() {
        let lines = vec!["The quick".to_string(), "the lazy".to_string()];
        let mut s = Search::default();
        s.set_query("The", &lines);
        assert_eq!(s.count(), 1);
        assert_eq!(s.matches()[0].line, 0);
        // Offsets still locate the match when the query is not folded.
        let m = s.matches()[0];
        assert_eq!(&lines[m.line][m.start..m.end], "The");
    }

    #[test]
    fn multiple_matches_on_one_line_are_all_found() {
        let mut s = Search::default();
        s.set_query("the", &lines());
        let on_last: Vec<_> = s.matches_on(2).collect();
        assert_eq!(on_last.len(), 2);
    }

    #[test]
    fn offsets_locate_the_match_within_the_line() {
        let mut s = Search::default();
        s.set_query("brown", &lines());
        let m = s.matches()[0];
        assert_eq!(&lines()[m.line][m.start..m.end], "brown");
    }

    #[test]
    fn navigation_cycles_in_both_directions() {
        let mut s = Search::default();
        s.set_query("the", &lines());
        assert_eq!(s.current().unwrap().line, 0);
        s.next();
        assert_eq!(s.current().unwrap().line, 2);
        s.next();
        assert_eq!(s.current().unwrap().line, 2);
        s.next();
        // Wraps back to the first match.
        assert_eq!(s.current().unwrap().line, 0);
        s.previous();
        assert_eq!(s.current().unwrap().line, 2);
    }

    #[test]
    fn an_empty_query_keeps_the_previous_one() {
        let mut s = Search::default();
        s.set_query("fox", &lines());
        s.set_query("", &lines());
        assert_eq!(s.query(), "fox");
        assert_eq!(s.count(), 1);
    }

    #[test]
    fn a_query_with_no_match_reports_zero() {
        let mut s = Search::default();
        s.set_query("absent", &lines());
        assert_eq!(s.count(), 0);
        assert!(s.current().is_none());
        assert!(s.next().is_none());
    }

    #[test]
    fn selection_starts_from_the_viewport() {
        let mut s = Search::default();
        s.set_query("the", &lines());
        s.select_from(2);
        assert_eq!(s.current().unwrap().line, 2);
    }

    #[test]
    fn japanese_queries_match() {
        let lines = vec!["日本語のテキスト".to_string(), "英語の text".to_string()];
        let mut s = Search::default();
        s.set_query("テキスト", &lines);
        assert_eq!(s.count(), 1);
        let m = s.matches()[0];
        assert_eq!(&lines[m.line][m.start..m.end], "テキスト");
    }
}
