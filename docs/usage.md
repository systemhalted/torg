# textr-org usage

A terminal text editor with a first cut of Org-mode–style structure editing. Opens, edits, and
saves a single buffer, and understands Org `*` headings (folding, navigation, TODO cycling).

## Running it

```sh
cargo run -p textr-org-tui -- <file>   # open an existing file (or create it on first save)
cargo run -p textr-org-tui             # start with an untitled buffer
```

Installed (e.g. `cargo install --path crates/tui`), the binary is `torg`:

```sh
torg notes.org
```

- **A path that exists** is opened.
- **A path that doesn't exist yet** starts an empty buffer; the first `Ctrl+S` writes it to
  that path without prompting.
- **No argument** starts an untitled buffer; the first `Ctrl+S` asks where to save.

## Keys

| Key | Action |
|-----|--------|
| `←` `→` `↑` `↓` | Move the cursor. Up/Down keep your column across shorter lines (goal column). |
| `Home` / `End` | Start / end of the current line. |
| `PageUp` / `PageDown` | Move up / down by one screenful. |
| *printable keys* | Insert the character. |
| `Enter` | Split the line (insert a newline). |
| `Backspace` | Delete the character before the cursor; at column 0, join onto the previous line. |
| `Delete` | Delete the character at the cursor; at a line's end, pull the next line up. |
| `Tab` | On a heading line, fold/unfold its subtree. Elsewhere, insert a tab. |
| `Ctrl+N` / `Ctrl+P` | Jump to the next / previous heading. |
| `Ctrl+T` | Cycle the current heading's keyword: none → `TODO` → `DONE` → none. |
| `Ctrl+S` | Save (opens the *Save As* prompt for an untitled buffer). |
| `Ctrl+Q` | Quit. |
| `Esc` | Cancel the *Save As* prompt. |

## Org structure

A line beginning with one or more `*` followed by a space is a **heading**; the number of stars
is its level (`*` = 1, `**` = 2, …). A heading's *subtree* is everything below it up to the
next heading of the same or a shallower level.

- **Folding** — put the cursor on a heading and press `Tab`. Its subtree collapses and the
  heading gains a `…` marker; `Tab` again expands it. Folding a parent hides its children too.
- **Navigation** — `Ctrl+N` / `Ctrl+P` jump straight between headings without scrolling by hand.
- **TODO cycling** — `Ctrl+T` on a heading rotates its workflow keyword:
  `* task` → `* TODO task` → `* DONE task` → `* task`.

The outline is re-read as you type, so turning a line into a heading (or editing one) updates
folding and navigation immediately.

## Saving, and *Save As*

- `Ctrl+S` on a buffer that has a file writes it and briefly shows `Saved` on the status line.
- `Ctrl+S` on an **untitled** buffer opens a `Save as:` prompt on the bottom line. Type a path
  and press `Enter` to write it, or `Esc` to cancel. `Backspace` edits the path.
- If a write fails (e.g. a bad directory), the error appears on the status line — the editor
  does not crash and the buffer stays marked unsaved.

## The status line

The bottom row shows, from left: the file name (or `[No Name]` for an untitled buffer), a `*`
if there are unsaved changes, and the cursor position as `line:col` (both 1-based). Transient
messages like `Saved` appear to the right. In the *Save As* prompt this row becomes `Save as:`
followed by what you've typed.

```
notes.org* — 3:5   Saved
```

## Known limitations (Milestone 2)

This is the first runnable milestone; several things are deliberately out of scope for now:

- **One buffer only** — no tabs or multiple files yet.
- **No line wrapping** — long lines are clipped at the right edge (no horizontal scroll).
- **Cursor drift on wide/combining characters** — the cursor is placed by character count, so
  full-width CJK or grapheme clusters can misalign visually.
- **Org only** — Markdown (`#` headings) structure support arrives in the next milestone.
- **Structure basics only** — no promote/demote or subtree moves, no tables, timestamps,
  agenda, source-block execution, or export yet. See [`roadmap.md`](roadmap.md) for where these
  land (M3–M7).
