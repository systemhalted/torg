# Plain lists, checkboxes, and statistics cookies (M4)

Date: 2026-07-26 · Status: draft for review, pre-implementation
Reference semantics: the Org manual — Plain Lists and Checkboxes chapters
(https://orgmode.org/org.html).

## Goal

Bring structure editing below the heading level: parse plain lists in both formats, toggle
checkboxes, insert and re-indent list items, and keep `[1/3]` / `[50%]` statistics cookies
up to date automatically. This is the first M4 chunk after timestamps and establishes the
in-entry-structure layer and the context-sensitive-key pattern that tables (a later chunk)
will reuse.

## Scope

In: unordered bullets (`-`, `+`, and `*` — see per-format rules), ordered bullets (`1.`,
`1)`), nesting by indentation, checkboxes `[ ]` / `[X]` / `[-]`, statistics cookies `[n/m]`
and `[p%]` on parent items and on headings, recomputed after every checkbox toggle and item
insert; toggle/insert/indent/dedent commands in both formats through one core module.

Out: description lists (`term :: def`), moving items among siblings, ordered-list
renumbering beyond the insert case, parent-checkbox tristate propagation (`[-]` is parsed
and displayed, never written by torg), `COOKIE_DATA` / recursive cookie counting, TODO-based
statistics on headings (M5), list folding, subtree-style indent that drags children,
`org-cycle` on items.

## Syntax recognized

A list item line is: indentation, bullet, one space, then optionally a checkbox and content.

| Piece | Org | Markdown |
|---|---|---|
| Unordered bullet | `-`, `+` at any indent; `*` only when indented ≥ 1 (col-0 `*` is a heading) | `-`, `+`, `*` at any indent (headings are `#`) |
| Ordered bullet | `1.`, `1)` | `1.`, `1)` |
| Checkbox | `[ ]`, `[X]`, `[-]` right after the bullet; written back as `[X]` | same; parses `[x]` too, written back as `[x]` (GFM convention) |
| Statistics cookie | `[n/m]` or `[p%]` anywhere on a parent-item line or a headline | same (torg extension, like TODO keywords in Markdown) |

Nesting: an item's children are the item lines below it with strictly greater indentation,
up to the next line with indentation ≤ its own. Non-item lines end the list region.
Indent unit is two spaces.

## Operations, keys, semantics

Context-sensitive resolution lives in `app.rs` (the `Shift+↑` timestamp-vs-priority
precedent, `app.rs::shift_date_or_priority`): the *action* is unchanged; the app checks
whether the cursor line is a list item and routes accordingly.

| Key | On a list-item line | Elsewhere (unchanged) |
|---|---|---|
| `Ctrl+Space` (mnemonic) / `Alt+X` (fallback) | Toggle checkbox: `[ ]`→`[X]`, `[X]`→`[ ]`, `[-]`→`[X]`; no-op with status if the item has no checkbox | no-op, status `No checkbox here` |
| `Alt+Enter` | Insert a same-level item after this item's subtree, with a fresh `[ ]` if the current item has a checkbox; ordered bullets get the next number and following same-level siblings renumber | Insert sibling heading |
| `Alt+←` / `Alt+→` | Dedent / indent the item line by one unit (children stay put); dedent at column 0 is a no-op, status `Already at top level` | Promote / demote heading |

Both chords follow the Ctrl+H/Ctrl+K precedent: `Ctrl+Space` arrives as NUL in some
terminals and not at all in others, so `Alt+X` must always work; both are documented.

Cookie recompute, after every toggle and insert:
- A cookie on a parent item counts that item's **direct** child checkbox items.
- A cookie on a headline counts the **top-level** checkbox items of the lists in its own
  section text (not in child headings' sections).
- `[n/m]` is written as counted; `[p%]` uses integer truncation (`100*n/m`), so `[2/3]` ↔
  `[66%]`. `[0/0]` and `[100%]`-with-no-boxes are written literally, never removed.
- Cookies are rewritten in place wherever they already appear on the line; torg never
  inserts a cookie on its own (type `[/]` or `[0/0]` yourself, the recompute fills it).
- Manual text edits do **not** trigger recompute (documented simplification, matches Org's
  update-on-command behavior).

Edge cases:
- Toggle/insert/indent on a blank or non-item line: no-op with status.
- `Alt+Enter` on an item whose subtree runs to end-of-buffer without a trailing newline
  still places the new item after it (same rule as `insert_sibling`).
- Checkbox parse requires the space-delimited form after the bullet (`- [ ] task`);
  `-[ ]` or a `[ ]` later in the text is content, not a checkbox.
- Indenting never rewrites bullets (an indented `1.` keeps its number; bullet-style cycling
  is out of scope).
- In Markdown, fenced code lines are never treated as items (reuses the existing fence
  guard the provider applies to `#` lines).
- Cursor stays on the same item after toggle/indent/dedent, lands on the new item's content
  position after insert.

## Design

### Core: new module `crates/core/src/list.rs`

Shape mirrors `timestamp.rs`: a plain data model plus free functions, no trait changes —
list syntax differs per format by only the bullet rules, so functions take the existing
`Format` (from `detect_format`) rather than growing `StructureProvider`.

```rust
pub struct ListItem {
    pub line: usize,
    pub indent: usize,
    pub bullet: Bullet,             // Dash | Plus | Star | Ordered { number, delim }
    pub checkbox: Option<CheckState>, // Unchecked | Checked | Partial
    pub content_col: usize,         // char col where content starts (cursor target)
}

pub fn item_at(doc: &Document, line: usize, format: Format) -> Option<ListItem>;
pub fn toggle_checkbox(doc: &mut Document, line: usize, format: Format) -> EditOutcome;
pub fn insert_item(doc: &mut Document, line: usize, format: Format) -> EditOutcome;
pub fn indent_item(doc: &mut Document, line: usize, format: Format, dedent: bool) -> EditOutcome;
pub fn update_cookies(doc: &mut Document, line: usize, format: Format);
```

All mutations return the existing `EditOutcome` (`structure.rs`) and end by calling
`update_cookies`, which walks up from `line` to the enclosing parent items and the
enclosing heading (via the provider's `parse()` outline) and rewrites any `[n/m]`/`[p%]`
tokens in place — the same in-place headline-rewrite pattern as `cycle_priority`.
Headline cookies stay part of `Heading.title` (parsing is untouched).

Registered in `crates/core/src/lib.rs` with the same re-export style as `timestamp`.

### TUI

- `action.rs`: one new variant, `Action::ToggleCheckbox`, mapped from `Ctrl+Space`
  (`KeyCode::Char(' ')` + CONTROL) and `Alt+X`. No other new actions — `InsertSibling`,
  `PromoteHeading`, `DemoteHeading` are reused.
- `app.rs`: `ToggleCheckbox` calls `list::toggle_checkbox`; the `InsertSibling` /
  `PromoteHeading` / `DemoteHeading` arms first check `list::item_at(cursor_line)` and
  route to `insert_item` / `indent_item` when on an item. No-op statuses surface as usual;
  `reparse()` after changes. Help text (`Ctrl+H`) gains the new chords.
- `ui.rs::highlight_line`: style checkboxes (`[X]` in the DONE style, `[ ]`/`[-]` dimmed)
  and cookies (complete → DONE style, incomplete → TODO style). Literal rendering only —
  no concealment (no line wrapping yet; hiding chars would desync the hardware cursor from
  `View` char columns).

### Docs

`usage.md` key table + a "Lists and checkboxes" section; `guide.md` feature section with
both formats; README works-today bullet; roadmap M4 line updated when shipped.

## Testing

- `list.rs` unit tests, per format: parse table over tricky indents and every bullet form
  (incl. Org col-0 `*` rejected / Markdown accepted, `1)` vs `1.`, `[x]` vs `[X]`, `-[ ]`
  rejected); toggle round-trips and `[-]`→`[X]`; insert placement after an item's subtree,
  checkbox inheritance, ordered renumbering; indent/dedent limits; cookie math (`[0/0]`,
  truncation `[66%]`, parent-item vs heading counting, multiple cookies on one line,
  cookie-in-place rewrite preserving surrounding text).
- `action.rs`: chord → `ToggleCheckbox` for both bindings.
- `app.rs`: context routing — `Alt+Enter` on an item inserts an item, on a heading inserts
  a sibling heading; same for `Alt+←/→`; toggle updates the parent cookie and the heading
  cookie in one keypress; no-op statuses.
- End-to-end via the project verify skill (tmux): fixture `.org` with a 3-item checklist
  under a `* Groceries [0/3]` heading — toggle one box, assert the rendered `[1/3]`;
  repeat with a `.md` fixture using `- [ ]` items.
- `cargo test --workspace` and `clippy -D warnings` green per task; the `crates/ffi` build
  must stay green (no core signature changes expected — additions only).

## Sub-project sequencing note

Second M4 chunk (after timestamps). Next after this: hyperlinks, then inline markup,
tables, drawers/PROPERTIES — see `docs/roadmap.md` M4.
