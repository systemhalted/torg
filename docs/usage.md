# torg usage

A terminal text editor with a first cut of Org-mode–style structure editing. Opens, edits, and
saves multiple buffers, and understands Org `*` headings and Markdown `#` headings (folding,
navigation, TODO cycling).

> This page is the **terse reference**. For a hands-on walkthrough of every feature with
> worked examples in both Org and Markdown, read [`guide.md`](guide.md).

## Running it

```sh
cargo run -p torg-tui -- <file>...   # open one or more files
cargo run -p torg-tui                # start with an untitled buffer
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

`torg --help` prints a usage summary and `torg --version` prints the version. Inside the
editor, `Ctrl+H` opens a categorized help menu: `←`/`→` or `Tab` switches category, `↑`/`↓`
selects a command, `Enter` runs it exactly as if you'd pressed its chord, and `Esc` closes the
menu. `Enter` is also how you reach a chord your terminal swallows before it gets to torg.
`Ctrl+K` opens the same menu, for the few terminals that send `Ctrl+H` as `Backspace`. `Ctrl+U`
still opens the full [guide](guide.md) in a buffer you read like any other and close with
`Ctrl+W`. Installed via a package, this reference is also a man page: `man torg`.

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
| `Ctrl+F` | Find (incremental); `Ctrl+F`/`Ctrl+R` next/previous while open. |
| `Ctrl+N` / `Ctrl+P` | Jump to the next / previous heading. |
| `Ctrl+T` | Cycle the current heading's keyword: none → `TODO` → `DONE` → none. |
| `Alt+←` / `Alt+→` | Promote / demote the current heading (children keep their level). List-aware: on a list-item line, dedent / indent the item instead. |
| `Alt+Shift+←` / `Alt+Shift+→` | Promote / demote the whole subtree. |
| `Alt+↑` / `Alt+↓` | Move the subtree up / down among its same-level siblings. |
| `Alt+Enter` | Insert a sibling heading after the current subtree. List-aware: on a list-item line, inserts a sibling item instead. |
| `Alt+T` (or `Alt+Shift+Enter`*) | Insert a `TODO` sibling heading. |
| `Ctrl+Space` (or `Alt+X`) | Toggle the checkbox on the cursor's list item. |
| `Shift+↑` / `Shift+↓` | Raise / lower the heading's priority: none ↔ `[#C]` ↔ `[#B]` ↔ `[#A]`. |
| `Ctrl+G` | Edit the heading's tags (space-separated in the prompt; empty removes them). |
| `Alt+S` / `Alt+D` | Set the heading's `SCHEDULED` / `DEADLINE` date (Org buffers only). |
| `Alt+.` / `Alt+i` | Insert an active `<…>` / inactive `[…]` timestamp at the cursor. |
| `Shift+↑` / `Shift+↓` | On a timestamp, shift the field under the cursor; elsewhere, change priority. |
| `Ctrl+S` | Save (opens the *Save As* prompt for an untitled buffer). |
| `Ctrl+O` | Open a file (or switch to it, if it is already open). |
| `Alt+N` / `Alt+P` | Switch to the next / previous buffer (wraps around). |
| `Ctrl+B` | Open the buffer list — pick an open file with `↑`/`↓` + `Enter` or `1`-`9`. |
| `Ctrl+W` | Close the current buffer (asks `y/n` if it has unsaved changes). |
| `Ctrl+H` | Open the categorized help menu (`←`/`→`/`Tab` category, `↑`/`↓` select, `Enter` run, `Esc` close). `Ctrl+K` also opens it, for terminals that can't distinguish `Ctrl+H` from `Backspace`. |
| `Ctrl+U` | Open the full guide in the editor. |
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

## Structure editing

The tree itself is editable, in both formats. Every command below acts on the **current
heading** — the one whose subtree contains the cursor — and reports on the status line when
it refuses (top level, Markdown's level-6 ceiling, no sibling to swap with, not inside any
subtree).

- **Promote / demote** — `Alt+←` / `Alt+→` shift just the heading's level; add `Shift` to
  carry the whole subtree along. In Markdown a demote that would push any heading past
  `######` is refused.
- **Move** — `Alt+↑` / `Alt+↓` swap the subtree with its previous / next same-level sibling;
  the cursor travels with it. A subtree can't leave its parent.
- **Insert** — `Alt+Enter` opens a new sibling heading after the current subtree and puts the
  cursor on it, ready for a title; `Alt+T` starts it as `TODO`. (For a child, insert a
  sibling and `Alt+→` it.) In a buffer without headings it starts a level-1 heading at the
  end. \* `Alt+Shift+Enter` also inserts a `TODO` sibling, but only in terminals with
  extended keyboard reporting — `Shift+Enter` has no classic escape sequence, so most
  terminals (and tmux) can't transmit it; `Alt+T` always works.
- **Priorities** — `Shift+↑` / `Shift+↓` cycle a `[#A]`/`[#B]`/`[#C]` cookie after the TODO
  keyword: `* TODO [#A] task`. Cycling stops at the ends (`Shift+↑` on `[#A]` does nothing).
- **Tags** — `Ctrl+G` prompts for space-separated tags and writes them at the end of the
  headline as `:work:urgent:`. Tags may use letters, digits, and `_ @ # %`; an empty prompt
  removes the run. Tags and priorities are parsed as data — the agenda (M5) will use them.

## Lists and checkboxes

A list item line is indentation, a bullet, one space, then optionally a checkbox and content.

| Piece | Org | Markdown |
|---|---|---|
| Unordered bullet | `-`, `+` at any indent; `*` only when indented (column 0 `*` is a heading) | `-`, `+`, `*` at any indent |
| Ordered bullet | `1.`, `1)` | `1.`, `1)` |
| Checkbox | `[ ]`, `[X]`, `[-]` right after the bullet's space; written back as `[X]` | same, plus `[x]` on read; written back as `[x]` |
| Statistics cookie | `[n/m]` or `[p%]`, anywhere on a parent-item line or a headline | same |

A checkbox only counts as one when its closing bracket is followed by a space or the end of
the line: `- [ ] task` is a checkbox, `- [ ]x` is not (the brackets are just text). Nesting is
by indentation — an item's children are the lines below it indented more than it is, up to the
next line indented the same or less.

- **`Ctrl+Space`** (or **`Alt+X`**, for terminals that eat `Ctrl+Space`) — toggle the checkbox
  on the cursor's item: `[ ]` and `[-]` become `[X]`, `[X]` becomes `[ ]`. On an item with no
  checkbox, or a non-item line, it's a no-op and says so on the status line.
- **`Alt+Enter`** — list-aware: on a list-item line, inserts a new sibling item right after
  this item's subtree, with a fresh `[ ]` if the current item has a checkbox; an ordered
  bullet gets the next number and later same-level siblings renumber. Off an item, it inserts
  a sibling heading as usual.
- **`Alt+←`** / **`Alt+→`** — list-aware: on a list-item line, dedent / indent the item by one
  level (its children stay where they are); dedenting at column 0 says `Already at top level`.
  Off an item, these promote / demote a heading as usual.

Statistics cookies are recomputed after every toggle and insert:

- A cookie on a parent item counts that item's **direct** children only — a checkbox nested
  two levels down doesn't count toward its grandparent's cookie.
- A cookie on a heading counts the **top-level** checkbox items in that heading's own section
  text, not items under any child heading.
- `[n/m]` is the count as written; `[p%]` truncates (`100*n/m`), so `[2/3]` reads as `[66%]`.
  When there's nothing to count, torg writes `[0/0]` and `[0%]` rather than removing the
  cookie.
- Recompute only rewrites cookie-shaped tokens that are already on the line — on the parent
  items, the heading, and the moved item itself. torg never inserts a cookie where none
  exists; type `[/]` or `[0/0]` yourself and the next toggle or insert fills it in.

Both formats work the same way:

```org
* Groceries [1/3]
- [X] milk
- [ ] eggs
- [ ] bread
```

```markdown
- [x] milk
- [ ] eggs
```

Checked boxes and complete cookies are highlighted like a `DONE` keyword; unchecked boxes are
dimmed and incomplete cookies are highlighted like `TODO`.

## Dates and scheduling

torg understands Org timestamps as data. A timestamp is a date in `<…>` (active — the kind
that would show up in an agenda) or `[…]` (inactive) brackets, optionally with a time, a time
range, a second date for a `--` range, and repeater/warning cookies:

```
<2024-01-15 Mon>            <2024-01-15 Mon 09:30>       <2024-01-15 Mon 09:30-11:00>
[2024-01-15 Mon]            <2024-01-15>--<2024-01-18>    <2024-01-15 Mon +1w -2d>
```

The weekday is optional when you type one — torg fills in (and keeps) the correct day.

- **Schedule / deadline** — `Alt+S` and `Alt+D` prompt for a date and write it on an indented
  planning line directly below the heading (`  SCHEDULED: <…>` / `  DEADLINE: <…>`; both can
  share the line). Submitting an empty prompt removes that entry. These are Org-only.
- **Insert a timestamp** — `Alt+.` (active) / `Alt+i` (inactive) prompt for a date and drop
  the timestamp in at the cursor, in any buffer.
- **Type the date** as `2024-01-15`, `2024-01-15 09:30`, a range, or with cookies — the same
  grammar shown above, brackets optional in the prompt.
- **Shift a field** — put the cursor on any part of a timestamp and press `Shift+↑` / `Shift+↓`
  to bump the field under it (year, month, day, hour, minute, or a cookie's count). Days, hours,
  and minutes carry; changing the month or year clamps the day to the month's length
  (Jan 31 → Feb 29 in a leap year). Off a timestamp, `Shift+↑`/`↓` still cycles the priority.

Timestamps and the `SCHEDULED:`/`DEADLINE:` keywords are highlighted in the buffer.

## Search

`Ctrl+F` opens the *Find* prompt. Matching is incremental: the cursor jumps to the
nearest match as you type. While the prompt is open, `Ctrl+F` steps to the next match
and `Ctrl+R` to the previous one; the search wraps around the buffer (status: `Wrapped`).
`Enter` closes the prompt and keeps your place; `Esc` returns to where you started.
A query in all-lowercase matches case-insensitively; any capital letter makes it exact.
`Ctrl+F` remembers the last query — press it twice to repeat a search. Searches are
literal text (no regular expressions).

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
  (`Enter` opens, `Esc` cancels). A leading `~` and `$VAR`/`${VAR}` are expanded, so
  `~/notes/todo.org` works; relative paths resolve against torg's working directory. The
  status line confirms which happened — `Opened <name>` when an existing file loads, or
  `New file: <name>` when the path doesn't exist yet (an empty buffer that will save there).
  A path that is already open — even one still waiting for its first save — switches to that
  buffer instead of opening a second copy.
- **Tab-complete** — press `Tab` in the *Open* or *Save As* prompt to complete the path
  against the filesystem: a single match fills in (directories gain a trailing `/`), several
  matches fill in as far as they share a prefix, and pressing `Tab` again with no further
  progress lists the candidates after the prompt. Dotfiles are shown only once the partial
  name starts with `.`.
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
  and press `Enter` to write it, or `Esc` to cancel. `Backspace` edits the path. As in the
  *Open* prompt, a leading `~` and `$VAR`/`${VAR}` are expanded, and `Tab` completes paths.
- If a write fails (e.g. a bad directory), the error appears on the status line — the editor
  does not crash and the buffer stays marked unsaved.

## The status line

The bottom row, in normal editing, shows from left to right:

```
[2/3] notes.org* — 3:5   Saved
```

- **`[2/3]`** — buffer position: current buffer / total open. Shown **only** when more than one
  buffer is open (hidden with a single file). Opening the guide with `Ctrl+U` adds a buffer, so
  this is why you might see `[2/2]` appear; the `Ctrl+H` help menu does not — it replaces the
  view without opening a buffer.
- **name** — the file name, `[No Name]` for an untitled buffer, or `*torg guide*` for the guide
  buffer.
- **`*`** — the buffer has unsaved changes.
- **`3:5`** — cursor `line:col`, both 1-based.
- **`Saved`** — a transient message (also `Opened <file>`, `New file: <file>`, completion
  hints, errors); cleared on the next keystroke.

The same row becomes the prompt line for `Open:` / `Save as:` / `Tags:` / `Scheduled:` /
`Deadline:` / timestamp input, the `Ctrl+B` buffer picker, and `y/n` confirmations.

## Known limitations

Several things are deliberately still ahead (see [`roadmap.md`](roadmap.md)):

- **No line wrapping** — long lines are clipped at the right edge (no horizontal scroll).
- **Cursor drift on wide/combining characters** — the cursor is placed by character count, so
  full-width CJK or grapheme clusters can misalign visually.
- **No tables, links, agenda, source-block execution, or export yet** — see
  [`roadmap.md`](roadmap.md) for where these land (M4–M10). Timestamps parse as data and can
  be edited, but there is no agenda that collects them yet (M5).
