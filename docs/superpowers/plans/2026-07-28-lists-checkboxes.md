# Plain Lists, Checkboxes, and Statistics Cookies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `docs/superpowers/specs/2026-07-26-lists-checkboxes-design.md`: parse plain lists in Org and Markdown, toggle checkboxes (`Ctrl+Space`/`Alt+X`), insert and re-indent items context-sensitively (`Alt+Enter`, `Alt+←/→` on item lines), and auto-recompute `[n/m]`/`[p%]` statistics cookies on parent items and headings.

**Architecture:** New core module `crates/core/src/list.rs` (free functions over `Document` + `Format`, the `timestamp.rs`/`search.rs` shape — no trait changes; `Format` itself implements `StructureProvider`, so `format.parse(&doc)` gives the outline where needed). TUI adds one `Action::ToggleCheckbox` (which trips the commands-table drift guard by design — the same commit adds its `COMMANDS` row) and routes the three existing structural actions through list-aware context checks in `app.rs`, following the `shift_date_or_priority` precedent.

**Tech Stack:** Rust, ropey via `Document`; ratatui/crossterm in the TUI tier only.

**Conventions:** plain imperative commits, no prefixes, no attribution trailers. Gates after every task: `cargo test --workspace -q` and `cargo clippy --workspace -- -D warnings`. Branch: `m4-lists`.

**Authoritative semantics** (from the spec — reread it before each task):
- Bullets: `-`, `+` (both formats); `*` any-indent in Markdown but only indent ≥ 1 in Org (col-0 `*` is a heading); ordered `1.` / `1)`. Bullet then one space.
- Checkbox: `[ ]`, `[X]`, `[-]` immediately after bullet+space, then a space (`- [ ] task`); parse `[x]` too; write back `[X]` in Org, `[x]` in Markdown. `-[ ]` or a bracket later in the line is content.
- Nesting by indentation (children strictly deeper, up to the next line at ≤ indent); non-item lines end the region; indent unit 2 spaces.
- Cookies `[n/m]` / `[p%]` (and unfilled `[/]` / `[%]`) anywhere on a parent-item line or headline; recomputed (never inserted) after toggle/insert; parent-item cookies count DIRECT child checkbox items; heading cookies count TOP-LEVEL checkbox items in the heading's own section; `p` = integer truncation of `100*n/m`; `[0/0]` written literally.
- Fenced code lines in Markdown are never items (reuse the provider's existing fence guard — find how MarkdownProvider skips fenced `#` lines and apply the same mechanism).

---

### Task 1: Core parsing — `list.rs` with `item_at`

**Files:**
- Create: `crates/core/src/list.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod list;` alphabetical; re-export `pub use list::{item_at, toggle_checkbox, insert_item, indent_item, ListItem, Bullet, CheckState};` — add names as they appear per task)

- [ ] **Step 1: Failing tests.** Model + parser tests (module shape mirrors `search.rs`):

```rust
//! Plain lists: bullets, checkboxes, statistics cookies, and their edits.
//! Free functions over `Document` + `Format`, like `timestamp` and `search`.

use crate::document::Document;
use crate::structure::{EditOutcome, Format, StructureProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bullet {
    Dash,
    Plus,
    Star,
    /// `1.` or `1)` — `number` as written, `paren` true for `)`.
    Ordered { number: usize, paren: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListItem {
    pub line: usize,
    pub indent: usize,
    pub bullet: Bullet,
    pub checkbox: Option<CheckState>,
    /// Char col where the content starts (after bullet/checkbox), the cursor target.
    pub content_col: usize,
}

pub fn item_at(doc: &Document, line: usize, format: Format) -> Option<ListItem>;
```

Tests (Org unless stated): `- task` at indent 0/2/5 parses with right indent/bullet/content_col; `+ task` parses; `* task` at col 0 is None in Org (heading) but Some in Markdown; `  * task` (indented) parses in Org; `3. x` and `3) x` parse Ordered with the number and paren flag; `- [ ] t`/`- [X] t`/`- [x] t`/`- [-] t` give the right `CheckState` (x lowercase accepted); `-[ ] t` parses as an item with NO checkbox (content starts at the bracket? no — `-[ ]` has no space after the bullet, so it is NOT an item at all: bullet requires one space. Assert None); `- x [ ] y` has checkbox None; blank line → None; plain text → None; heading line → None; `1.x` (no space) → None; content_col points past `[ ] ` when a checkbox exists (assert an exact number, e.g. `- [ ] t` → 6). In Markdown, a `- item` line inside a fenced code block → None.

- [ ] **Step 2: Verify compile failure** (`cargo test -p torg-core list`).

- [ ] **Step 3: Implement `item_at`.** Parse: count leading spaces (tabs: treat a line with tab-indent conservatively — a tab counts as one char of indent for nesting comparisons; document this in a comment); bullet char or `digits + ('.'|')')`; require one space after; Org rejects `Star` at indent 0; optionally parse `[c]` + trailing space where c ∈ {' ', 'X', 'x', '-'}. Markdown fence check: reuse the provider's mechanism — read `structure.rs`'s MarkdownProvider fence handling first; if its fence-scan helper is private, EITHER make it `pub(crate)` (preferred, minimal) or replicate the scan; state which in the report.

- [ ] **Step 4: Tests pass; workspace + clippy clean.**
- [ ] **Step 5: Commit** — "Parse plain list items in both formats" (list.rs + lib.rs).

---

### Task 2: `toggle_checkbox` + cookie recompute (`update_cookies`)

**Files:** `crates/core/src/list.rs`

- [ ] **Step 1: Failing tests.**
- Toggle: `[ ]`→`[X]` (Org) / `[x]` (Markdown); `[X]`/`[x]`→`[ ]`; `[-]`→`[X]`; no checkbox → `NoOp("No checkbox here")`; non-item line → same NoOp; cursor stays (`EditOutcome::Changed { cursor_line }` = same line).
- Cookies, via toggle: parent `- top [0/2]` with two direct `[ ]` children — toggling one child rewrites parent to `[1/2]`; grandchildren do NOT count toward the grandparent (build a 3-level fixture and assert only the direct parent's cookie changes); `[50%]` form updates by truncation (`[2/3]`-equivalent → `[66%]`); unfilled `[/]` and `[%]` get filled; heading `* Heading [0/2]` counts only TOP-LEVEL checkbox items in its section (nested `[ ]` children excluded — assert); a heading cookie in a DIFFERENT section is untouched; multiple cookies on one line all update; cookie-free parents/headings gain nothing (no insertion); surrounding text preserved exactly.

- [ ] **Step 2: verify failures.**

- [ ] **Step 3: Implement.**

```rust
pub fn toggle_checkbox(doc: &mut Document, line: usize, format: Format) -> EditOutcome;

/// Recompute every [n/m] / [p%] cookie affected by a change on `line`: the chain of
/// parent items above it, and the enclosing heading. Never inserts a cookie.
pub fn update_cookies(doc: &mut Document, line: usize, format: Format);
```

Algorithms (implement as free/private helpers, each unit-testable):
- Region walk: from `line`, scan up while lines are items (`item_at` Some) to find the region start; parent of an item = nearest item above it within the region with strictly smaller indent.
- Direct children of item P: items below P, before the next item with indent ≤ P.indent (or region end), whose indent equals the MINIMUM indent among items in that span deeper than P.
- Heading cookie: `format.parse(doc)` outline → enclosing heading (greatest heading.line ≤ line); section = lines from heading.line+1 to heading's `last_line` (the `Heading` struct has `last_line`; children headings' sections are excluded by using the outline's next heading at ANY level as the section end — read the Heading fields and pick the field that means "this heading's own body"; if only subtree ranges exist, stop the count at the next heading line of any level). Top-level checkbox items = checkbox items in that span with no parent item.
- Cookie rewrite: scan the target line's text for `[` `digits?` `/` `digits?` `]` and `[` `digits?` `%` `]` token spans (also matching the unfilled forms); replace each with the recomputed text; preserve everything else. Char-range `doc.remove`/`doc.insert` edits, rightmost-first so earlier spans stay valid.
- `toggle_checkbox` flips the char at the checkbox position (checkbox col = derivable from `content_col`; store or recompute), then calls `update_cookies(line)`.

- [ ] **Step 4: green + clean.**
- [ ] **Step 5: Commit** — "Toggle checkboxes and keep statistics cookies current".

---

### Task 3: `insert_item` and `indent_item`

**Files:** `crates/core/src/list.rs`

- [ ] **Step 1: Failing tests.**
- Insert: after `- a` (no children) inserts `- ` on the next line, `Changed { cursor_line }` = new line (content position noted for the TUI); after an item WITH children inserts after the whole subtree at the SAME level; current item has checkbox → new item gets `[ ] `; ordered `1.` → new item numbered next and FOLLOWING same-level siblings renumbered (+1 each, `2.`→`3.` etc.; `1)` keeps paren style); at end-of-buffer without trailing newline still works; non-item line → NoOp; cookies recompute (a `[1/2]` parent with a new unchecked child → `[1/3]`).
- Indent/dedent: indent adds 2 spaces to the item line only (children untouched); dedent removes up to 2; dedent at indent 0 → `NoOp("Already at top level")`; Org `* item` at indent 1 dedenting would hit col 0 → NoOp too (would become a heading — refuse, message `Would become a heading`); bullets never rewritten; cookies recompute after indent/dedent (an item indented under a new parent changes both old and new parents' counts — assert one such case; recompute the heading cookie too).

- [ ] **Step 2: verify failures.**
- [ ] **Step 3: Implement** `insert_item(doc, line, format) -> EditOutcome` and `indent_item(doc, line, format, dedent: bool) -> EditOutcome`, both ending with `update_cookies`. Subtree span of an item = its lines through the last line of its deepest descendant (same walk as Task 2). Renumbering: after inserting an ordered item, walk following same-level siblings in the region and rewrite their numbers (+1). Keep functions small; share the region/children walks from Task 2.
- [ ] **Step 4: green + clean.**
- [ ] **Step 5: Commit** — "Insert and re-indent list items".

---

### Task 4: TUI — `Action::ToggleCheckbox`, COMMANDS row, context-sensitive routing

**Files:** `crates/tui/src/action.rs`, `crates/tui/src/commands.rs`, `crates/tui/src/app.rs`

- [ ] **Step 1: Failing tests.**
- action.rs: `Ctrl+Space` (`KeyCode::Char(' ')` + CONTROL — also verify how crossterm delivers NUL: add a second mapping for `KeyCode::Null` if the existing test harness can express it; check crossterm's parse of `\0` and note what you find) and `Alt+X` both → `Some(Action::ToggleCheckbox)`.
- app.rs: toggle on a checkbox item flips it and updates the parent cookie in one keypress; toggle on a plain line sets status "No checkbox here"; `Alt+Enter` on an item line inserts an item (cursor on the new item's content col) while on a heading line it still inserts a sibling heading; `Alt+←/→` on an item line dedents/indents while on a heading line it still promotes/demotes; dedent at top level shows "Already at top level".
- commands.rs drift guard: adding the variant breaks `requires_entry` compilation — the fix (marking it `true` + a `COMMANDS` row in Structure: keys "Ctrl+Space", name "Toggle checkbox", description "Toggle the item's checkbox (Alt+X also works)") is part of THIS task; also extend the tests' `menu_actions()` list.

- [ ] **Step 2: verify failures** (compile error from the drift guard arrives immediately — that is the guard working; fold the required arms in to reach runnable-failing-test state, exactly like the search feature did for `Action::Find`).

- [ ] **Step 3: Implement.**
- `Action::ToggleCheckbox` variant + both key mappings (+ `KeyCode::Null` arm if warranted).
- app.rs: `ToggleCheckbox` arm calls `list::toggle_checkbox(&mut b.doc, line, b.format)` via the same edit-then-reparse helper the other structural edits use (find `structural_edit`/the closure-taking helper at ~app.rs:516 and reuse it or mirror it — cursor sync + status + `reparse()`).
- Context routing: in the `InsertSibling`, `PromoteHeading`, `DemoteHeading` arms, first check `list::item_at(&b.doc, cursor_line, b.format)`; on Some route to `insert_item` / `indent_item(dedent=true/false)`; else the existing heading behavior. Follow `shift_date_or_priority`'s shape. Cursor placement: after insert, `view.move_to(&doc, new_line, content_col)` (the core returns `Changed { cursor_line }`; content col = `item_at` on the new line).
- COMMANDS row + `menu_actions()` entry.

- [ ] **Step 4: green + clean (workspace).**
- [ ] **Step 5: Commit** — "Wire checkbox toggling and list-aware structural keys into the TUI".

---

### Task 5: Rendering + docs + tmux verify

**Files:** `crates/tui/src/ui.rs`, `docs/usage.md`, `docs/guide.md`, `README.md`, `man/torg.1`, spec + this plan

- [ ] **Step 1 (rendering, TDD on the pure function):** extend `highlight_line` (or its span pipeline) to style checkboxes and cookies: `[X]`/`[x]` in the DONE style the file already has for done TODO keywords (find the existing style constants/logic), `[ ]`/`[-]` dimmed, complete cookies (`n == m` or `100%`) DONE-style, incomplete TODO-style. Literal rendering, no concealment. Tests mirror the existing `highlight_line` timestamp tests.
- [ ] **Step 2 (docs):** usage.md key table (+ "Lists and checkboxes" section: syntax, keys, cookie behavior, both formats); guide.md section; README works-today bullet; man/torg.1 entries (`Ctrl+Space`/`Alt+X` + a Lists paragraph; groff -ww clean). Spec status → "approved design, implemented" + record any deviations found during implementation. Tick this plan's boxes. Plain sentences.
- [ ] **Step 3 (tmux, per `.claude/skills/verify/SKILL.md`):** fixture .org: `* Groceries [0/3]` with three `- [ ]` items, one nested `- [ ]` child under the first. Verify: Ctrl+Space (and Alt+X) on item 1 → `[X]` and heading shows `[1/3]` (nested child NOT counted); Alt+Enter on an item → new `- [ ]` line, cursor ready; Alt+→ indents it; Alt+← twice → "Already at top level" status; Alt+Enter on the heading still creates a sibling heading. Repeat the toggle + cookie check in a `.md` fixture with `- [ ]` GFM items and a `[0/2]` cookie. Honest pane-by-pane report; BLOCKED on failure.
- [ ] **Step 4: full gates, commit** — "Render and document lists, checkboxes, and cookies". No push/PR — final review first.

---

## Final phase (controller)

Whole-branch review (spec sweep, cross-task seams — especially cookie recompute vs the TUI's reparse cycle and Markdown/Org divergence — regression risk to heading commands, docs accuracy), fix blockers, then push + PR.

## Self-review notes

- Spec coverage: syntax table → T1; toggle + cookies → T2; insert/indent + renumber → T3; keys/context routing/COMMANDS → T4; rendering/docs/verify → T5. Spec-out items (description lists, reordering, tristate propagation, COOKIE_DATA, list folding, subtree indent) appear in no task.
- The drift guard firing on `Action::ToggleCheckbox` is expected and its resolution is inside Task 4, keeping every commit green.
- Types consistent: `ListItem`/`Bullet`/`CheckState`, `item_at(doc, line, format)`, `toggle_checkbox/insert_item/indent_item -> EditOutcome`, `update_cookies` internal-but-public.
