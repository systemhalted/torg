//! The headless cursor + editing-intent model — gedit's `GeditView` / insert-mark
//! over the buffer, with no terminal or windowing dependency.
//!
//! A [`View`] holds where the caret is and knows how to move and edit, but it does
//! **not** own the text: every method borrows a [`Document`] (`&Document` to read line
//! geometry, `&mut Document` to edit). The owning `App` holds both. This keeps the model
//! layer pure and fully unit-testable, and lets the same `View` drive any frontend.
//!
//! All positions are **character** units, never bytes — ropey is char-indexed, so a `'é'`
//! is one column, not two. The flat char index a `Document` edit needs is derived on demand
//! (`line_to_char(line) + column`), never stored, so it can never drift out of sync.

use crate::document::Document;

/// A cursor over a [`Document`]: a `(line, column)` position plus a remembered
/// *goal column* (gedit's preferred-x).
///
/// Vertical moves try to land on `goal_column` so the caret returns to its original
/// column after passing over shorter lines; horizontal moves and edits reset the goal to
/// the current column. An internal `clamp` runs after every operation, so the
/// caret can never sit past the end of the buffer or past a line's last character.
#[derive(Debug, Clone, Default)]
pub struct View {
    line: usize,
    column: usize,
    goal_column: usize,
}

impl View {
    /// A fresh cursor at the top-left of the buffer, `(0, 0)` with goal column 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// The line the caret is on (0-based).
    pub fn cursor_line(&self) -> usize {
        self.line
    }

    /// The column the caret is on (0-based, in characters).
    pub fn cursor_column(&self) -> usize {
        self.column
    }

    /// The flat character index of the caret within `doc`, suitable for
    /// [`Document::insert`] / [`Document::remove`]. Derived, never stored.
    pub fn cursor_char_idx(&self, doc: &Document) -> usize {
        doc.line_to_char(self.line) + self.column
    }

    // ---- internals --------------------------------------------------------

    /// The last line index the caret may rest on. ropey reports a trailing empty
    /// line for a buffer ending in `\n` (e.g. `"a\nb\n"` has 3 lines); the caret may
    /// sit on it but no further.
    fn last_line(doc: &Document) -> usize {
        doc.line_count().saturating_sub(1)
    }

    /// Set the column *and* reset the goal column to it. Used by horizontal moves and
    /// edits — anything that should make the new column "sticky" for later vertical moves.
    fn set_column(&mut self, column: usize) {
        self.column = column;
        self.goal_column = column;
    }

    /// Pull the caret back inside the buffer after any move or edit: never past the last
    /// line, never past the current line's last character.
    fn clamp(&mut self, doc: &Document) {
        let last = Self::last_line(doc);
        if self.line > last {
            self.line = last;
        }
        let max_col = doc.line_len_chars(self.line);
        if self.column > max_col {
            self.column = max_col;
        }
    }

    // ---- horizontal movement (reset goal column) --------------------------

    /// Move one character left; at column 0, wrap to the end of the previous line.
    /// No-op at the very start of the buffer.
    pub fn move_left(&mut self, doc: &Document) {
        if self.column > 0 {
            self.set_column(self.column - 1);
        } else if self.line > 0 {
            self.line -= 1;
            self.set_column(doc.line_len_chars(self.line));
        }
        self.clamp(doc);
    }

    /// Move one character right; at a line's end, wrap to the start of the next line.
    /// No-op at the very end of the buffer.
    pub fn move_right(&mut self, doc: &Document) {
        if self.column < doc.line_len_chars(self.line) {
            self.set_column(self.column + 1);
        } else if self.line < Self::last_line(doc) {
            self.line += 1;
            self.set_column(0);
        }
        self.clamp(doc);
    }

    /// Move to the start of the current line.
    pub fn move_home(&mut self) {
        self.set_column(0);
    }

    /// Move to the end of the current line (its last character, newline excluded).
    pub fn move_end(&mut self, doc: &Document) {
        self.set_column(doc.line_len_chars(self.line));
        self.clamp(doc);
    }

    // ---- vertical movement (preserve goal column) -------------------------

    /// Move up one line, landing on the goal column clamped to the target line's length.
    /// No-op on the first line.
    pub fn move_up(&mut self, doc: &Document) {
        if self.line > 0 {
            self.line -= 1;
            self.column = self.goal_column.min(doc.line_len_chars(self.line));
        }
        self.clamp(doc);
    }

    /// Move down one line, landing on the goal column clamped to the target line's length.
    /// No-op on the last line.
    pub fn move_down(&mut self, doc: &Document) {
        if self.line < Self::last_line(doc) {
            self.line += 1;
            self.column = self.goal_column.min(doc.line_len_chars(self.line));
        }
        self.clamp(doc);
    }

    /// Move up by `page` lines (clamping at the top), preserving the goal column.
    pub fn move_page_up(&mut self, doc: &Document, page: usize) {
        self.line = self.line.saturating_sub(page);
        self.column = self.goal_column.min(doc.line_len_chars(self.line));
        self.clamp(doc);
    }

    /// Move down by `page` lines (clamping at the bottom), preserving the goal column.
    pub fn move_page_down(&mut self, doc: &Document, page: usize) {
        self.line = (self.line + page).min(Self::last_line(doc));
        self.column = self.goal_column.min(doc.line_len_chars(self.line));
        self.clamp(doc);
    }

    /// Jump the caret to the start of `line` (clamped into the buffer). Used by structure
    /// navigation — jumping to a heading. Resets the goal column.
    pub fn move_to_line(&mut self, doc: &Document, line: usize) {
        self.line = line;
        self.set_column(0);
        self.clamp(doc);
    }

    /// Place the caret at `(line, col)`, clamping the line into the buffer (same rule as
    /// [`View::move_to_line`]) and the column into the target line (same rule as
    /// [`View::move_end`]). Resets the goal column. Used to land on a search match and to
    /// restore the origin position when a search is cancelled.
    pub fn move_to(&mut self, doc: &Document, line: usize, col: usize) {
        self.move_to_line(doc, line);
        let max = doc.line_len_chars(self.line);
        self.set_column(col.min(max));
    }

    // ---- editing (mutate the document, then the cursor) -------------------

    /// Insert `ch` at the caret and step the caret past it. A `char` is one column
    /// regardless of how many UTF-8 bytes it takes.
    pub fn insert_char(&mut self, doc: &mut Document, ch: char) {
        let idx = self.cursor_char_idx(doc);
        let mut buf = [0u8; 4];
        doc.insert(idx, ch.encode_utf8(&mut buf));
        self.set_column(self.column + 1);
        self.clamp(doc);
    }

    /// Split the line at the caret, moving it to the start of the new line below.
    pub fn insert_newline(&mut self, doc: &mut Document) {
        let idx = self.cursor_char_idx(doc);
        doc.insert(idx, "\n");
        self.line += 1;
        self.set_column(0);
        self.clamp(doc);
    }

    /// Delete the character before the caret. At column 0 this removes the previous
    /// line's break, joining the current line onto it. No-op at the start of the buffer.
    pub fn backspace(&mut self, doc: &mut Document) {
        let idx = self.cursor_char_idx(doc);
        if idx == 0 {
            return;
        }
        if self.column > 0 {
            doc.remove(idx - 1..idx);
            self.set_column(self.column - 1);
        } else {
            // Column 0: the char just before the caret is the previous line's '\n'.
            // Removing it joins this line onto the previous one; the caret lands at the
            // seam, i.e. the previous line's old length.
            let prev = self.line - 1;
            let seam = doc.line_len_chars(prev);
            doc.remove(idx - 1..idx);
            self.line = prev;
            self.set_column(seam);
        }
        self.clamp(doc);
    }

    /// Delete the character at the caret. At a line's end this removes the line break,
    /// pulling the next line up. No-op at the end of the buffer.
    pub fn delete(&mut self, doc: &mut Document) {
        let idx = self.cursor_char_idx(doc);
        if idx >= doc.char_count() {
            return;
        }
        doc.remove(idx..idx + 1);
        self.clamp(doc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a document and drop a fresh view on it — the pair every test needs.
    fn fixture(text: &str) -> (Document, View) {
        (Document::from_text(text), View::new())
    }

    #[test]
    fn new_view_is_top_left() {
        let view = View::new();
        assert_eq!(view.cursor_line(), 0);
        assert_eq!(view.cursor_column(), 0);
    }

    // ---- horizontal -------------------------------------------------------

    #[test]
    fn right_advances_then_wraps_at_line_end() {
        let (doc, mut v) = fixture("ab\ncd\n");
        v.move_right(&doc); // (0,1)
        v.move_right(&doc); // (0,2) — end of "ab"
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 2));
        v.move_right(&doc); // wraps to start of next line
        assert_eq!((v.cursor_line(), v.cursor_column()), (1, 0));
    }

    #[test]
    fn right_at_end_of_buffer_is_a_noop() {
        let (doc, mut v) = fixture("ab"); // one line, no trailing newline
        v.move_end(&doc); // (0,2)
        v.move_right(&doc);
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 2));
    }

    #[test]
    fn left_wraps_to_previous_line_end_and_stops_at_origin() {
        let (doc, mut v) = fixture("ab\ncd\n");
        v.move_down(&doc); // (1,0)
        v.move_left(&doc); // wraps to end of "ab"
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 2));
        v.move_home();
        v.move_left(&doc); // already at (0,0)
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 0));
    }

    #[test]
    fn home_and_end() {
        let (doc, mut v) = fixture("hello\n");
        v.move_end(&doc);
        assert_eq!(v.cursor_column(), 5);
        v.move_home();
        assert_eq!(v.cursor_column(), 0);
    }

    // ---- vertical + goal column ------------------------------------------

    #[test]
    fn down_and_up_clamp_column_to_target_line() {
        let (doc, mut v) = fixture("longline\nx\nlongline\n");
        v.move_end(&doc); // (0,8)
        v.move_down(&doc); // line 1 "x" is length 1 → clamp to (1,1)
        assert_eq!((v.cursor_line(), v.cursor_column()), (1, 1));
    }

    #[test]
    fn goal_column_survives_a_short_line() {
        let (doc, mut v) = fixture("longline\nx\nlongline\n");
        v.move_end(&doc); // (0,8), goal 8
        v.move_down(&doc); // (1,1) but goal stays 8
        v.move_down(&doc); // (2, min(8,8)) = (2,8)
        assert_eq!((v.cursor_line(), v.cursor_column()), (2, 8));
    }

    #[test]
    fn a_horizontal_move_resets_the_goal_column() {
        let (doc, mut v) = fixture("longline\nx\nlongline\n");
        v.move_end(&doc); // (0,8), goal 8
        v.move_down(&doc); // (1,1), goal still 8
        v.move_left(&doc); // horizontal → goal reset to current column (0)
        v.move_down(&doc); // (2,0), not (2,8)
        assert_eq!((v.cursor_line(), v.cursor_column()), (2, 0));
    }

    #[test]
    fn up_on_first_line_and_down_on_last_line_are_noops() {
        let (doc, mut v) = fixture("a\nb\n");
        v.move_up(&doc);
        assert_eq!(v.cursor_line(), 0);
        // last resting line is the phantom trailing empty line (index 2)
        v.move_down(&doc); // (1,0)
        v.move_down(&doc); // (2,0)
        v.move_down(&doc); // stays
        assert_eq!(v.cursor_line(), 2);
    }

    #[test]
    fn caret_reaches_the_phantom_trailing_line_and_no_further() {
        let (doc, mut v) = fixture("a\nb\n"); // 3 lines: "a\n","b\n",""
        v.move_page_down(&doc, 100);
        assert_eq!((v.cursor_line(), v.cursor_column()), (2, 0));
    }

    #[test]
    fn paging_moves_by_page_and_clamps() {
        let (doc, mut v) = fixture("0\n1\n2\n3\n4\n5\n");
        v.move_page_down(&doc, 3); // (3,0)
        assert_eq!(v.cursor_line(), 3);
        v.move_page_up(&doc, 10); // clamp at top
        assert_eq!(v.cursor_line(), 0);
    }

    // ---- editing ----------------------------------------------------------

    #[test]
    fn insert_char_into_empty_buffer() {
        let (mut doc, mut v) = fixture("");
        v.insert_char(&mut doc, 'x');
        assert_eq!(doc.text(), "x");
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 1));
        assert!(doc.is_modified());
    }

    #[test]
    fn insert_char_mid_line_shifts_and_advances() {
        let (mut doc, mut v) = fixture("ac\n");
        v.move_right(&doc); // (0,1) between a and c
        v.insert_char(&mut doc, 'b');
        assert_eq!(doc.text(), "abc\n");
        assert_eq!(v.cursor_column(), 2);
    }

    #[test]
    fn insert_newline_splits_the_line() {
        let (mut doc, mut v) = fixture("abcd\n");
        v.move_right(&doc);
        v.move_right(&doc); // (0,2)
        v.insert_newline(&mut doc);
        assert_eq!(doc.text(), "ab\ncd\n");
        assert_eq!((v.cursor_line(), v.cursor_column()), (1, 0));
    }

    #[test]
    fn backspace_mid_line_removes_the_preceding_char() {
        let (mut doc, mut v) = fixture("abc\n");
        v.move_end(&doc); // (0,3)
        v.backspace(&mut doc);
        assert_eq!(doc.text(), "ab\n");
        assert_eq!(v.cursor_column(), 2);
    }

    #[test]
    fn backspace_at_column_zero_joins_the_previous_line() {
        let (mut doc, mut v) = fixture("ab\ncd\n");
        v.move_down(&doc); // (1,0)
        v.backspace(&mut doc);
        assert_eq!(doc.text(), "abcd\n");
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 2)); // at the seam
    }

    #[test]
    fn backspace_at_origin_is_a_noop() {
        let (mut doc, mut v) = fixture("ab\n");
        v.backspace(&mut doc);
        assert_eq!(doc.text(), "ab\n");
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 0));
    }

    #[test]
    fn delete_removes_char_at_cursor() {
        let (mut doc, mut v) = fixture("abc\n");
        v.move_right(&doc); // (0,1)
        v.delete(&mut doc); // removes 'b'
        assert_eq!(doc.text(), "ac\n");
        assert_eq!(v.cursor_column(), 1);
    }

    #[test]
    fn delete_at_line_end_joins_the_next_line() {
        let (mut doc, mut v) = fixture("ab\ncd\n");
        v.move_end(&doc); // (0,2), the char there is '\n'
        v.delete(&mut doc);
        assert_eq!(doc.text(), "abcd\n");
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 2));
    }

    #[test]
    fn delete_at_end_of_buffer_is_a_noop() {
        let (mut doc, mut v) = fixture("ab");
        v.move_end(&doc); // (0,2) == end of buffer
        v.delete(&mut doc);
        assert_eq!(doc.text(), "ab");
    }

    #[test]
    fn unicode_char_is_a_single_column() {
        let (mut doc, mut v) = fixture("");
        v.insert_char(&mut doc, 'é'); // two UTF-8 bytes, ONE char
        assert_eq!(v.cursor_column(), 1);
        assert_eq!(v.cursor_char_idx(&doc), 1);
        assert_eq!(doc.text(), "é");
    }

    #[test]
    fn move_to_line_jumps_to_line_start_and_clamps() {
        let (doc, mut v) = fixture("aa\nbb\ncc\n");
        v.move_to_line(&doc, 2);
        assert_eq!((v.cursor_line(), v.cursor_column()), (2, 0));
        v.move_to_line(&doc, 999); // clamps to the phantom trailing line
        assert_eq!(v.cursor_line(), 3);
    }

    #[test]
    fn move_to_places_the_cursor_at_line_and_column_clamped() {
        let (doc, mut v) = fixture("short\na longer line\n");
        v.move_to(&doc, 1, 9);
        assert_eq!((v.cursor_line(), v.cursor_column()), (1, 9));
        v.move_to(&doc, 0, 99); // clamps to the end of "short"
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 5));
        v.move_to(&doc, 99, 0); // clamps to the phantom trailing line
        assert_eq!((v.cursor_line(), v.cursor_column()), (2, 0));
    }

    #[test]
    fn move_to_clamps_the_goal_column_not_just_the_cursor() {
        let (doc, mut v) = fixture("short\na longer line\n");
        v.move_to(&doc, 0, 99); // clamps to (0,5), and the goal must clamp too
        v.move_down(&doc); // should land near column 5, not drift to line 1's full length (13)
        assert_eq!((v.cursor_line(), v.cursor_column()), (1, 5));
    }

    #[test]
    fn cursor_char_idx_matches_line_to_char_plus_column() {
        let (doc, mut v) = fixture("ab\ncd\n");
        v.move_down(&doc);
        v.move_right(&doc); // (1,1)
        assert_eq!(v.cursor_char_idx(&doc), doc.line_to_char(1) + 1);
        assert_eq!(v.cursor_char_idx(&doc), 4);
    }
}
