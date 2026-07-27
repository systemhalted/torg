//! In-buffer text search: smart-case literal matching, per line and across the document.
//!
//! Queries never contain newlines (the TUI prompt is single-line), so all matching is
//! per-line; [`find`] walks lines without materializing the whole document.

use crate::document::Document;

/// A match position: `line` and char `col` of the first query char.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub col: usize,
}

/// Case-fold one char the simple way (first char of its lowercase expansion).
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Smart case: a query with any uppercase char matches exactly; otherwise both sides fold.
fn smart_eq(a: char, b: char, exact: bool) -> bool {
    if exact {
        a == b
    } else {
        fold(a) == fold(b)
    }
}

/// Char cols of every occurrence of `query` in `text` (smart case, overlaps included).
pub fn matches_in_line(text: &str, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let exact = query.chars().any(|c| c.is_uppercase());
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = query.chars().collect();
    if needle.len() > hay.len() {
        return Vec::new();
    }
    (0..=hay.len() - needle.len())
        .filter(|&start| {
            needle
                .iter()
                .zip(&hay[start..])
                .all(|(&q, &h)| smart_eq(h, q, exact))
        })
        .collect()
}

/// First match at-or-after (`forward`) / at-or-before (backward) `from = (line, col)`,
/// wrapping around the document once. The `bool` is true when the result wrapped.
/// `None` for an empty query or no match anywhere.
pub fn find(
    doc: &Document,
    query: &str,
    from: (usize, usize),
    forward: bool,
) -> Option<(Match, bool)> {
    if query.is_empty() {
        return None;
    }
    let last = doc.line_count().saturating_sub(1);
    let (from_line, from_col) = (from.0.min(last), from.1);

    // On the anchor line the match must sit at-or-after (forward) / at-or-before
    // (backward) `from_col`; swept lines have no bound.
    let hit = |line: usize, bound: Option<usize>| -> Option<Match> {
        let cols = matches_in_line(&doc.line_text(line), query);
        let col = match bound {
            Some(b) if forward => cols.into_iter().find(|&c| c >= b),
            Some(b) => cols.into_iter().rev().find(|&c| c <= b),
            None if forward => cols.first().copied(),
            None => cols.last().copied(),
        }?;
        Some(Match { line, col })
    };

    if forward {
        if let Some(m) = hit(from_line, Some(from_col)) {
            return Some((m, false));
        }
        for line in from_line + 1..=last {
            if let Some(m) = hit(line, None) {
                return Some((m, false));
            }
        }
        for line in 0..=from_line {
            if let Some(m) = hit(line, None) {
                return Some((m, true));
            }
        }
    } else {
        if let Some(m) = hit(from_line, Some(from_col)) {
            return Some((m, false));
        }
        for line in (0..from_line).rev() {
            if let Some(m) = hit(line, None) {
                return Some((m, false));
            }
        }
        for line in (from_line..=last).rev() {
            if let Some(m) = hit(line, None) {
                return Some((m, true));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::from_text(text)
    }

    #[test]
    fn lowercase_query_matches_case_insensitively() {
        assert_eq!(matches_in_line("Foo foo FOO", "foo"), vec![0, 4, 8]);
    }

    #[test]
    fn uppercase_in_query_forces_exact_match() {
        assert_eq!(matches_in_line("Foo foo FOO", "Foo"), vec![0]);
    }

    #[test]
    fn cols_are_char_indices_not_bytes() {
        // "héllo héllo": second occurrence starts at char 6.
        assert_eq!(matches_in_line("héllo héllo", "héllo"), vec![0, 6]);
    }

    #[test]
    fn overlapping_occurrences_are_all_reported() {
        assert_eq!(matches_in_line("aaa", "aa"), vec![0, 1]);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert_eq!(matches_in_line("anything", ""), Vec::<usize>::new());
    }

    #[test]
    fn query_longer_than_the_line_matches_nothing() {
        assert_eq!(matches_in_line("hi", "hello"), Vec::<usize>::new());
    }

    #[test]
    fn forward_finds_the_first_match_at_or_after_from() {
        let d = doc("alpha\nbeta alpha\ngamma\n");
        let (m, wrapped) = find(&d, "alpha", (0, 0), true).unwrap();
        assert_eq!((m.line, m.col, wrapped), (0, 0, false));
        let (m, wrapped) = find(&d, "alpha", (0, 1), true).unwrap();
        assert_eq!((m.line, m.col, wrapped), (1, 5, false));
    }

    #[test]
    fn forward_wraps_once_and_reports_it() {
        let d = doc("alpha\nbeta\ngamma\n");
        let (m, wrapped) = find(&d, "alpha", (1, 0), true).unwrap();
        assert_eq!((m.line, m.col, wrapped), (0, 0, true));
    }

    #[test]
    fn backward_finds_the_first_match_at_or_before_from() {
        let d = doc("alpha\nbeta alpha\ngamma\n");
        let (m, wrapped) = find(&d, "alpha", (1, 9), false).unwrap();
        assert_eq!((m.line, m.col, wrapped), (1, 5, false));
        let (m, wrapped) = find(&d, "alpha", (1, 4), false).unwrap();
        assert_eq!((m.line, m.col, wrapped), (0, 0, false));
    }

    #[test]
    fn backward_wraps_once_and_reports_it() {
        let d = doc("alpha\nbeta alpha\ngamma\n");
        let (m, wrapped) = find(&d, "gamma", (0, 0), false).unwrap();
        assert_eq!((m.line, m.col, wrapped), (2, 0, true));
    }

    #[test]
    fn no_match_returns_none() {
        let d = doc("alpha\nbeta\n");
        assert!(find(&d, "zeta", (0, 0), true).is_none());
        assert!(find(&d, "", (0, 0), true).is_none());
    }

    #[test]
    fn match_on_the_last_line_without_trailing_newline_is_found() {
        let d = doc("alpha\nomega");
        let (m, wrapped) = find(&d, "omega", (0, 0), true).unwrap();
        assert_eq!((m.line, m.col, wrapped), (1, 0, false));
    }

    #[test]
    fn forward_wrap_back_onto_the_anchor_line_reports_wrapped() {
        // Single line, only match before `from_col`: found via the wrap sweep.
        let d = doc("alpha zzz");
        let (m, wrapped) = find(&d, "alpha", (0, 3), true).unwrap();
        assert_eq!((m.line, m.col, wrapped), (0, 0, true));
    }

    #[test]
    fn backward_wrap_back_onto_the_anchor_line_reports_wrapped() {
        // Single line, only match after `from_col`: found via the wrap sweep.
        let d = doc("zzz alpha");
        let (m, wrapped) = find(&d, "alpha", (0, 2), false).unwrap();
        assert_eq!((m.line, m.col, wrapped), (0, 4, true));
    }
}
