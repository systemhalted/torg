# Structural Editing Implementation Plan (M3 completion)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote/demote headings and subtrees, move subtrees, insert sibling headings, cycle `[#X]` priorities, and edit `:tags:` — in both Org and Markdown buffers, per the approved spec at `docs/superpowers/specs/2026-07-12-structural-editing-design.md`.

**Architecture:** All edit operations are default methods on the existing `StructureProvider` trait (`crates/core/src/structure.rs`), driven by two new required primitives: `marker() -> u8` and `max_level() -> Option<usize>`. `Heading` gains `priority`/`tags` fields parsed by a shared headline chain. The TUI adds `Action` variants on Alt/Alt+Shift/Shift chords plus a `Tags:` prompt mode reusing the existing prompt machinery.

**Tech Stack:** Rust workspace; TDD with inline `#[cfg(test)]` mods; `cargo test` + `cargo clippy --all-targets -- -D warnings` green after every task; tmux end-to-end verification per `.claude/skills/verify/SKILL.md`.

**Working conventions for every task:** RED first (write the test, run it, watch it fail for the right reason), then GREEN (minimal code), then run the full workspace suite, then commit. Commit messages: no Co-Authored-By/attribution trailers.

---

### Task 1: Headline metadata — `priority` + `tags` on `Heading`, shared parse chain

**Files:**
- Modify: `crates/core/src/structure.rs` (Heading struct, both parsers, new helpers, tests)

- [ ] **Step 1.1: Write the failing tests** (append inside `mod tests`)

```rust
// ---- headline metadata: priorities and tags -------------------------------

#[test]
fn parses_priority_cookie_after_the_keyword() {
    let h = &outline("* TODO [#A] write\n").headings[0];
    assert_eq!(h.todo, Some(TodoState::Todo));
    assert_eq!(h.priority, Some('A'));
    assert_eq!(h.title, "write");
    let h = &outline("* [#B] plain\n").headings[0]; // cookie without keyword
    assert_eq!(h.priority, Some('B'));
    assert_eq!(h.title, "plain");
}

#[test]
fn a_malformed_cookie_stays_in_the_title() {
    assert_eq!(outline("* [#D] x\n").headings[0].priority, None); // only A-C
    assert_eq!(outline("* [#D] x\n").headings[0].title, "[#D] x");
    assert_eq!(outline("* [#A]x\n").headings[0].priority, None); // no space after
}

#[test]
fn parses_trailing_tags_out_of_the_title() {
    let h = &outline("* TODO [#A] fix bug :work:urgent:\n").headings[0];
    assert_eq!(h.tags, vec!["work", "urgent"]);
    assert_eq!(h.title, "fix bug");
    let h = &outline("* plain title\n").headings[0];
    assert!(h.tags.is_empty());
}

#[test]
fn tag_run_must_be_whole_valid_and_trailing() {
    assert!(outline("* a :not a tag:\n").headings[0].tags.is_empty()); // space inside
    assert_eq!(outline("* a :b: c\n").headings[0].tags, Vec::<String>::new()); // not trailing
    assert_eq!(outline("* x :a_1:@b:#c:%d:\n").headings[0].tags, vec!["a_1", "@b", "#c", "%d"]);
}

#[test]
fn markdown_headlines_share_the_metadata_chain() {
    let h = &md_outline("## DONE [#C] ship :rel:\n").headings[0];
    assert_eq!(h.todo, Some(TodoState::Done));
    assert_eq!(h.priority, Some('C'));
    assert_eq!(h.title, "ship");
    assert_eq!(h.tags, vec!["rel"]);
}
```

- [ ] **Step 1.2: Run and verify RED**

Run: `cargo test -p torg-core 2>&1 | grep -E "^error|FAILED"`
Expected: compile error — no field `priority` on `Heading`.

- [ ] **Step 1.3: Implement.** Add fields to `Heading`:

```rust
    /// The `[#X]` priority cookie (A–C) after the keyword, if present.
    pub priority: Option<char>,
    /// The `:tag:` run at the end of the headline, colons stripped.
    pub tags: Vec<String>,
```

Add helpers next to `split_todo_keyword`:

```rust
/// Split keyword, priority cookie, title, and trailing tags off a headline's text
/// (everything after the markers and their space).
fn parse_headline_rest(rest: &str) -> (Option<TodoState>, Option<char>, String, Vec<String>) {
    let (todo, rest) = split_todo_keyword(rest);
    let (priority, rest) = split_priority(rest);
    let (title, tags) = split_tags(rest);
    (todo, priority, title, tags)
}

/// Split a leading `[#A]`/`[#B]`/`[#C]` cookie. It must be followed by a space or the
/// end of the line; anything else stays in the title.
fn split_priority(rest: &str) -> (Option<char>, &str) {
    let Some(tail) = rest.strip_prefix("[#") else {
        return (None, rest);
    };
    let bytes = tail.as_bytes();
    match (bytes.first(), bytes.get(1)) {
        (Some(p @ b'A'..=b'C'), Some(b']')) => {
            let after = &tail[2..];
            if after.is_empty() {
                (Some(*p as char), "")
            } else if let Some(t) = after.strip_prefix(' ') {
                (Some(*p as char), t.trim_start())
            } else {
                (None, rest)
            }
        }
        _ => (None, rest),
    }
}

/// A character allowed inside a tag: letters, digits, `_ @ # %` (the Org manual's rule).
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// Whether `s` is a complete tag run like `:a:b:`.
fn is_tag_run(s: &str) -> bool {
    s.len() >= 3
        && s.starts_with(':')
        && s.ends_with(':')
        && s[1..s.len() - 1]
            .split(':')
            .all(|t| !t.is_empty() && t.chars().all(is_tag_char))
}

/// Byte index where a trailing tag run starts in `text` (after trailing-whitespace trim),
/// or `None` if the line doesn't end in one.
fn tag_run_start(text: &str) -> Option<usize> {
    let trimmed = text.trim_end();
    if !trimmed.ends_with(':') {
        return None;
    }
    let start = trimmed
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    is_tag_run(&trimmed[start..]).then_some(start)
}

/// Split a trailing `:tag:` run off the title.
fn split_tags(rest: &str) -> (String, Vec<String>) {
    match tag_run_start(rest) {
        Some(start) => {
            let tags = rest.trim_end()[start..]
                .trim_matches(':')
                .split(':')
                .map(str::to_string)
                .collect();
            (rest[..start].trim_end().to_string(), tags)
        }
        None => (rest.trim_end().to_string(), Vec::new()),
    }
}
```

In **both** `parse_org_heading` and `parse_md_heading`, replace

```rust
    let (todo, title) = split_todo_keyword(rest);
    Some(Heading { level, line, title: title.to_string(), todo, last_line: line })
```

with

```rust
    let (todo, priority, title, tags) = parse_headline_rest(rest);
    Some(Heading { level, line, title, todo, priority, tags, last_line: line })
```

- [ ] **Step 1.4: Run and verify GREEN**

Run: `cargo test 2>&1 | grep -E "test result|FAILED"` and `cargo clippy --all-targets -- -D warnings`
Expected: all pass (existing titles unaffected — fixtures carry no cookies/tags), clippy clean.

- [ ] **Step 1.5: Commit** — `git add -A && git commit -m "Parse priority cookies and tags out of headlines"`

---

### Task 2: Trait primitives + `EditOutcome`

**Files:**
- Modify: `crates/core/src/structure.rs`

- [ ] **Step 2.1: Failing tests**

```rust
#[test]
fn providers_expose_marker_and_max_level() {
    assert_eq!(OrgProvider.marker(), b'*');
    assert_eq!(OrgProvider.max_level(), None);
    assert_eq!(MarkdownProvider.marker(), b'#');
    assert_eq!(MarkdownProvider.max_level(), Some(6));
    assert_eq!(Format::Markdown.marker(), b'#'); // enum delegates
}
```

- [ ] **Step 2.2: Verify RED** — `cargo test -p torg-core` → compile error, no method `marker`.

- [ ] **Step 2.3: Implement.** Extend the trait (above the navigation section):

```rust
/// What a structural edit did — either the document changed (and where the cursor should
/// land) or nothing happened (and why, for the status line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOutcome {
    Changed { cursor_line: usize },
    NoOp(&'static str),
}

const NOT_ON_HEADING: &str = "Not inside a heading's subtree";
```

Add to `trait StructureProvider`:

```rust
    /// The heading marker byte (`*` for Org, `#` for Markdown).
    fn marker(&self) -> u8;
    /// The deepest legal heading level, if the format has one.
    fn max_level(&self) -> Option<usize>;
```

Implementations: `OrgProvider` → `b'*'` / `None`; `MarkdownProvider` → `b'#'` / `Some(6)`; `Format` → match-delegate both (same shape as its `parse`).

Also add the shared lookup helper (free function):

```rust
/// The heading whose subtree contains `line`: the nearest heading at or above it.
fn enclosing(outline: &Outline, line: usize) -> Option<&Heading> {
    outline.headings.iter().rev().find(|h| h.line <= line)
}
```

- [ ] **Step 2.4: GREEN + clippy** (a temporary `#[allow(dead_code)]` on `enclosing`/`NOT_ON_HEADING` is acceptable until Task 3 uses them — remove it there).

- [ ] **Step 2.5: Commit** — `"Add marker/max_level primitives and EditOutcome to StructureProvider"`

---

### Task 3: Promote/demote heading (default methods)

**Files:** `crates/core/src/structure.rs`

- [ ] **Step 3.1: Failing tests**

```rust
// ---- structural edits: promote/demote --------------------------------------

#[test]
fn promote_and_demote_change_one_heading_level() {
    let mut doc = Document::from_text("** A\n*** child\n");
    assert_eq!(OrgProvider.promote_heading(&mut doc, 0), EditOutcome::Changed { cursor_line: 0 });
    assert_eq!(doc.text(), "* A\n*** child\n"); // child untouched
    assert_eq!(OrgProvider.demote_heading(&mut doc, 0), EditOutcome::Changed { cursor_line: 0 });
    assert_eq!(doc.text(), "** A\n*** child\n");
}

#[test]
fn edits_target_the_enclosing_heading_from_a_body_line() {
    let mut doc = Document::from_text("** A\nbody\n");
    OrgProvider.demote_heading(&mut doc, 1); // cursor on "body"
    assert_eq!(doc.text(), "*** A\nbody\n");
}

#[test]
fn promote_refuses_at_level_1_and_demote_at_markdown_max() {
    let mut doc = Document::from_text("* top\n");
    assert!(matches!(OrgProvider.promote_heading(&mut doc, 0), EditOutcome::NoOp(_)));
    let mut doc = Document::from_text("###### deep\n");
    assert!(matches!(MarkdownProvider.demote_heading(&mut doc, 0), EditOutcome::NoOp(_)));
    let mut doc = Document::from_text("plain\n");
    assert!(matches!(OrgProvider.promote_heading(&mut doc, 0), EditOutcome::NoOp(_)));
}
```

- [ ] **Step 3.2: RED** — no method `promote_heading`.

- [ ] **Step 3.3: Implement** as default methods on the trait:

```rust
    /// Promote the enclosing heading one level (children keep theirs).
    fn promote_heading(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if h.level == 1 {
            return EditOutcome::NoOp("Already at top level");
        }
        let start = doc.line_to_char(h.line);
        doc.remove(start..start + 1);
        EditOutcome::Changed { cursor_line: line }
    }

    /// Demote the enclosing heading one level (children keep theirs).
    fn demote_heading(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if Some(h.level) == self.max_level() {
            return EditOutcome::NoOp("Markdown headings stop at level 6");
        }
        let start = doc.line_to_char(h.line);
        doc.insert(start, &(self.marker() as char).to_string());
        EditOutcome::Changed { cursor_line: line }
    }
```

- [ ] **Step 3.4: GREEN + clippy.**  **Step 3.5: Commit** — `"Add promote/demote heading as trait default methods"`

---

### Task 4: Promote/demote subtree

**Files:** `crates/core/src/structure.rs`

- [ ] **Step 4.1: Failing tests**

```rust
#[test]
fn subtree_promote_and_demote_shift_every_member() {
    let mut doc = Document::from_text("** A\nbody\n*** B\n** next\n");
    OrgProvider.demote_subtree(&mut doc, 0);
    assert_eq!(doc.text(), "*** A\nbody\n**** B\n** next\n"); // sibling untouched
    OrgProvider.promote_subtree(&mut doc, 0);
    assert_eq!(doc.text(), "** A\nbody\n*** B\n** next\n");
}

#[test]
fn subtree_demote_refuses_if_any_member_would_pass_markdown_max() {
    let mut doc = Document::from_text("##### A\n###### deep child\n");
    assert!(matches!(MarkdownProvider.demote_subtree(&mut doc, 0), EditOutcome::NoOp(_)));
    assert_eq!(doc.text(), "##### A\n###### deep child\n"); // untouched
}
```

- [ ] **Step 4.2: RED.**  **Step 4.3: Implement:**

```rust
    /// Promote the enclosing heading and every heading in its subtree.
    fn promote_subtree(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if h.level == 1 {
            return EditOutcome::NoOp("Already at top level");
        }
        let members: Vec<usize> = subtree_member_lines(&outline, h);
        for l in members {
            let start = doc.line_to_char(l);
            doc.remove(start..start + 1);
        }
        EditOutcome::Changed { cursor_line: line }
    }

    /// Demote the enclosing heading and every heading in its subtree.
    fn demote_subtree(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if let Some(max) = self.max_level() {
            let too_deep = outline
                .headings
                .iter()
                .any(|m| m.line >= h.line && m.line <= h.last_line && m.level == max);
            if too_deep {
                return EditOutcome::NoOp("Markdown headings stop at level 6");
            }
        }
        let m = (self.marker() as char).to_string();
        let members: Vec<usize> = subtree_member_lines(&outline, h);
        for l in members {
            let start = doc.line_to_char(l);
            doc.insert(start, &m);
        }
        EditOutcome::Changed { cursor_line: line }
    }
```

Free helper next to `enclosing`:

```rust
/// The heading lines of `h` and every heading inside its subtree.
fn subtree_member_lines(outline: &Outline, h: &Heading) -> Vec<usize> {
    outline
        .headings
        .iter()
        .filter(|m| m.line >= h.line && m.line <= h.last_line)
        .map(|m| m.line)
        .collect()
}
```

- [ ] **Step 4.4: GREEN + clippy.**  **Step 4.5: Commit** — `"Add subtree promote/demote"`

---

### Task 5: Move subtree up/down

**Files:** `crates/core/src/structure.rs`

- [ ] **Step 5.1: Failing tests**

```rust
#[test]
fn move_subtree_swaps_with_the_adjacent_same_level_sibling() {
    let text = "* A\na body\n** A child\n* B\nb body\n";
    let mut doc = Document::from_text(text);
    // Cursor on "a body" (line 1): A's 3-line subtree swaps with B's 2-line one.
    assert_eq!(OrgProvider.move_subtree_down(&mut doc, 1), EditOutcome::Changed { cursor_line: 3 });
    assert_eq!(doc.text(), "* B\nb body\n* A\na body\n** A child\n");
    assert_eq!(OrgProvider.move_subtree_up(&mut doc, 3), EditOutcome::Changed { cursor_line: 1 });
    assert_eq!(doc.text(), text);
}

#[test]
fn move_refuses_at_the_edges_and_across_parents() {
    let mut doc = Document::from_text("* A\n* B\n");
    assert!(matches!(OrgProvider.move_subtree_up(&mut doc, 0), EditOutcome::NoOp(_)));
    assert!(matches!(OrgProvider.move_subtree_down(&mut doc, 1), EditOutcome::NoOp(_)));
    // ** child may not escape its parent in either direction
    let mut doc = Document::from_text("* P1\n** only child\n* P2\n");
    assert!(matches!(OrgProvider.move_subtree_up(&mut doc, 1), EditOutcome::NoOp(_)));
    assert!(matches!(OrgProvider.move_subtree_down(&mut doc, 1), EditOutcome::NoOp(_)));
}

#[test]
fn move_down_at_end_of_buffer_without_trailing_newline_keeps_text_intact() {
    let mut doc = Document::from_text("* A\n* B no newline");
    OrgProvider.move_subtree_down(&mut doc, 0);
    assert_eq!(doc.text(), "* B no newline\n* A");
}
```

- [ ] **Step 5.2: RED.**  **Step 5.3: Implement:**

```rust
    /// Swap the enclosing subtree with the previous same-level sibling subtree.
    fn move_subtree_up(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        // The previous sibling is the heading whose subtree ends right above ours.
        let Some(prev) = outline
            .headings
            .iter()
            .find(|p| p.level == h.level && p.last_line + 1 == h.line)
        else {
            return EditOutcome::NoOp("No previous sibling at this level");
        };
        let offset = line - h.line;
        let new_top = swap_blocks(doc, prev.line, h.line, h.last_line);
        EditOutcome::Changed { cursor_line: new_top + offset }
    }

    /// Swap the enclosing subtree with the next same-level sibling subtree.
    fn move_subtree_down(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let Some(next) = outline
            .headings
            .iter()
            .find(|n| n.line == h.last_line + 1 && n.level == h.level)
        else {
            return EditOutcome::NoOp("No next sibling at this level");
        };
        let next_len = next.last_line - next.line + 1;
        swap_blocks(doc, h.line, next.line, next.last_line);
        EditOutcome::Changed { cursor_line: line + next_len }
    }
```

Free helper:

```rust
/// Swap the block of lines `[first_top, second_top)` with `[second_top, second_last]`
/// (two adjacent line blocks). Returns the line where the second block now starts
/// (= `first_top`). Handles a missing trailing newline at end of buffer.
fn swap_blocks(doc: &mut Document, first_top: usize, second_top: usize, second_last: usize) -> usize {
    let start_a = doc.line_to_char(first_top);
    let start_b = doc.line_to_char(second_top);
    let end_b = if second_last + 1 < doc.line_count() {
        doc.line_to_char(second_last + 1)
    } else {
        doc.text().chars().count()
    };
    let text = doc.text();
    let mut a: String = text.chars().skip(start_a).take(start_b - start_a).collect();
    let mut b: String = text.chars().skip(start_b).take(end_b - start_b).collect();
    if !b.ends_with('\n') {
        b.push('\n');
        if a.ends_with('\n') {
            a.pop(); // keep the total newline count identical
        }
    }
    doc.remove(start_a..end_b);
    doc.insert(start_a, &(b + &a));
    first_top
}
```

- [ ] **Step 5.4: GREEN + clippy.**  **Step 5.5: Commit** — `"Add move subtree up/down"`

---

### Task 6: Insert sibling heading

**Files:** `crates/core/src/structure.rs`

- [ ] **Step 6.1: Failing tests**

```rust
#[test]
fn insert_sibling_lands_after_the_current_subtree_at_the_same_level() {
    let mut doc = Document::from_text("** A\nbody\n* next\n");
    // Cursor on "body": sibling of A (level 2) goes before "* next".
    assert_eq!(OrgProvider.insert_sibling(&mut doc, 1, false), EditOutcome::Changed { cursor_line: 2 });
    assert_eq!(doc.text(), "** A\nbody\n** \n* next\n");
}

#[test]
fn insert_todo_sibling_carries_the_keyword() {
    let mut doc = Document::from_text("# A\n");
    MarkdownProvider.insert_sibling(&mut doc, 0, true);
    assert_eq!(doc.text(), "# A\n# TODO \n");
}

#[test]
fn insert_at_end_of_buffer_without_trailing_newline() {
    let mut doc = Document::from_text("* A\nbody no newline");
    assert_eq!(OrgProvider.insert_sibling(&mut doc, 1, false), EditOutcome::Changed { cursor_line: 2 });
    assert_eq!(doc.text(), "* A\nbody no newline\n* \n");
}

#[test]
fn insert_with_no_enclosing_heading_appends_a_level_1_heading() {
    let mut doc = Document::from_text("just prose\n");
    assert_eq!(OrgProvider.insert_sibling(&mut doc, 0, false), EditOutcome::Changed { cursor_line: 1 });
    assert_eq!(doc.text(), "just prose\n* \n");
}
```

- [ ] **Step 6.2: RED.**  **Step 6.3: Implement:**

```rust
    /// Insert a new (possibly `TODO`) sibling heading after the enclosing subtree; with no
    /// enclosing heading, append a level-1 heading at the end of the document.
    fn insert_sibling(&self, doc: &mut Document, line: usize, todo: bool) -> EditOutcome {
        let outline = self.parse(doc);
        let (level, target_line) = match enclosing(&outline, line) {
            Some(h) => (h.level, h.last_line + 1),
            None => (1, doc.line_count()),
        };
        let markers = (self.marker() as char).to_string().repeat(level);
        let keyword = if todo { "TODO " } else { "" };
        let (at, prefix) = if target_line < doc.line_count() {
            (doc.line_to_char(target_line), "")
        } else {
            let end = doc.text().chars().count();
            let needs_nl = end > 0 && !doc.text().ends_with('\n');
            (end, if needs_nl { "\n" } else { "" })
        };
        doc.insert(at, &format!("{prefix}{markers} {keyword}\n"));
        let cursor_line = doc.char_to_line(at + prefix.chars().count());
        EditOutcome::Changed { cursor_line }
    }
```

- [ ] **Step 6.4: GREEN + clippy.**  **Step 6.5: Commit** — `"Add insert-sibling-heading"`

---

### Task 7: Priority cycling

**Files:** `crates/core/src/structure.rs`

- [ ] **Step 7.1: Failing tests**

```rust
#[test]
fn priority_up_walks_none_c_b_a_and_stops() {
    let mut doc = Document::from_text("* TODO task\n");
    OrgProvider.cycle_priority(&mut doc, 0, true);
    assert_eq!(doc.text(), "* TODO [#C] task\n"); // cookie after the keyword
    OrgProvider.cycle_priority(&mut doc, 0, true);
    OrgProvider.cycle_priority(&mut doc, 0, true);
    assert_eq!(doc.text(), "* TODO [#A] task\n");
    assert!(matches!(OrgProvider.cycle_priority(&mut doc, 0, true), EditOutcome::NoOp(_)));
}

#[test]
fn priority_down_walks_a_b_c_none_and_stops() {
    let mut doc = Document::from_text("## [#A] task\n"); // Markdown, no keyword
    MarkdownProvider.cycle_priority(&mut doc, 0, false);
    assert_eq!(doc.text(), "## [#B] task\n");
    MarkdownProvider.cycle_priority(&mut doc, 0, false);
    MarkdownProvider.cycle_priority(&mut doc, 0, false);
    assert_eq!(doc.text(), "## task\n"); // cookie and its space removed
    assert!(matches!(MarkdownProvider.cycle_priority(&mut doc, 0, false), EditOutcome::NoOp(_)));
}

#[test]
fn priority_on_a_bare_cookie_headline_removes_cleanly() {
    let mut doc = Document::from_text("* DONE [#C]\n"); // keyword, cookie, no title
    OrgProvider.cycle_priority(&mut doc, 0, false);
    assert_eq!(doc.text(), "* DONE \n");
}
```

- [ ] **Step 7.2: RED.**  **Step 7.3: Implement:**

```rust
    /// Cycle the `[#X]` cookie on the enclosing heading: up = none→C→B→A, down = A→B→C→none.
    fn cycle_priority(&self, doc: &mut Document, line: usize, up: bool) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let new = match (h.priority, up) {
            (None, true) => Some('C'),
            (Some('C'), true) => Some('B'),
            (Some('B'), true) => Some('A'),
            (Some(_), true) => return EditOutcome::NoOp("Already at highest priority"),
            (None, false) => return EditOutcome::NoOp("No priority to lower"),
            (Some('A'), false) => Some('B'),
            (Some('B'), false) => Some('C'),
            (Some(_), false) => None,
        };
        // Everything before the cookie is ASCII (markers, spaces, keyword), so byte
        // offsets equal char offsets.
        let raw = doc.line_text(h.line);
        let text = raw.strip_suffix('\n').unwrap_or(&raw);
        let after_markers = &text[h.level..];
        let mut pos = h.level + (after_markers.len() - after_markers.trim_start().len());
        let rest = after_markers.trim_start();
        if split_todo_keyword(rest).0.is_some() {
            let after_kw = &rest[4..]; // TODO and DONE are both 4 ASCII bytes
            pos += 4 + (after_kw.len() - after_kw.trim_start().len());
        }
        let start = doc.line_to_char(h.line) + pos;
        match (h.priority, new) {
            (None, Some(p)) => doc.insert(start, &format!("[#{p}] ")),
            (Some(_), Some(p)) => {
                doc.remove(start..start + 5);
                doc.insert(start, &format!("[#{p}]"));
            }
            (Some(_), None) => {
                let followed_by_space = text[pos + 5..].starts_with(' ');
                doc.remove(start..start + if followed_by_space { 6 } else { 5 });
            }
            (None, None) => unreachable!(),
        }
        EditOutcome::Changed { cursor_line: line }
    }
```

- [ ] **Step 7.4: GREEN + clippy.**  **Step 7.5: Commit** — `"Add priority cycling"`

---

### Task 8: Set tags

**Files:** `crates/core/src/structure.rs`

- [ ] **Step 8.1: Failing tests**

```rust
#[test]
fn set_tags_appends_replaces_and_removes() {
    let mut doc = Document::from_text("* TODO task\n");
    OrgProvider.set_tags(&mut doc, 0, &["work".into(), "urgent".into()]);
    assert_eq!(doc.text(), "* TODO task :work:urgent:\n");
    OrgProvider.set_tags(&mut doc, 0, &["home".into()]); // replaces, not appends
    assert_eq!(doc.text(), "* TODO task :home:\n");
    OrgProvider.set_tags(&mut doc, 0, &[]); // removes
    assert_eq!(doc.text(), "* TODO task\n");
}

#[test]
fn is_valid_tag_enforces_the_character_set() {
    assert!(is_valid_tag("a_1@b"));
    assert!(!is_valid_tag("has space"));
    assert!(!is_valid_tag(""));
}
```

- [ ] **Step 8.2: RED.**  **Step 8.3: Implement.** Public validator (near `is_tag_char`):

```rust
/// Whether `tag` is a legal tag name (non-empty, only letters/digits/`_@#%`).
pub fn is_valid_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.chars().all(is_tag_char)
}
```

Default method:

```rust
    /// Replace the enclosing heading's trailing tag run with `tags` (empty = remove).
    /// Tags must already satisfy [`is_valid_tag`].
    fn set_tags(&self, doc: &mut Document, line: usize, tags: &[String]) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let raw = doc.line_text(h.line);
        let text = raw.strip_suffix('\n').unwrap_or(&raw).to_string();
        let prefix = match tag_run_start(&text) {
            Some(i) => text[..i].trim_end(),
            None => text.trim_end(),
        };
        let mut new_line = prefix.to_string();
        if !tags.is_empty() {
            new_line.push_str(&format!(" :{}:", tags.join(":")));
        }
        let start = doc.line_to_char(h.line);
        doc.remove(start..start + text.chars().count());
        doc.insert(start, &new_line);
        EditOutcome::Changed { cursor_line: line }
    }
```

- [ ] **Step 8.4: GREEN + clippy.**  **Step 8.5: Commit** — `"Add set_tags and the public tag validator"`

---

### Task 9: TUI actions and keymap

**Files:** `crates/tui/src/action.rs`

- [ ] **Step 9.1: Failing tests** (extend `mod tests`; add an `alt_shift`/`shift` helper)

```rust
fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}
fn alt_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}
fn alt_shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT)
}

#[test]
fn structural_editing_chords_map() {
    assert_eq!(key_to_action(alt_key(KeyCode::Left)), Some(Action::PromoteHeading));
    assert_eq!(key_to_action(alt_key(KeyCode::Right)), Some(Action::DemoteHeading));
    assert_eq!(key_to_action(alt_shift(KeyCode::Left)), Some(Action::PromoteSubtree));
    assert_eq!(key_to_action(alt_shift(KeyCode::Right)), Some(Action::DemoteSubtree));
    assert_eq!(key_to_action(alt_key(KeyCode::Up)), Some(Action::MoveSubtreeUp));
    assert_eq!(key_to_action(alt_key(KeyCode::Down)), Some(Action::MoveSubtreeDown));
    assert_eq!(key_to_action(alt_key(KeyCode::Enter)), Some(Action::InsertSibling));
    assert_eq!(key_to_action(alt_shift(KeyCode::Enter)), Some(Action::InsertTodoSibling));
    assert_eq!(key_to_action(shift(KeyCode::Up)), Some(Action::PriorityUp));
    assert_eq!(key_to_action(shift(KeyCode::Down)), Some(Action::PriorityDown));
    assert_eq!(key_to_action(ctrl('g')), Some(Action::EditTags));
}

#[test]
fn plain_arrows_and_enter_still_map_to_movement_and_newline() {
    assert_eq!(key_to_action(press(KeyCode::Left)), Some(Action::MoveLeft)); // regression
    assert_eq!(key_to_action(press(KeyCode::Enter)), Some(Action::Newline));
}
```

- [ ] **Step 9.2: RED.**  **Step 9.3: Implement.** Add variants to `Action`:

```rust
    // structural editing
    PromoteHeading,
    DemoteHeading,
    PromoteSubtree,
    DemoteSubtree,
    MoveSubtreeUp,
    MoveSubtreeDown,
    InsertSibling,
    InsertTodoSibling,
    PriorityUp,
    PriorityDown,
    EditTags,
```

In `key_to_action`, add `let shift = key.modifiers.contains(KeyModifiers::SHIFT);` and put the modified arms **before** the plain ones:

```rust
        KeyCode::Left if alt && shift => Some(Action::PromoteSubtree),
        KeyCode::Right if alt && shift => Some(Action::DemoteSubtree),
        KeyCode::Left if alt => Some(Action::PromoteHeading),
        KeyCode::Right if alt => Some(Action::DemoteHeading),
        KeyCode::Up if alt => Some(Action::MoveSubtreeUp),
        KeyCode::Down if alt => Some(Action::MoveSubtreeDown),
        KeyCode::Up if shift => Some(Action::PriorityUp),
        KeyCode::Down if shift => Some(Action::PriorityDown),
        KeyCode::Enter if alt && shift => Some(Action::InsertTodoSibling),
        KeyCode::Enter if alt => Some(Action::InsertSibling),
```

and `'g' => Some(Action::EditTags),` in the ctrl-char match.

- [ ] **Step 9.4: GREEN on action tests; app.rs now fails to compile (non-exhaustive `apply`) — add a temporary catch-all `_ => {}` arm at the end of `apply`'s match, removed in Task 10.**

- [ ] **Step 9.5: Commit** — `"Map structural-editing chords to actions"`

---

### Task 10: App wiring for tree ops and priorities

**Files:** `crates/tui/src/app.rs`

- [ ] **Step 10.1: Failing tests**

```rust
// ---- structural editing ---------------------------------------------------

fn alt_key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::ALT));
}
fn alt_shift(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT));
}
fn shift_key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::SHIFT));
}

#[test]
fn alt_arrows_promote_demote_and_move() {
    let mut app = single(Document::from_text("** A\nbody\n** B\n"), None);
    alt_key(&mut app, KeyCode::Left);
    assert_eq!(app.document().text(), "* A\nbody\n** B\n");
    alt_key(&mut app, KeyCode::Right);
    alt_key(&mut app, KeyCode::Down); // A's subtree past B's
    assert_eq!(app.document().text(), "** B\n** A\nbody\n");
    assert_eq!(app.view().cursor_line(), 1); // cursor followed A
}

#[test]
fn refused_edits_show_the_reason_on_the_status_line() {
    let mut app = single(Document::from_text("* top\n"), None);
    alt_key(&mut app, KeyCode::Left);
    assert_eq!(app.document().text(), "* top\n");
    assert!(!app.status().is_empty());
}

#[test]
fn alt_enter_inserts_a_sibling_and_puts_the_cursor_on_it() {
    let mut app = single(Document::from_text("* A\nbody\n"), None);
    alt_key(&mut app, KeyCode::Enter);
    assert_eq!(app.document().text(), "* A\nbody\n* \n");
    assert_eq!(app.view().cursor_line(), 2);
    typ(&mut app, "typed title");
    assert_eq!(app.document().text(), "* A\nbody\n* typed title\n");
}

#[test]
fn shift_arrows_cycle_priority() {
    let mut app = single(Document::from_text("* TODO t\n"), None);
    shift_key(&mut app, KeyCode::Up);
    assert_eq!(app.document().text(), "* TODO [#C] t\n");
    shift_key(&mut app, KeyCode::Down);
    assert_eq!(app.document().text(), "* TODO t\n");
}

#[test]
fn structural_edits_respect_the_buffer_format() {
    let mut app = single(Document::from_text("# A\n"), Some(PathBuf::from("x.md")));
    alt_key(&mut app, KeyCode::Right);
    assert_eq!(app.document().text(), "## A\n");
}
```

- [ ] **Step 10.2: RED** (catch-all arm swallows the actions; tests fail on unchanged text).

- [ ] **Step 10.3: Implement.** Remove the temporary `_ => {}`. Add arms:

```rust
            Action::PromoteHeading => self.structure_edit(|f, d, l| f.promote_heading(d, l)),
            Action::DemoteHeading => self.structure_edit(|f, d, l| f.demote_heading(d, l)),
            Action::PromoteSubtree => self.structure_edit(|f, d, l| f.promote_subtree(d, l)),
            Action::DemoteSubtree => self.structure_edit(|f, d, l| f.demote_subtree(d, l)),
            Action::MoveSubtreeUp => self.structure_edit(|f, d, l| f.move_subtree_up(d, l)),
            Action::MoveSubtreeDown => self.structure_edit(|f, d, l| f.move_subtree_down(d, l)),
            Action::InsertSibling => self.insert_sibling(false),
            Action::InsertTodoSibling => self.insert_sibling(true),
            Action::PriorityUp => self.structure_edit(|f, d, l| f.cycle_priority(d, l, true)),
            Action::PriorityDown => self.structure_edit(|f, d, l| f.cycle_priority(d, l, false)),
            Action::EditTags => self.open_tags_prompt(), // Task 11; stub as `{}` until then
```

Helpers (below `save`):

```rust
    /// Run a structural edit on the active buffer, then sync cursor, status, and outline.
    fn structure_edit(&mut self, op: impl FnOnce(Format, &mut Document, usize) -> EditOutcome) {
        let b = self.buf_mut();
        match op(b.format, &mut b.doc, b.view.cursor_line()) {
            EditOutcome::Changed { cursor_line } => {
                let b = self.buf_mut();
                b.view.move_to_line(&b.doc, cursor_line);
                self.reparse();
            }
            EditOutcome::NoOp(msg) => self.status = msg.into(),
        }
    }

    /// Insert a sibling heading and leave the cursor at the end of its (empty) title.
    fn insert_sibling(&mut self, todo: bool) {
        let b = self.buf_mut();
        match b.format.insert_sibling(&mut b.doc, b.view.cursor_line(), todo) {
            EditOutcome::Changed { cursor_line } => {
                let b = self.buf_mut();
                b.view.move_to_line(&b.doc, cursor_line);
                b.view.move_end(&b.doc);
                self.reparse();
            }
            EditOutcome::NoOp(msg) => self.status = msg.into(),
        }
    }
```

Imports: add `EditOutcome, Format` to the `textr_org_core::structure` import list, and `use textr_org_core::document::Document;` is already present.

- [ ] **Step 10.4: GREEN + clippy.**  **Step 10.5: Commit** — `"Wire structural editing into the app"`

---

### Task 11: Tags prompt (`Ctrl+G`, `Mode::EditTags`)

**Files:** `crates/tui/src/app.rs`, `crates/tui/src/ui.rs`

- [ ] **Step 11.1: Failing tests** (app.rs)

```rust
#[test]
fn ctrl_g_prefills_edits_and_writes_tags() {
    let mut app = single(Document::from_text("* task :old:\n"), None);
    ctrl(&mut app, 'g');
    assert_eq!(app.mode(), &Mode::EditTags { input: "old".into() });
    press(&mut app, KeyCode::Backspace); // "ol"
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace); // ""
    typ(&mut app, "work urgent");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode(), &Mode::Edit);
    assert_eq!(app.document().text(), "* task :work:urgent:\n");
}

#[test]
fn empty_tags_input_removes_the_tag_run() {
    let mut app = single(Document::from_text("* task :old:\n"), None);
    ctrl(&mut app, 'g');
    for _ in 0..3 {
        press(&mut app, KeyCode::Backspace);
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.document().text(), "* task\n");
}

#[test]
fn invalid_tag_characters_keep_the_prompt_open_with_a_message() {
    let mut app = single(Document::from_text("* task\n"), None);
    ctrl(&mut app, 'g');
    typ(&mut app, "bad!tag");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::EditTags { .. })); // still prompting
    assert!(!app.status().is_empty());
    assert_eq!(app.document().text(), "* task\n");
}

#[test]
fn ctrl_g_off_any_heading_is_a_status_noop() {
    let mut app = single(Document::from_text("prose only\n"), None);
    ctrl(&mut app, 'g');
    assert_eq!(app.mode(), &Mode::Edit);
    assert!(!app.status().is_empty());
}
```

- [ ] **Step 11.2: RED** (no `Mode::EditTags`).

- [ ] **Step 11.3: Implement.**
Add `EditTags { input: String }` to `Mode` (doc comment: the `Tags:` prompt). Route it in `handle_key` to `handle_prompt_key`. Rework the prompt handler's kind detection (it currently uses `is_save: bool`):

```rust
    fn handle_prompt_key(&mut self, key: KeyEvent) {
        enum Kind {
            SaveAs,
            Open,
            Tags,
        }
        let kind = match &self.mode {
            Mode::SaveAs { .. } => Kind::SaveAs,
            Mode::OpenFile { .. } => Kind::Open,
            Mode::EditTags { .. } => Kind::Tags,
            _ => return,
        };
        let event = match &mut self.mode {
            Mode::SaveAs { input } | Mode::OpenFile { input } | Mode::EditTags { input } => {
                prompt_event(input, key)
            }
            _ => return,
        };
        match event {
            PromptEvent::Pending => {}
            PromptEvent::Cancelled => self.mode = Mode::Edit,
            PromptEvent::Submitted(text) => {
                self.mode = Mode::Edit;
                match kind {
                    Kind::Tags => self.apply_tags(&text),
                    Kind::SaveAs | Kind::Open => {
                        let path = PathBuf::from(text);
                        if path.as_os_str().is_empty() {
                            return;
                        }
                        if matches!(kind, Kind::SaveAs) {
                            self.save_as(&path);
                        } else {
                            self.open_path(path);
                        }
                    }
                }
            }
        }
    }
```

`open_tags_prompt` (replacing the Task 10 stub) and `apply_tags`:

```rust
    /// `Ctrl+G`: open the tags prompt pre-filled with the enclosing heading's tags.
    fn open_tags_prompt(&mut self) {
        let b = self.buf();
        let line = b.view.cursor_line();
        match b.outline.headings.iter().rev().find(|h| h.line <= line) {
            Some(h) => {
                self.mode = Mode::EditTags {
                    input: h.tags.join(" "),
                };
            }
            None => self.status = "Not inside a heading's subtree".into(),
        }
    }

    /// Validate and write the space-separated tags typed into the prompt.
    fn apply_tags(&mut self, text: &str) {
        let tags: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        if let Some(bad) = tags.iter().find(|t| !is_valid_tag(t)) {
            self.status = format!("Invalid tag {bad:?} — use letters, digits, _ @ # %");
            self.mode = Mode::EditTags { input: text.to_string() };
            return;
        }
        let b = self.buf_mut();
        match b.format.set_tags(&mut b.doc, b.view.cursor_line(), &tags) {
            EditOutcome::Changed { .. } => self.reparse(),
            EditOutcome::NoOp(msg) => self.status = msg.into(),
        }
    }
```

Import `is_valid_tag` from `textr_org_core::structure`.

ui.rs: `status_text` gains `Mode::EditTags { input } => return format!("Tags: {input}"),` and `place_cursor` gains an `EditTags` arm identical in shape to `OpenFile` with the `"Tags: "` prefix width.

- [ ] **Step 11.4: GREEN + clippy.**  **Step 11.5: Commit** — `"Add the Ctrl+G tags prompt"`

---

### Task 12: Docs, verification, ship

**Files:** `docs/usage.md`, `README.md`, `docs/roadmap.md`, `docs/architecture.md`

- [ ] **Step 12.1: usage.md** — key-table rows for the nine chords (matching the spec's table) and a new "Structure editing" section after "Org structure" describing promote/demote heading-vs-subtree, moves, insertion, priorities (`[#A]` after the keyword, `Shift+↑/↓` stops at the ends), tags (`Ctrl+G`, space-separated, allowed characters, empty removes). Mention no-op status messages, and note operations apply to the heading whose subtree contains the cursor.

- [ ] **Step 12.2: README** — extend the structure bullet in "What works today" with "promote/demote, subtree moves, sibling insertion, `[#A]` priorities, `:tags:`"; flip roadmap list item 3 to *done*.

- [ ] **Step 12.3: roadmap.md** — M3 status paragraph → shipped (both halves); coverage-map rows "Structure editing…" and "Priorities…" get ✅; mermaid M3 line marked ✅.

- [ ] **Step 12.4: architecture.md** — trait sketch in the structure-layer section gains `marker`/`max_level` + a line noting the edit operations are trait default methods written once.

- [ ] **Step 12.5: Full gate** — `cargo test` (expect ~150+, all green), `cargo clippy --all-targets -- -D warnings`, `cargo build`.

- [ ] **Step 12.6: tmux end-to-end** per `.claude/skills/verify/SKILL.md` (private socket, `send-keys -l` for text): open an .org file; `Alt+→`/`Alt+←` on a heading with a child (child unmoved); `Alt+Shift+→` (child moves); `Alt+↓`/`Alt+↑` round-trip a subtree; `Alt+Enter`, type a title; `Shift+↑` twice → `[#B]`; `Ctrl+G`, type `work home`, Enter → `:work:home:`; repeat promote/demote + insert in a .md buffer (`# ` markers, level-6 refusal on `######`); confirm no-op statuses at edges. Capture panes as evidence.

- [ ] **Step 12.7: Commit docs** — `"Document structural editing; mark M3 complete"`. Report with verdict + evidence; offer push.

---

## Self-review notes

- Spec coverage: all nine operations (T3–T8, T10–T11), metadata parsing (T1), primitives (T2), keys (T9), docs+e2e (T12). Edge cases from the spec each have a named test: level-1/level-6 refusals (T3/T4), parent boundary + EOF newline (T5), no-enclosing-heading insert (T6), cookie-after-keyword + bare-cookie removal (T7), tag replace/remove + validation (T8/T11).
- Type consistency: `EditOutcome::Changed { cursor_line }` / `NoOp(&'static str)` used identically in core (T2–T8) and TUI (T10–T11); `structure_edit` closure signature `(Format, &mut Document, usize) -> EditOutcome` matches all six uses.
- Known simplifications (per spec): fold state doesn't travel with moved subtrees; tags are normalized to single-space separation before the run; no tag right-alignment.
