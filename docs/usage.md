# textr-org usage

A terminal text editor with a first cut of Org-mode–style structure editing. Opens, edits, and
saves multiple buffers, and understands Org `*` headings and Markdown `#` headings (folding,
navigation, TODO cycling).

## Running it

```sh
cargo run -p textr-org-tui -- <file>...   # open one or more files
cargo run -p textr-org-tui                # start with an untitled buffer
```

Installed (e.g. `cargo install --path crates/tui`), the binary is `torg`:

```sh
torg notes.org            # one file
torg notes.org ideas.org  # several files — the first is shown, Alt+N reaches the rest
```

- **A path that exists** is opened.
- **A path that doesn't exist yet** starts an empty buffer; the first `Ctrl+S` writes it to
  that path without prompting.
- **A path given twice** opens once.
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
| `Tab` | On a heading line, fold/unfold its subtree. Elsewhere, insert a tab (displayed at 4-column tab stops). |
| `Ctrl+N` / `Ctrl+P` | Jump to the next / previous heading. |
| `Ctrl+T` | Cycle the current heading's keyword: none → `TODO` → `DONE` → none. |
| `Ctrl+S` | Save (opens the *Save As* prompt for an untitled buffer). |
| `Ctrl+O` | Open a file (or switch to it, if it is already open). |
| `Alt+N` / `Alt+P` | Switch to the next / previous buffer (wraps around). |
| `Ctrl+B` | Open the buffer list — pick an open file with `↑`/`↓` + `Enter` or `1`-`9`. |
| `Ctrl+W` | Close the current buffer (asks `y/n` if it has unsaved changes). |
| `Ctrl+Q` | Quit (asks `y/n` if any buffer has unsaved changes). |
| `Esc` | Cancel a prompt, the buffer list, or a confirmation. |

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

## File formats

The structure features work per buffer, in the format the file's extension names:

- **`.md` / `.markdown`** (any letter case) — Markdown ATX headings: `#` through `######` at
  the start of a line, followed by a space and a title. Same keys as Org: `Tab` folds,
  `Ctrl+N`/`Ctrl+P` navigate, and `Ctrl+T` cycles `# task` → `# TODO task` → `# DONE task` —
  the TODO keywords are torg's convention, carried over from Org.
- **Everything else** — including `.org`, unknown extensions, and untitled buffers — is
  treated as Org.

Details worth knowing:

- Headings inside fenced code blocks (``` or `~~~`) are ignored — a `# comment` in a fenced
  shell script neither folds nor navigates, and `Ctrl+T` leaves it alone.
- The format is re-detected when *Save As* gives a buffer a new name: save an untitled buffer
  as `notes.md` and its outline switches to Markdown on the spot.
- Setext (underlined) headings are not recognized, and a closing hash run (`## title ##`)
  stays part of the title.

## Multiple files

Several files can be open at once; each keeps its own cursor, folds, and scroll position, so
switching away and back puts you exactly where you left off.

- **Open** — pass several paths on the command line, or press `Ctrl+O` and type a path
  (`Enter` opens, `Esc` cancels). A path that is already open — even one still waiting for its
  first save — switches to that buffer instead of opening a second copy. A path that doesn't
  exist yet starts an empty buffer that will save there.
- **Switch** — `Alt+N` / `Alt+P` cycle through the open buffers (wrapping at the ends), or
  press `Ctrl+B` for a list of open files: move with `↑`/`↓` and press `Enter`, or jump
  straight to a buffer with `1`-`9`. Dirty buffers show a `*` in the list. (`Ctrl+B` is the
  default tmux prefix — inside tmux, press it twice or rebind.)
- **Close** — `Ctrl+W` closes the current buffer. If it has unsaved changes you're asked
  `y/n` first. Closing the last buffer leaves a fresh untitled one; quitting stays `Ctrl+Q`.

With more than one file open, the status line gains a position marker: `[2/3] notes.org*`.

## Saving, and *Save As*

- `Ctrl+S` on a buffer that has a file writes it and briefly shows `Saved` on the status line.
- `Ctrl+S` on an **untitled** buffer opens a `Save as:` prompt on the bottom line. Type a path
  and press `Enter` to write it, or `Esc` to cancel. `Backspace` edits the path.
- If a write fails (e.g. a bad directory), the error appears on the status line — the editor
  does not crash and the buffer stays marked unsaved.

## The status line

The bottom row shows, from left: the buffer position (`[2/3]`, only when more than one file is
open), the file name (or `[No Name]` for an untitled buffer), a `*` if there are unsaved
changes, and the cursor position as `line:col` (both 1-based). Transient messages like `Saved`
appear to the right. In the *Save As* and *Open* prompts this row becomes `Save as:` / `Open:`
followed by what you've typed; confirmations and the buffer list put their question or key
hints here too.

```
[2/3] notes.org* — 3:5   Saved
```

## Known limitations (Milestone 2)

This is the first runnable milestone; several things are deliberately out of scope for now:

- **No line wrapping** — long lines are clipped at the right edge (no horizontal scroll).
- **Cursor drift on wide/combining characters** — the cursor is placed by character count, so
  full-width CJK or grapheme clusters can misalign visually.
- **Structure basics only** — no promote/demote or subtree moves, no tables, timestamps,
  agenda, source-block execution, or export yet. See [`roadmap.md`](roadmap.md) for where these
  land (M3–M10).
