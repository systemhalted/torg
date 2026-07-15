# The torg guide

A hands-on tour of everything torg can do, with worked examples. torg is a terminal editor
whose editing model is **structure-first**, in the spirit of Emacs Org mode: a document is a
tree of headings you fold, navigate, restructure, and annotate — not just a flat wall of text.

torg speaks two formats and gives them the **same** editing commands:

- **Org** — headings marked with `*` (`*.org` and, by default, anything that isn't Markdown).
- **Markdown** — headings marked with `#` (`*.md` / `*.markdown`).

Throughout this guide, wherever the two formats differ, you'll see paired **In Org** / **In
Markdown** examples. Where there's no pairing, the feature works identically in both.

> This is the narrative walkthrough. For a terse one-screen key reference, see
> [`usage.md`](usage.md); for the project's design, see [`architecture.md`](architecture.md).

## Table of contents

New to editor jargon like *buffer* or *subtree*? Start with
[Concepts and vocabulary](#concepts-and-vocabulary).

1. [First run](#1-first-run)
2. [Moving around and basic editing](#2-moving-around-and-basic-editing)
3. [Files, buffers, and formats](#3-files-buffers-and-formats)
4. [Outlines: headings, folding, navigation](#4-outlines-headings-folding-navigation)
5. [TODO workflow](#5-todo-workflow)
6. [Restructuring the tree](#6-restructuring-the-tree)
7. [Priorities and tags](#7-priorities-and-tags)
8. [Dates and scheduling](#8-dates-and-scheduling)
9. [Org vs Markdown: what differs](#9-org-vs-markdown-what-differs)
10. [Full keybinding reference](#10-full-keybinding-reference)
11. [Limits and what's next](#11-limits-and-whats-next)

---

## Concepts and vocabulary

torg borrows a few terms from Emacs and Org mode. They're worth knowing before the tour —
they show up throughout this guide and on torg's status line.

**Editing**

- **Buffer** — an in-memory working copy of a file that you're editing. Opening a file loads it
  into a buffer; your edits happen in the buffer and only reach the file on disk when you
  **save**. torg can hold several buffers open at once, and each remembers its own cursor
  position, folds, and scroll — so switching between files is instant and lossless. (The term
  comes from Emacs; think "a tab" in other editors, minus the tab bar.)
- **Untitled buffer** — a buffer not yet tied to a file (you started torg with no argument, or
  opened a path that doesn't exist). Its first save asks where to write it.
- **Modified** (or *dirty*) — a buffer with unsaved changes. The status line marks it with a
  `*`; torg warns before you close or quit a modified buffer.
- **Goal column** — the column `↑`/`↓` try to keep as you move through lines of different
  lengths, so paging down and back up returns you to where you started horizontally.
- **Status line** — the bottom row of the screen. It shows the current buffer's name, a `*` if
  modified, the buffer position (`[2/3]`) when several are open, and the cursor's `line:col`.
  It also becomes the **prompt** line for commands that need input (Open, Save As, tags,
  dates) — you type there and press `Enter` or `Esc`.

**Structure**

- **Outline** — the tree of headings torg derives from a buffer. It's re-read as you type, so
  it always matches the text.
- **Heading** — a line that is a node in the outline: `*`-prefixed in Org, `#`-prefixed in
  Markdown. Everything that isn't a heading is body text.
- **Level** — a heading's depth, given by how many markers it has: `*` / `#` is level 1, `**` /
  `##` is level 2, and so on.
- **Subtree** — a heading together with everything beneath it, down to (but not including) the
  next heading of the same or a shallower level. Folding, moving, and promoting a "subtree"
  act on this whole block.
- **Fold** — a collapsed subtree: its body and children are hidden and the heading shows a `…`.
  Folding is a *view* — it never changes the text.

**Metadata (on a heading)**

- **TODO keyword** — a `TODO` or `DONE` state marking a heading as a task.
- **Priority cookie** — a `[#A]`, `[#B]`, or `[#C]` marker (A is highest).
- **Tag** — a `:label:` at the end of a headline; several read `:work:urgent:`.
- **Timestamp** — a date (optionally a time and range) written in brackets. **Active** `<…>`
  timestamps are the kind an agenda would collect; **inactive** `[…]` ones are just notes.
- **Planning line** — the indented `SCHEDULED:` / `DEADLINE:` line torg writes directly below a
  heading (an Org-only concept).
- **Repeater / warning** — cookies inside a timestamp: `+1w` makes it recur, `-2d` sets a
  warning lead.

**Formats**

- **Format** — whether a buffer is parsed as **Org** or **Markdown**. torg picks it from the
  file extension and re-checks on *Save As*; the two share almost all commands (see §9).

---

## 1. First run

Launch torg on a file (installed as the `torg` binary — see [`install.md`](install.md)):

```sh
torg notes.org          # open (or create-on-save) a single file
torg a.org b.md         # open several files at once
torg                    # start with an empty, untitled buffer
```

- A path that **exists** is opened.
- A path that **doesn't exist yet** starts an empty buffer; the first save writes it there.
- With no argument you get an untitled buffer; the first save asks where to put it.

To leave, press **`Ctrl+Q`**. If any buffer has unsaved changes, torg asks `y/n` first rather
than dropping your work.

Everything happens on one screen: your text fills the window, and the **bottom row** is the
status line — it shows which file you're on, whether it's modified, and your cursor position,
and it doubles as the prompt line for commands like *Open* and *Save As*.

---

## 2. Moving around and basic editing

Movement is what you'd expect from any editor:

| Key | Moves the cursor |
|-----|------------------|
| `←` `→` `↑` `↓` | left / right / up / down |
| `Home` / `End` | start / end of the line |
| `PageUp` / `PageDown` | up / down one screenful |

`↑` and `↓` keep a **goal column**: move down through a short line and back onto a long one and
your cursor returns to the column you started from, instead of clamping to the short line.

Editing basics:

- Type any printable character to insert it.
- **`Enter`** splits the line (inserts a newline).
- **`Backspace`** deletes the character before the cursor; at column 0 it joins onto the
  previous line.
- **`Delete`** deletes the character at the cursor; at the end of a line it pulls the next
  line up.
- **`Tab`** inserts a tab *when the cursor isn't on a heading* (tabs display at 4-column
  stops). On a heading line, `Tab` folds instead — see §4.

Save with **`Ctrl+S`**. On a titled buffer it writes and briefly shows `Saved`; on an untitled
one it opens a *Save As* prompt (§3).

---

## 3. Files, buffers, and formats

torg holds several files open at once, each in its own **buffer** that remembers its own
cursor, folds, and scroll position — so switching away and back drops you exactly where you
left off.

### Opening

- **From the shell**: list paths on the command line (`torg a.org b.md`).
- **From inside the editor**: press **`Ctrl+O`**, type a path, `Enter` to open (`Esc` cancels).

The Open prompt is smart about paths:

- A leading **`~`** expands to your home directory, and **`$VAR`** / **`${VAR}`** expand to
  environment values (`~/notes/todo.org`, `$WORK/plan.md`).
- **`Tab` completes** the path against the filesystem: a unique match fills in (directories get
  a trailing `/`), several matches fill to their common prefix, and a further `Tab` with no
  progress lists the candidates on the prompt line. Dotfiles appear only once your partial
  starts with `.`.
- The status line tells you what happened: **`Opened <name>`** when a real file loads, or
  **`New file: <name>`** when the path doesn't exist yet (an empty buffer that will save
  there). That distinction means a mistyped path can't quietly masquerade as a loaded file.
- Opening a path that's **already open** switches to that buffer instead of making a second
  copy.

### Switching, listing, closing

| Key | Action |
|-----|--------|
| `Alt+N` / `Alt+P` | switch to the next / previous buffer (wraps around) |
| `Ctrl+B` | open the buffer list — pick with `↑`/`↓` + `Enter`, or a digit `1`–`9` |
| `Ctrl+W` | close the current buffer |
| `Ctrl+Q` | quit torg |

`Ctrl+W` on a buffer with unsaved changes asks `y/n` first. Closing the *last* buffer leaves a
fresh untitled one rather than quitting. With more than one file open, the status line shows a
position marker: `[2/3] notes.org*` (the `*` means unsaved changes).

> `Ctrl+B` is also the default tmux prefix key — inside tmux, press it twice or rebind tmux.

### Saving and *Save As*

- `Ctrl+S` on a titled buffer writes it (`Saved`).
- `Ctrl+S` on an untitled buffer opens a `Save as:` prompt. Type a path and `Enter`, or `Esc`
  to cancel. The prompt supports the same `~`/`$VAR` expansion and `Tab` completion as *Open*.
- If a write fails (say, a bad directory), the error shows on the status line and the buffer
  stays marked unsaved — torg never crashes on a write error.

### Formats and how torg picks one

Each buffer is parsed as **Org** or **Markdown**, chosen by the file extension:

- **`.md` / `.markdown`** (any letter case) → Markdown.
- **Everything else** — `.org`, `.txt`, `.sh`, no extension, untitled — → **Org** (torg is an
  Org editor first).

The format is **re-detected on *Save As***: start an untitled buffer, write it as `plan.md`,
and its outline immediately switches to Markdown rules. All the structure commands in the rest
of this guide work in both formats; the handful of genuine differences are collected in §9.

---

## 4. Outlines: headings, folding, navigation

This is the heart of torg. A **heading** turns a line into a node in the document tree.

**In Org** — one or more `*` at the start of the line, then a space, then a title. The number
of stars is the level:

```org
* Project
** Design
** Build
* Personal
```

**In Markdown** — one to six `#`, then a space, then a title (`#` = level 1, `######` = level 6):

```markdown
# Project
## Design
## Build
# Personal
```

A heading's **subtree** is everything beneath it up to the next heading of the same or a
shallower level. Above, `Design` and `Build` are the subtree of `Project`; `Personal` starts a
new top-level subtree.

### Folding

Put the cursor on a heading and press **`Tab`**: its subtree collapses and the heading gains a
`…` marker. `Tab` again expands it. Folding a parent hides its children too.

```org
* Project …          ← folded; everything under it is hidden
* Personal
```

Folds are remembered per buffer, so folding, switching files, and coming back leaves your folds
intact.

### Navigating by heading

Instead of scrolling, jump straight between headings:

- **`Ctrl+N`** — next heading.
- **`Ctrl+P`** — previous heading.

The outline is re-read **as you type**, so the moment a line becomes a heading (or stops being
one) folding and navigation update to match.

### Markdown: fenced code is not structure

In Markdown, a `#` at the start of a line inside a fenced code block is a *comment*, not a
heading — torg knows the difference:

````markdown
# Real heading
```sh
# this is a shell comment, NOT a heading
echo hi
```
# Another real heading
````

Folding `# Real heading` collapses the whole fenced block with it, and `Ctrl+N` skips straight
past the shell comment to `# Another real heading`. Fences are recognized with `` ``` `` or
`~~~` (an unclosed fence runs to the end of the file).

---

## 5. TODO workflow

Any heading can carry a **TODO keyword**. Press **`Ctrl+T`** on a heading to cycle it:

```
none  →  TODO  →  DONE  →  none
```

**In Org:**

```org
* Write the report          Ctrl+T →   * TODO Write the report
* TODO Write the report     Ctrl+T →   * DONE Write the report
* DONE Write the report     Ctrl+T →   * Write the report
```

**In Markdown** — identical, with `#`:

```markdown
# Ship v2       Ctrl+T →   # TODO Ship v2
# TODO Ship v2  Ctrl+T →   # DONE Ship v2
```

The `TODO`/`DONE` keywords are torg's convention carried into Markdown (plain Markdown has no
task keywords), so the same muscle memory works in both formats. The keyword sits right after
the markers and before the title, and combines with priorities and tags (§7).

---

## 6. Restructuring the tree

torg edits the *tree*, not just the text. Every command here acts on the **current heading** —
the one whose subtree contains the cursor — so you don't have to be exactly on the heading
line. When a command can't apply, it says why on the status line (e.g. "Already at top level").

### Promote and demote

- **`Alt+←`** / **`Alt+→`** — promote / demote the **heading only** (its children keep their
  level).
- **`Alt+Shift+←`** / **`Alt+Shift+→`** — promote / demote the **whole subtree** (heading and
  all descendants shift together).

**In Org**, demoting a heading adds a star; promoting removes one:

```org
* A            Alt+→ (demote heading)      ** A
** child                                    ** child        ← child unchanged
```

```org
** A           Alt+Shift+← (promote subtree)   * A
*** child                                       ** child     ← child shifts too
```

**In Markdown** the same commands add/remove a `#`:

```markdown
# A            Alt+→        ## A
## child                    ## child
```

Promoting a top-level heading is refused (you can't go above level 1). In **Markdown**, demoting
is refused when it would push any heading past level 6 (`######`) — the status line explains.

### Moving a subtree

- **`Alt+↑`** / **`Alt+↓`** — swap the current subtree with its previous / next sibling of the
  **same level**. The cursor travels with the subtree, and a subtree can never escape its
  parent.

```org
* A          Alt+↓         * B
  body A                     body B
** A child                 * A
* B          →               body A
  body B                   ** A child
```

### Inserting headings

- **`Alt+Enter`** — insert a new **sibling** heading right after the current subtree, and put
  the cursor on it ready for a title.
- **`Alt+T`** — insert a new sibling that already carries `TODO`.

```org
* A            Alt+Enter, type "B"        * A
  body                                      body
                                          * B
```

To make a **child** instead of a sibling, insert a sibling and then `Alt+→` (demote) it. In a
buffer with no headings yet, `Alt+Enter` starts a level-1 heading at the end.

> A note on `Alt+T`: Org's `Alt+Shift+Enter` also inserts a TODO sibling, but `Shift+Enter` has
> no classic terminal escape sequence, so most terminals (and tmux) can't transmit it. `Alt+T`
> always works.

---

## 7. Priorities and tags

### Priorities

A heading can carry a priority cookie **`[#A]`**, **`[#B]`**, or **`[#C]`** (A highest). Cycle
it with **`Shift+↑`** (raise) and **`Shift+↓`** (lower):

```
none  ⟷  [#C]  ⟷  [#B]  ⟷  [#A]
```

Cycling stops at the ends — `Shift+↑` on `[#A]` does nothing. The cookie sits after the TODO
keyword and before the title, in both formats:

```org
* TODO task        Shift+↑ →   * TODO [#C] task
* TODO [#C] task   Shift+↑ →   * TODO [#B] task
```

```markdown
# TODO ship        Shift+↑ →   # TODO [#C] ship
```

> `Shift+↑`/`↓` is context-sensitive: when the cursor sits on a **timestamp** it shifts the
> date instead of the priority (see §8). Off a timestamp, it always means priority.

### Tags

Tags are colon-delimited labels at the **end** of a headline: `:work:urgent:`. Edit them with
**`Ctrl+G`**, which opens a prompt pre-filled with the heading's current tags:

```
* Prepare deck          Ctrl+G, type "work urgent", Enter        * Prepare deck :work:urgent:
```

- Type tags **space-separated** in the prompt; torg writes the `:a:b:` form.
- Submitting an **empty** prompt removes the tag run.
- Tag names may use letters, digits, and `_ @ # %`; an invalid character keeps the prompt open
  with a message.

Tags work the same in Markdown (`# Prepare deck :work:urgent:`). Priorities and tags are parsed
as *data*, so a future agenda view can filter and sort by them.

---

## 8. Dates and scheduling

torg understands Org timestamps as structured data, in both formats.

### Timestamp anatomy

A timestamp is a date in **`<…>`** (active — the kind an agenda would collect) or **`[…]`**
(inactive) brackets, optionally with a time, a time or date range, and repeater/warning cookies:

```
<2024-01-15 Mon>                 active, date only
[2024-01-15 Mon]                 inactive
<2024-01-15 Mon 09:30>           with a time
<2024-01-15 Mon 09:30-11:00>     time range within one day
<2024-01-15>--<2024-01-18>       date range across days
<2024-01-15 Mon +1w>             repeats weekly (+1d/+1w/+1m/+1y; also ++ and .+)
<2024-01-15 Mon +1w -2d>         …with a 2-day warning lead
```

You may type the weekday or omit it — torg fills in (and keeps) the correct one. This all works
inline in **both** Org and Markdown.

### Inserting a timestamp

- **`Alt+.`** — insert an **active** `<…>` timestamp at the cursor.
- **`Alt+i`** — insert an **inactive** `[…]` timestamp.

Both open a prompt; type the date (`2024-01-15`, `2024-01-15 09:30`, a range, or with a cookie —
brackets optional) and press `Enter`.

### Scheduling a heading (Org only)

- **`Alt+S`** — set the heading's **`SCHEDULED`** date.
- **`Alt+D`** — set its **`DEADLINE`**.

These write an indented planning line directly below the heading (both can share the line), and
an empty prompt removes the entry:

```org
* TODO Write the report
  SCHEDULED: <2024-01-15 Mon>            ← Alt+S
* TODO Ship v2
  SCHEDULED: <2024-01-10 Wed> DEADLINE: <2024-01-20 Sat>
```

Planning lines are an **Org concept** — in a Markdown buffer `Alt+S`/`Alt+D` do nothing and say
so on the status line. (Inline `Alt+.`/`Alt+i` timestamps still work in Markdown.)

### Shifting a date under the cursor

Put the cursor on any part of a timestamp and press **`Shift+↑`** / **`Shift+↓`** to bump the
**field under it** — year, month, day, hour, minute, or a cookie's count:

```
<2024-01-15 Mon>      cursor on the day, Shift+↑ →   <2024-01-16 Tue>   (weekday recomputed)
<2024-01-31 Wed>      cursor on the month, Shift+↑ →  <2024-02-29 Thu>   (day clamped to Feb)
```

Days, hours, and minutes **carry** across boundaries (Jan 31 → Feb 1); changing the **month** or
**year** clamps the day to the month's length (so Jan 31 + 1 month is Feb 29 in a leap year,
Feb 28 otherwise). Remember the overloading: `Shift+↑`/`↓` shifts the date only when the cursor
is *on* a timestamp — everywhere else it cycles the priority (§7).

Timestamps and the `SCHEDULED:`/`DEADLINE:` keywords are highlighted in the buffer so they're
easy to spot.

---

## 9. Org vs Markdown: what differs

The commands are the same; only a few underlying rules change with the format.

| Aspect | Org | Markdown |
|--------|-----|----------|
| Heading marker | `*`, `**`, `***`, … (no limit) | `#` … `######` (levels 1–6) |
| Deepest level | unlimited | 6 (demote past it is refused) |
| Chosen for | `.org`, `.txt`, `.sh`, no extension, untitled | `.md`, `.markdown` |
| Fenced code blocks | not special | `#` inside ```` ``` ````/`~~~` is ignored, not a heading |
| `SCHEDULED:` / `DEADLINE:` planning | yes (`Alt+S`/`Alt+D`) | no (command is a no-op) |
| TODO keywords, `[#A]` priorities, `:tags:` | yes | yes (torg's convention) |
| Inline timestamps + date-shift | yes | yes |
| Folding, heading nav, structural editing | yes | yes |

Everything not in that table behaves identically. And because each buffer keeps its own format,
you can have an `.org` and a `.md` file open side by side and every command does the right thing
for whichever one you're in.

---

## 10. Full keybinding reference

**Movement & editing**

| Key | Action |
|-----|--------|
| `←` `→` `↑` `↓` | move (↑/↓ keep a goal column) |
| `Home` / `End` | start / end of line |
| `PageUp` / `PageDown` | up / down a screenful |
| `Enter` | split the line |
| `Backspace` / `Delete` | delete back / forward (joining lines at the edges) |
| `Tab` | fold a heading, else insert a tab |

**Outline & TODO**

| Key | Action |
|-----|--------|
| `Ctrl+N` / `Ctrl+P` | next / previous heading |
| `Ctrl+T` | cycle TODO: none → `TODO` → `DONE` → none |

**Restructuring**

| Key | Action |
|-----|--------|
| `Alt+←` / `Alt+→` | promote / demote heading |
| `Alt+Shift+←` / `Alt+Shift+→` | promote / demote whole subtree |
| `Alt+↑` / `Alt+↓` | move subtree up / down among siblings |
| `Alt+Enter` | insert a sibling heading |
| `Alt+T` | insert a `TODO` sibling heading |

**Priorities, tags, dates**

| Key | Action |
|-----|--------|
| `Shift+↑` / `Shift+↓` | on a timestamp: shift the field; else: raise / lower priority |
| `Ctrl+G` | edit the heading's tags |
| `Alt+S` / `Alt+D` | set `SCHEDULED` / `DEADLINE` (Org only) |
| `Alt+.` / `Alt+i` | insert an active `<…>` / inactive `[…]` timestamp |

**Files & buffers**

| Key | Action |
|-----|--------|
| `Ctrl+S` | save (Save As for an untitled buffer) |
| `Ctrl+O` | open a file (`~`/`$VAR` expansion, `Tab` completion) |
| `Alt+N` / `Alt+P` | next / previous buffer |
| `Ctrl+B` | buffer list |
| `Ctrl+W` | close the current buffer |
| `Ctrl+Q` | quit |
| `Esc` | cancel a prompt, picker, or confirmation |

---

## 11. Limits and what's next

torg is built in small, runnable milestones. What you've read here is everything that works
today; several Org-class features are deliberately still ahead:

- **No agenda yet** — timestamps, priorities, and tags parse as data, but there's no view that
  collects scheduled/TODO items across files. That's the next major milestone.
- **Structure basics** — no tables, plain lists/checkboxes, links, inline markup, or drawers
  yet.
- **No line wrapping** — long lines are clipped at the right edge (no horizontal scroll).
- **Cursor drift on wide/combining characters** — the cursor is placed by character count, so
  full-width CJK or grapheme clusters can misalign visually.

The full arc — agenda, capture/refile, clocking, Babel, export — is mapped in
[`roadmap.md`](roadmap.md), which also traces every feature back to
[Org mode's feature set](https://orgmode.org/features.html).
