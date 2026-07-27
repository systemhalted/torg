# In-buffer incremental search

Date: 2026-07-26 · Status: draft for review, pre-implementation
Reference behavior: Emacs isearch (incremental, origin-anchored, smart case), simplified.

## Goal

Find text in the active buffer without leaving the keyboard flow: `Ctrl+F` opens a prompt,
the cursor jumps to the nearest match live as the query is typed, `Ctrl+F`/`Ctrl+R` step
through matches, `Enter` keeps the position, `Esc` restores it. This is basic-editor
functionality the roadmap never listed (M5's "search view" is the agenda-level, multi-file
feature — this chunk is deliberately smaller and ships first).

## Scope

In: literal-text incremental search in the active buffer, forward and backward, smart
case, wraparound, last-query recall, match highlighting in the viewport.
Out: regex, replace, multi-buffer search, match counters (`k of n`), search history beyond
the single last query, sparse-tree/occur views (M5), multi-line queries (the prompt is one
line; `Enter` never inserts into the query).

## Keys and semantics

| Key | In Edit mode | While the Find prompt is open |
|---|---|---|
| `Ctrl+F` | Open the `Find: ` prompt, pre-filled with the last query (empty on first use); searching starts immediately if pre-filled | Jump to the **next** match after the current one (sets direction forward) |
| `Ctrl+R` | — (unbound in Edit mode) | Jump to the **previous** match (sets direction backward) |
| printable chars / `Backspace` | — | Edit the query; after every edit the search re-runs **from the origin** in the current direction (so backspacing walks back toward the origin match, Emacs-style) |
| `Enter` | — | Accept: close the prompt, cursor stays on the match |
| `Esc` | — | Cancel: close the prompt, restore cursor and viewport to the origin |

Semantics:
- **Origin** = cursor position (and viewport) when the prompt opened; kept in the mode
  state for Esc-restore and as the anchor that query edits re-search from.
- **Smart case**: if the query is all-lowercase the comparison is case-insensitive
  (per-char `to_lowercase` on both sides); any uppercase character makes it exact.
- **Wraparound**: hitting the end (or start, going backward) continues from the other end
  once, with status `Wrapped`; if the query has no match at all, status `Not found` and
  the cursor stays where it was (on the origin if nothing was ever matched).
- Empty query matches nothing: the cursor sits at the origin, no status.
- The match the cursor is on is *found again* only by `Ctrl+F`/`Ctrl+R` stepping —
  stepping searches from just past the current match, query edits search from the origin.
- The status line shows the prompt exactly like the other prompts (`Find: <input>`).

## Design

### Core: new module `crates/core/src/search.rs`

Free functions over `Document`, the `timestamp.rs` shape — no trait involvement, search is
format-independent:

```rust
/// A match: start position and char length (length = query char count).
pub struct Match { pub line: usize, pub col: usize }

/// Find the first match at-or-after (forward) / at-or-before (backward) `from`,
/// wrapping once. Returns None for an empty query or no match.
pub fn find(
    doc: &Document, query: &str, from: (usize, usize), forward: bool, // (line, col) chars
) -> Option<(Match, bool /* wrapped */)>;

/// All match start cols of `query` within a single line's text — for highlighting.
pub fn matches_in_line(text: &str, query: &str) -> Vec<usize>;
```

Both apply smart case internally (a shared `smart_eq` helper). Matching is per-char
case-folded comparison over the rope's chars; no allocation of the whole document.
Registered in `crates/core/src/lib.rs` alongside the `timestamp` re-exports.

### TUI

- `action.rs`: one new variant `Action::Find`, mapped from `Ctrl+F` (Edit mode only —
  prompt-mode keys are handled by the mode handler in `app.rs`, like every other prompt).
- `app.rs`: new mode variant

  ```rust
  Mode::Search { input: String, origin: (usize, usize), origin_top: usize, forward: bool }
  ```

  plus a persistent `last_query: String` field on `App`. The mode handler implements the
  key table above; each re-search calls `search::find` and moves cursor + viewport on a
  hit. `Esc` restores `origin`/`origin_top`. `Enter` stores the query into `last_query`.
- `ui.rs`: prompt arm for `Find: `; while `Mode::Search` is active, `highlight_line` calls
  `search::matches_in_line` on visible lines and styles every occurrence, with the match
  under the cursor in a distinct (current-match) style. Literal rendering, no concealment.
- Help text (`Ctrl+H`) and docs gain the new key.

### Docs

`usage.md` key table + a "Search" section; `guide.md` feature section; README works-today
bullet.

## Testing

- `search.rs` unit tests: forward/backward from mid-document; smart case (`foo` matches
  `Foo`, `Foo` does not match `foo`); wraparound flag both directions; no-match; empty
  query; query at buffer start/end boundaries; multi-byte/Unicode queries; `matches_in_line`
  with overlapping and repeated occurrences.
- `action.rs`: `Ctrl+F` → `Action::Find`.
- `app.rs`: typing narrows/jumps incrementally and backspace returns toward the origin;
  `Ctrl+F` steps forward and `Ctrl+R` back; `Esc` restores cursor and scroll; `Enter`
  keeps position and stores the query; reopening pre-fills and immediately re-finds;
  `Wrapped` / `Not found` statuses.
- End-to-end via the project verify skill (tmux): open a fixture, `Ctrl+F`, type a word
  that first matches below the viewport, assert the view scrolled and the match is
  highlighted; `Esc`, assert the original viewport returned.
- `cargo test --workspace` + `clippy -D warnings` green; `crates/ffi` untouched.

## Sub-project sequencing note

Ships before the categorized help menu (same day's design), so the menu's command table
includes the search keys. The parked `m4-lists` spec follows after both.
