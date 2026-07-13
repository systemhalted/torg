# Structural editing design (M3 completion)

Date: 2026-07-12 · Status: approved design, pre-implementation
Reference semantics: the Org manual — Structure Editing, Priorities, Tags chapters
(https://orgmode.org/org.html).

## Goal

Complete milestone M3: edit the *tree*, not just the text, in both Org and Markdown buffers —
promote/demote headings and subtrees, move subtrees among siblings, insert sibling headings —
plus priority cookies and tag syntax with a tag-editing prompt. Together with the shipped
MarkdownProvider this finishes every M3 line in the roadmap coverage map.

## Scope

In: the operations below, in both formats, with core logic written once against the
`StructureProvider` trait; `Heading` gains `priority` and `tags` fields.
Out: refile/archive (M6), agenda consumption of priorities/tags (M5), tag inheritance (M5),
custom TODO keyword sets (M5), numeric priorities, tag right-alignment, region-wide
promote/demote.

## Operations, keys, semantics

All operations act on the heading whose subtree contains the cursor (the *current heading*);
on a non-heading line outside any subtree they are status-line no-ops.

| Key | Operation | Semantics (per manual) |
|---|---|---|
| `Alt+←` / `Alt+→` | Promote / demote heading | ±1 level on the heading line only; children keep their levels |
| `Alt+Shift+←` / `Alt+Shift+→` | Promote / demote subtree | ±1 level on the heading and every heading in its subtree |
| `Alt+↑` / `Alt+↓` | Move subtree up / down | Swap the whole subtree text with the previous/next **same-level** sibling subtree |
| `Alt+Enter` | Insert sibling heading | New heading at the current heading's level, placed after the current subtree, cursor on its title position |
| `Alt+Shift+Enter` | Insert TODO sibling | Same, with the `TODO` keyword |
| `Shift+↑` / `Shift+↓` | Priority up / down | Cycle the `[#X]` cookie: up = none→C→B→A (stop), down = A→B→C→none (stop); cookie sits after the TODO keyword, before the title |
| `Ctrl+G` | Edit tags | Bottom-line `Tags:` prompt (existing prompt machinery) pre-filled with current tags as `a b c`; Enter writes `:a:b:c:` at the end of the headline (empty input removes tags), Esc cancels |

Edge cases:
- Promote at level 1 → no-op, status `Already at top level`.
- `Alt+Enter` with no enclosing heading (e.g. an empty or heading-less buffer) inserts a
  level-1 heading at the end of the document; every other operation is a no-op there.
- Demote in Markdown beyond level 6 → no-op, status `Markdown headings stop at level 6`
  (subtree demote is refused if **any** heading in the subtree would pass 6).
- Move up with no previous same-level sibling (or down with no next) → no-op with status.
- Subtree operations never cross the parent's boundary.
- Tag characters: letters, digits, `_ @ # %` (manual rule); invalid characters in the prompt
  are rejected with a status message.
- Folds are line-keyed; after a move, `reparse()`'s existing retention drops stale folds.
  Fold state does not travel with a moved subtree (documented simplification).
- Cursor follows the operation: stays on the same heading after promote/demote, follows the
  subtree after a move, lands on the new heading after insert.

## Design

### Core (`crates/core/src/structure.rs`)

`StructureProvider` gains two required primitives and six default-implemented operations:

```rust
pub trait StructureProvider {
    fn parse(&self, doc: &Document) -> Outline;
    fn cycle_todo(&self, doc: &mut Document, line: usize);
    fn marker(&self) -> u8;                 // b'*' (Org) / b'#' (Markdown)  — NEW
    fn max_level(&self) -> Option<usize>;   // None (Org) / Some(6) (Markdown) — NEW

    // default methods, written once (NEW). Each returns whether the document changed,
    // so the TUI can show a no-op status. `line` is any line inside the target subtree.
    fn promote_heading(&self, doc: &mut Document, line: usize) -> EditOutcome;
    fn demote_heading(&self, doc: &mut Document, line: usize) -> EditOutcome;
    fn promote_subtree(&self, doc: &mut Document, line: usize) -> EditOutcome;
    fn demote_subtree(&self, doc: &mut Document, line: usize) -> EditOutcome;
    fn move_subtree_up(&self, doc: &mut Document, line: usize) -> EditOutcome;
    fn move_subtree_down(&self, doc: &mut Document, line: usize) -> EditOutcome;
    fn insert_sibling(&self, doc: &mut Document, line: usize, todo: bool) -> EditOutcome;
    fn cycle_priority(&self, doc: &mut Document, line: usize, up: bool) -> EditOutcome;
    fn set_tags(&self, doc: &mut Document, line: usize, tags: &[String]) -> EditOutcome;
}
```

```rust
pub enum EditOutcome {
    /// The document changed; the cursor should move to this line.
    Changed { cursor_line: usize },
    /// Nothing changed; the message explains why (shown on the status line).
    NoOp(&'static str),
}
``` The default methods work through `parse()` + marker arithmetic (level = marker
run length, the invariant both providers already hold) and `Document` char-range edits.

`Heading` gains:

```rust
pub priority: Option<char>,   // 'A' | 'B' | 'C' from a [#X] cookie after the keyword
pub tags: Vec<String>,        // :a:b: run at the end of the headline, colons stripped
```

Parsing order on a headline: markers → TODO keyword → `[#X]` cookie → title → trailing tag
run. The keyword/cookie/tag helpers are shared by both providers (extending the existing
`split_todo_keyword` chain). Titles no longer include the tag run; the priority cookie is not
part of the title either.

### TUI

- `action.rs`: new `Action` variants (`PromoteHeading`, `DemoteHeading`, `PromoteSubtree`,
  `DemoteSubtree`, `MoveSubtreeUp`, `MoveSubtreeDown`, `InsertSibling`, `InsertTodoSibling`,
  `PriorityUp`, `PriorityDown`, `EditTags`) mapped from the Alt/Alt+Shift/Shift chords and
  `Ctrl+G`. Shift+arrows currently being unbound keeps this conflict-free; plain arrows are
  untouched.
- `app.rs`: each new action calls the trait method on the active buffer's `format`, applies
  the returned cursor line, sets the no-op status when refused, and `reparse()`s. A new
  `Mode::EditTags { input }` reuses the shared prompt handler; Enter validates characters and
  calls `set_tags`.
- `ui.rs`: prompt arm for `Tags: `; no other rendering changes (tags/priority stay inline in
  the headline text).

### Docs

`usage.md` key table + a "Structure editing" section; README works-today bullet; roadmap M3
status updated to done when shipped; architecture.md trait sketch updated with the new
methods/primitives.

## Testing

- Core, per operation × both formats, mirroring the manual: heading-only vs subtree
  promote/demote level changes; refusals at level 1 / Markdown 6 (incl. subtree deep-member
  refusal); move swaps exact text and is a no-op at edges; insert placement after the subtree
  incl. end-of-buffer without trailing newline; priority cycle stops at A/none and sits after
  the keyword; tag parse/write round-trip incl. `_@#%` characters; title excludes cookie and
  tags; fenced `#` lines in Markdown are never operated on.
- TUI: chord → action mappings; one integration test per operation driving `handle_key`;
  tags prompt open/edit/commit/cancel; no-op statuses shown; cursor placement.
- End-to-end tmux session per the project verify skill.

## Sub-project sequencing note

This is the first of the sub-projects decomposing "all core Org features" (see the roadmap
coverage map). Next after this: M4 rich content, starting with either tables or timestamps.
