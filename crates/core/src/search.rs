//! In-buffer text search: smart-case literal matching, per line and across the document.
//!
//! Queries never contain newlines (the TUI prompt is single-line), so all matching is
//! per-line; [`find`] walks lines without materializing the whole document.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
