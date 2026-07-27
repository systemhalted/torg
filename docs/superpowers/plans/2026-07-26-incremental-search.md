# Incremental Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In-buffer incremental find per `docs/superpowers/specs/2026-07-26-incremental-search-design.md` — `Ctrl+F` prompt, live jump while typing, `Ctrl+F`/`Ctrl+R` step, smart case, wraparound, Esc-restore.

**Architecture:** A new UI-agnostic `crates/core/src/search.rs` (free functions, the `timestamp.rs` shape) does all matching per-line (queries cannot contain newlines, so no whole-document scan buffer is needed). The TUI adds `Action::Find`, a `Mode::Search` prompt built on the existing `prompt_event` machinery, and search-aware line rendering in `ui.rs`.

**Tech Stack:** Rust, ropey (via `Document`), ratatui/crossterm (TUI tier only).

**Conventions:** every commit message is plain imperative (repo style, no `feat:` prefixes, no attribution trailers). After each task: `cargo test --workspace -q` and `cargo clippy --workspace -- -D warnings` must be clean. Work on the `search` branch.

---

### Task 1: Core matching — `matches_in_line` with smart case

**Files:**
- Create: `crates/core/src/search.rs`
- Modify: `crates/core/src/lib.rs` (module + re-exports)

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/search.rs`:

```rust
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
```

- [ ] **Step 2: Register the module and run the tests to verify they fail**

In `crates/core/src/lib.rs` add `pub mod search;` after `pub mod document;` (keep the list alphabetical: document, search, structure, timestamp, view).

Run: `cargo test -p torg-core search -- --nocapture`
Expected: FAIL to compile — `matches_in_line` not found.

- [ ] **Step 3: Implement `matches_in_line`**

Add above the tests in `search.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p torg-core search`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/search.rs crates/core/src/lib.rs
git commit -m "Add smart-case per-line matching to a new core search module"
```

---

### Task 2: Core document search — `find` with direction and wraparound

**Files:**
- Modify: `crates/core/src/search.rs`
- Modify: `crates/core/src/lib.rs` (re-export)

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `search.rs`:

```rust
    fn doc(text: &str) -> Document {
        Document::from_text(text)
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p torg-core search`
Expected: FAIL to compile — `find` not found.

- [ ] **Step 3: Implement `find`**

Add to `search.rs` (above the tests):

```rust
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

    let hit = |line: usize, bound: Option<(usize, bool)>| -> Option<Match> {
        let cols = matches_in_line(&doc.line_text(line), query);
        let col = match bound {
            // (col bound, at_or_after): filter relative to `from` on the anchor line.
            Some((b, true)) => cols.into_iter().find(|&c| c >= b),
            Some((b, false)) => cols.into_iter().rev().find(|&c| c <= b),
            None if forward => cols.first().copied(),
            None => cols.last().copied(),
        }?;
        Some(Match { line, col })
    };

    if forward {
        if let Some(m) = hit(from_line, Some((from_col, true))) {
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
        if let Some(m) = hit(from_line, Some((from_col, false))) {
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
```

In `crates/core/src/lib.rs`, extend the flat re-exports (next to the `timestamp` ones) with:

```rust
pub use search::{find, matches_in_line, Match};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p torg-core search`
Expected: 11 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/search.rs crates/core/src/lib.rs
git commit -m "Add directional document search with single wraparound"
```

---

### Task 3: `View::move_to` — column-precise cursor placement

**Files:**
- Modify: `crates/core/src/view.rs`

- [ ] **Step 1: Write the failing test**

In the existing `tests` module of `crates/core/src/view.rs`, add (mirroring the neighboring `move_to_line` tests' style):

```rust
    #[test]
    fn move_to_places_the_cursor_at_line_and_column_clamped() {
        let mut doc = Document::from_text("short\na longer line\n");
        let mut v = View::new();
        v.move_to(&doc, 1, 9);
        assert_eq!((v.cursor_line(), v.cursor_column()), (1, 9));
        v.move_to(&doc, 0, 99); // col past end clamps to line length
        assert_eq!((v.cursor_line(), v.cursor_column()), (0, 5));
        v.move_to(&mut doc, 99, 0); // line past end clamps to last line
        assert_eq!(v.cursor_line(), 1);
    }
```

(If `move_to_line`'s tests build `doc` immutably, match that — drop the stray `&mut`.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p torg-core move_to_places`
Expected: FAIL to compile — no method `move_to`.

- [ ] **Step 3: Implement `move_to`**

Next to `move_to_line` in `view.rs`:

```rust
    /// Place the cursor at `(line, col)`, clamping the line into the document and the
    /// column into the line (same clamping rules as [`View::move_to_line`]).
    pub fn move_to(&mut self, doc: &Document, line: usize, col: usize) {
        self.move_to_line(doc, line);
        let max = doc.line_len_chars(self.cursor_line());
        // Reuse move_to_line's line clamp, then pin the column (and desired column).
        self.cursor_col = col.min(max);
        self.desired_col = self.cursor_col;
    }
```

Adjust the two field names to whatever `view.rs` actually calls them (`move_home`/`move_end` show the pattern — copy their assignments verbatim).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p torg-core view`
Expected: all view tests pass, including the new one.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/view.rs
git commit -m "Add View::move_to for column-precise cursor placement"
```

---

### Task 4: `Action::Find` mapped from Ctrl+F

**Files:**
- Modify: `crates/tui/src/action.rs`

- [ ] **Step 1: Write the failing test**

In `action.rs`'s tests, next to the other ctrl-chord tests:

```rust
    #[test]
    fn ctrl_f_opens_find() {
        assert_eq!(key_to_action(ctrl('f')), Some(Action::Find));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p torg-tui ctrl_f_opens_find`
Expected: FAIL to compile — no variant `Find`.

- [ ] **Step 3: Implement**

Add `Find,` to the `Action` enum (with doc comment `/// Open the incremental-search prompt (Ctrl+F).`) and `'f' => Some(Action::Find),` to the ctrl-chord match arm in `key_to_action` (keep the arm list in the existing order style).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p torg-tui action`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/tui/src/action.rs
git commit -m "Map Ctrl+F to a new Find action"
```

---

### Task 5: `Mode::Search` — open, incremental re-search, step, accept, cancel

**Files:**
- Modify: `crates/tui/src/app.rs`

- [ ] **Step 1: Write the failing tests**

In `app.rs`'s tests (use the existing `ctrl`/`press`/`type_str`-style helpers; check the helper block at ~line 884 and reuse what exists):

```rust
    fn app_with(text: &str) -> App {
        // Use the existing test constructor the other tests use (e.g. App::with_text or
        // the fixture helper); this pseudo-helper stands for that.
        App::with_text(text)
    }

    #[test]
    fn typing_in_the_find_prompt_jumps_to_the_nearest_match() {
        let mut app = app_with("alpha\nbeta\ngamma\n");
        ctrl(&mut app, 'f');
        type_str(&mut app, "gam");
        assert_eq!(app.cursor_line_for_test(), 2);
    }

    #[test]
    fn backspace_re_searches_from_the_origin() {
        let mut app = app_with("gab\ngamma\n");
        ctrl(&mut app, 'f');
        type_str(&mut app, "gam"); // jumps to line 1
        press_backspace(&mut app); // "ga" → first match from origin is line 0
        assert_eq!(app.cursor_line_for_test(), 0);
    }

    #[test]
    fn ctrl_f_steps_forward_and_ctrl_r_back() {
        let mut app = app_with("x\nfoo\nbar\nfoo\n");
        ctrl(&mut app, 'f');
        type_str(&mut app, "foo"); // line 1
        ctrl(&mut app, 'f'); // line 3
        assert_eq!(app.cursor_line_for_test(), 3);
        ctrl(&mut app, 'r'); // back to line 1
        assert_eq!(app.cursor_line_for_test(), 1);
    }

    #[test]
    fn esc_restores_cursor_and_enter_keeps_the_match_and_query() {
        let mut app = app_with("x\ny\nzz needle\n");
        ctrl(&mut app, 'f');
        type_str(&mut app, "needle");
        press_esc(&mut app);
        assert_eq!(app.cursor_line_for_test(), 0); // origin restored

        ctrl(&mut app, 'f');
        type_str(&mut app, "needle");
        press_enter(&mut app);
        assert_eq!(app.cursor_line_for_test(), 2); // match kept

        // Reopening pre-fills the last query and immediately re-finds.
        app.buf_view_move_top_for_test(); // move cursor back up (use existing motion keys)
        ctrl(&mut app, 'f');
        assert_eq!(app.cursor_line_for_test(), 2);
    }

    #[test]
    fn wraparound_and_not_found_set_statuses() {
        let mut app = app_with("needle\nx\n");
        ctrl(&mut app, 'f');
        type_str(&mut app, "needle");
        ctrl(&mut app, 'f'); // only match → wraps back to itself
        assert_eq!(app.status(), "Wrapped");

        let mut app = app_with("plain\n");
        ctrl(&mut app, 'f');
        type_str(&mut app, "zzz");
        assert_eq!(app.status(), "Not found");
    }
```

Replace the pseudo-helpers with whatever the existing test module already provides
(`app.buf().view.cursor_line()` is the real accessor used by neighboring tests; motion =
pressing arrow keys). Do not invent new public API for tests — the existing tests reach
state through `handle_key`, `status()`, and `buf()`-style internals.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p torg-tui search`
Expected: FAIL to compile — no `Mode::Search`, no `Find` arm.

- [ ] **Step 3: Implement**

In `app.rs`:

1. Import `torg_core::search` items alongside the existing `torg_core` imports:
   `find` as `search_find` (or path-qualify `torg_core::search::find`) and reuse the
   existing `Match` name carefully (import as `SearchMatch` if `Match` collides).

2. Extend `Mode`:

```rust
    /// The incremental-search prompt is open. `origin*` restore the view on Esc; query
    /// edits re-search from the origin, stepping searches from the cursor.
    Search { input: String, origin: (usize, usize), origin_top: usize, forward: bool },
```

3. Add a field to `App`: `last_query: String` (init `String::new()` in the constructor).

4. In `handle_key`'s mode match, route `Mode::Search { .. } => self.handle_search_key(key)`.

5. In the Edit-mode action dispatch, add `Action::Find => self.open_search(),`.

6. Implement (private methods on `App`):

```rust
    /// `Ctrl+F`: open the Find prompt, pre-filled with the last query.
    fn open_search(&mut self) {
        let b = self.buf();
        let origin = (b.view.cursor_line(), b.view.cursor_column());
        self.mode = Mode::Search {
            input: self.last_query.clone(),
            origin,
            origin_top: b.scroll_top,
            forward: true,
        };
        if !self.last_query.is_empty() {
            self.search_from(origin, true);
        }
    }

    /// Re-run the search and move the cursor; sets Wrapped / Not found statuses.
    fn search_from(&mut self, from: (usize, usize), forward: bool) {
        let query = match &self.mode {
            Mode::Search { input, .. } => input.clone(),
            _ => return,
        };
        if query.is_empty() {
            return;
        }
        match torg_core::search::find(&self.buf().doc, &query, from, forward) {
            Some((m, wrapped)) => {
                let b = self.buf_mut();
                b.view.move_to(&b.doc, m.line, m.col);
                self.status = if wrapped { "Wrapped".into() } else { String::new() };
            }
            None => self.status = "Not found".into(),
        }
    }

    /// One char past the cursor in the stepping direction, for Ctrl+F/Ctrl+R repeats.
    fn step_anchor(&self, forward: bool) -> (usize, usize) {
        let b = self.buf();
        let idx = b.view.cursor_char_idx(&b.doc);
        let idx = if forward { (idx + 1).min(b.doc.char_count()) } else { idx.saturating_sub(1) };
        let line = b.doc.char_to_line(idx);
        (line, idx - b.doc.line_to_char(line))
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('r')) {
            let fwd = matches!(key.code, KeyCode::Char('f'));
            if let Mode::Search { forward, .. } = &mut self.mode {
                *forward = fwd;
            }
            let from = self.step_anchor(fwd);
            self.search_from(from, fwd);
            return;
        }
        self.status.clear();
        let (event, origin, origin_top, forward) = match &mut self.mode {
            Mode::Search { input, origin, origin_top, forward } => {
                (prompt_event(input, key), *origin, *origin_top, *forward)
            }
            _ => return,
        };
        match event {
            PromptEvent::Pending => self.search_from(origin, forward),
            PromptEvent::Cancelled => {
                let b = self.buf_mut();
                b.view.move_to(&b.doc, origin.0, origin.1);
                b.scroll_top = origin_top;
                self.mode = Mode::Edit;
            }
            PromptEvent::Submitted(text) => {
                self.last_query = text;
                self.mode = Mode::Edit;
            }
        }
    }
```

Notes for the implementer:
- `Pending` fires for every printable char and Backspace — that is exactly the
  "re-search from origin" rule; no extra bookkeeping.
- Borrow care: `search_from` reads `self.mode` then mutates the buffer — clone the query
  first exactly as written; don't hold a `&self.mode` borrow across `buf_mut()`.
- The viewport follows automatically: the frame loop already calls `ensure_visible`
  (see `viewport_top` usage at `app.rs:353-357`); confirm and rely on it rather than
  scrolling manually. If prompts skip that path, call the same helper after moving.
- An empty query never moves the cursor and never sets a status (clearing input walks
  the cursor back only when the shorter query still matches somewhere — from-origin
  semantics handle this).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p torg-tui`
Expected: all pass, including the 5 new tests.

- [ ] **Step 5: Commit**

```bash
git add crates/tui/src/app.rs
git commit -m "Add the incremental-search mode: live jump, stepping, Esc-restore"
```

---

### Task 6: Rendering — `Find:` prompt line and match highlighting

**Files:**
- Modify: `crates/tui/src/ui.rs`
- Modify: `crates/tui/src/app.rs` (one read-only accessor)

- [ ] **Step 1: Write the failing tests**

In `ui.rs`'s tests, next to the existing `highlight_line` tests:

```rust
    #[test]
    fn search_line_styles_every_occurrence_and_the_current_one_distinctly() {
        let line = search_line("foo bar foo", "foo", Some(8), Style::default());
        // Spans: "foo"(match) " bar " "foo"(current) — 3 spans, matches styled.
        let spans: Vec<&str> = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(spans, vec!["foo", " bar ", "foo"]);
        assert_ne!(line.spans[0].style, line.spans[1].style);
        assert_ne!(line.spans[0].style, line.spans[2].style); // current ≠ other matches
    }

    #[test]
    fn search_line_without_matches_is_one_plain_span() {
        let line = search_line("nothing here", "zzz", None, Style::default());
        assert_eq!(line.spans.len(), 1);
    }
```

Also add a status-line test next to the existing prompt-format tests:

```rust
    #[test]
    fn search_mode_status_shows_the_find_prompt() {
        // Follow the pattern of the SaveAs/Tags status tests: build an App, enter the
        // mode via handle_key (ctrl 'f' + type), assert the returned status string.
        // Expected text: "Find: ne" after typing "ne".
    }
```

(Write it concretely against the neighboring tests' actual helper style — they already
construct an `App` and call the status/prompt formatter; mirror the `Tags:` test.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p torg-tui ui`
Expected: FAIL to compile — `search_line` not found; `Mode::Search` arm missing (non-exhaustive match errors in `ui.rs` may already appear when Task 5 landed — if `ui.rs` stopped compiling in Task 5, fold the minimal match arms into that task's step 3 to keep the tree green, and note it in that commit).

- [ ] **Step 3: Implement**

1. `app.rs` accessor (near `status()`/`mode()`):

```rust
    /// The active search highlight: query + the current match's (line, col), if searching.
    pub fn search_hl(&self) -> Option<(&str, (usize, usize))> {
        match &self.mode {
            Mode::Search { input, .. } if !input.is_empty() => {
                let b = self.buf();
                Some((input.as_str(), (b.view.cursor_line(), b.view.cursor_column())))
            }
            _ => None,
        }
    }
```

2. `ui.rs` — new function next to `highlight_line`:

```rust
/// Render one line during search: every `query` occurrence styled, the one starting at
/// `current` (char col) in the current-match style. Falls back to a single plain span.
fn search_line(text: &str, query: &str, current: Option<usize>, base: Style) -> Line<'static> {
    let cols = torg_core::search::matches_in_line(text, query);
    if cols.is_empty() {
        return Line::from(Span::styled(text.to_string(), base));
    }
    let match_style = base.add_modifier(Modifier::REVERSED);
    let current_style = base.add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let chars: Vec<char> = text.chars().collect();
    let qlen = query.chars().count();
    let mut spans = Vec::new();
    let mut at = 0usize;
    for col in cols {
        if col < at {
            continue; // overlapping match already covered
        }
        if col > at {
            spans.push(Span::styled(chars[at..col].iter().collect::<String>(), base));
        }
        let style = if current == Some(col) { current_style } else { match_style };
        spans.push(Span::styled(chars[col..col + qlen].iter().collect::<String>(), style));
        at = col + qlen;
    }
    if at < chars.len() {
        spans.push(Span::styled(chars[at..].iter().collect::<String>(), base));
    }
    Line::from(spans)
}
```

   Match the file's real imports/styles: reuse the exact `Style`/`Span`/`Line` types and
   the styling constants the file already defines (if there's a selection/DONE style
   palette, prefer those over raw `Modifier::REVERSED` — follow local convention).

3. In the body-render loop where `highlight_line(text, base)` is called for each visible
   line: if `app.search_hl()` is `Some((query, (cur_line, cur_col)))`, call
   `search_line(text, query, (line == cur_line).then_some(cur_col), base)` instead.

4. Status line (`ui.rs` ~line 221): add
   `Mode::Search { input, .. } => return format!("Find: {input}"),` beside the other
   prompt arms, and add `Mode::Search { .. }` wherever the other prompt modes are grouped
   (e.g. the cursor-placement match at the top of the file follows prompt text length —
   mirror the `Tags:` handling, prompt prefix is 6 chars: `Find: `).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p torg-tui`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/tui/src/ui.rs crates/tui/src/app.rs
git commit -m "Render the Find prompt and highlight search matches in the viewport"
```

---

### Task 7: Docs, end-to-end verification, PR

**Files:**
- Modify: `docs/usage.md` (key table + new "Search" section, and the Known limitations list if regex/replace absence belongs there)
- Modify: `docs/guide.md` (feature section)
- Modify: `README.md` (works-today bullet)
- Modify: `docs/superpowers/specs/2026-07-26-incremental-search-design.md` (Status → `approved design, implemented`)

- [ ] **Step 1: Write the docs**

`docs/usage.md`: add to the key table —

```
| `Ctrl+F` | Find (incremental); `Ctrl+F`/`Ctrl+R` next/previous while open |
```

and a section after the structure-editing one:

```markdown
## Search

`Ctrl+F` opens the *Find* prompt. Matching is incremental: the cursor jumps to the
nearest match as you type. While the prompt is open, `Ctrl+F` steps to the next match
and `Ctrl+R` to the previous one; the search wraps around the buffer (status: `Wrapped`).
`Enter` closes the prompt and keeps your place; `Esc` returns to where you started.
A query in all-lowercase matches case-insensitively; any capital letter makes it exact.
`Ctrl+F` remembers the last query — press it twice to repeat a search. Searches are
literal text (no regular expressions).
```

`docs/guide.md`: mirror the same content in the guide's voice (one short section, both
formats behave identically). `README.md`: extend the works-today feature list with
incremental search. Keep wording plain; no marketing.

- [ ] **Step 2: Full workspace check**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
Expected: clean. (If `cargo fmt --check` is not part of this repo's CI, skip it — check `.github/workflows/ci.yml` once and follow what CI enforces.)

- [ ] **Step 3: End-to-end tmux verification**

Use the project verify skill (`.claude/skills/verify/SKILL.md`). Scenario:

1. Fixture: a `.org` file whose first viewport-full has no `needle`, with `needle zzz`
   on a line ~100 lines down.
2. Launch torg on it, `Ctrl+F`, type `needle` — assert the pane shows the match line
   (view scrolled) with the match visibly highlighted and status `Find: needle`.
3. `Esc` — assert the pane shows the top of the file again.
4. `Ctrl+F` (pre-filled) — assert it jumps back; `Enter`; cursor stays.
5. Type `NEEDLE` into a fresh search — assert `Not found` (smart case).

- [ ] **Step 4: Commit and open the PR**

```bash
git add docs/usage.md docs/guide.md README.md docs/superpowers/specs/2026-07-26-incremental-search-design.md
git commit -m "Document incremental search"
git push -u origin search
gh pr create --title "Add in-buffer incremental search (Ctrl+F)" --body "..."
```

PR body: goal paragraph, key table excerpt, note that core gained a UI-agnostic
`search` module reusable by future frontends, and the spec/plan file links.
(Repo convention: no attribution trailers in commits or PR bodies.)

---

## Self-review notes

- Spec coverage: keys/semantics table → Tasks 4-5; smart case & wraparound → Tasks 1-2;
  Esc-restore incl. viewport → Task 5; highlighting → Task 6; last-query recall → Task 5;
  docs → Task 7. "Out" items appear in no task — good.
- The one deliberate deviation from mechanical TDD: Task 6 warns that `Mode::Search`
  may force `ui.rs` match arms during Task 5 (non-exhaustive enum) — the plan allows
  folding minimal arms forward to keep every commit green.
- Types consistent: `Match { line, col }`, `find(doc, query, from, forward) ->
  Option<(Match, bool)>`, `matches_in_line(text, query) -> Vec<usize>`, `View::move_to`
  used in Task 5 exactly as defined in Task 3.
