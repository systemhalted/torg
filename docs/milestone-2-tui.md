# Milestone 2 — `textr` TUI + Org outline core

> Living design/implementation doc for Milestone 2. Built test-first in small batches
> (see the TDD plan at the bottom). Progress is tracked against those steps.
>
> **Scope note (redefined).** M2 was originally "a terminal editor for one buffer." It is now
> the runnable editor **plus** the first cut of textr's structure layer — a format-agnostic
> outline in the core (Org provider first) surfaced in the TUI as folding, heading navigation,
> and `TODO`/`DONE` cycling. This is where the [north-star architecture](roadmap.md) is laid
> down; the heavier Org capabilities (structural editing, tables, agenda, babel, export) are
> mapped across M3–M7 in [`roadmap.md`](roadmap.md) and are **out of scope** here.

## Progress

All parts complete — the editor is runnable and green (~70 unit tests; verified end-to-end
through a pty). Rustdoc, `docs/usage.md`, and `docs/tutorial.md` landed with the code.

**Part A — base runnable TUI**
- [x] **A0 — `Document` line/char accessors** (`line_len_chars`, `line_to_char`,
  `char_to_line`, `line_text`, `char_count`).
- [x] **A1 — `View` movement** (scaffold + horizontal + vertical/goal column + paging)
- [x] **A2 — `View` editing** (insert / newline / backspace / delete)
- [x] **A3 — Workspace wiring** (`crates/tui` member + `ratatui`/`crossterm`)
- [x] **A4 — `key_to_action`**
- [x] **A5 — `viewport_top`**
- [x] **A6 — `App` mode logic** (Edit / Save As prompt)
- [x] **A7 — Render + driver** (`ui.rs` render; `terminal.rs`/`main.rs` lifecycle + panic hook)

**Part B — Org structure core**
- [x] **B1 — Outline + Org `parse`** (`structure` module, `OrgProvider::parse`)
- [x] **B2 — Subtree extent + navigation** (`last_line`, `next/prev/parent_heading`)
- [x] **B3 — `cycle_todo`** (None → TODO → DONE → None)

**Part C — TUI structure surface**
- [x] **C1 — Actions + folding state** (`ToggleFold`/`NextHeading`/`PrevHeading`/`CycleTodo`,
  `App.folded`, extended `key_to_action`)
- [x] **C2 — Render + cues** (skip folded ranges, fold marker, heading style)

## Context

`textr` is a from-scratch gedit clone in Rust, built to learn the language while mirroring
gedit's architecture: a headless, UI-agnostic core with thin frontends on top. Today only
`textr-org-core` exists (a `Document` over a ropey rope with open/save/edit + a `modified`
flag). There is **no frontend and no binary**, so textr is not yet runnable as an editor.

This milestone delivers the first runnable program: a terminal UI that opens a file, lets
you move the cursor and edit, and saves. It also lays down the reusable model layer the
later GUI will share — specifically a headless `View` (cursor + editing intent) in core,
mirroring gedit's `GeditView`/insert-mark over the buffer. The hard rule from
`crates/core/Cargo.toml` stands: **core must compile and be fully testable with no terminal
or windowing dependency**; the TUI depends on core, never the reverse.

Scope is deliberately minimal (open/edit/save one buffer). Command dispatch (M3), tabs
(M4), syntax highlighting / line numbers / find-replace (M5), and the GUI (M7) are separate
later milestones and are **out of scope** here.

Decisions confirmed with the user:
- **Save As prompt**: saving a buffer with no filename opens a bottom-line text input.
- **Feature width**: core editing/navigation set **plus** PageUp/PageDown.

## Architecture

Layers, each pushing logic down so the untested surface is tiny:

1. **`textr-org-core::view::View`** — headless cursor model: position, goal column, and all
   editing intent (move / insert / newline / backspace / delete). Pure, no terminal,
   fully unit-tested. Does **not** own the `Document`; methods take `&Document` /
   `&mut Document` (the `App` owns both). gedit analog: the view references the buffer.
2. **`textr-org-core::structure`** — the format-agnostic outline layer: parses a buffer into an
   `Outline` of `Heading`s (level, title, TODO state, subtree extent) behind a
   `StructureProvider` trait, with `OrgProvider` as the first implementer. Pure, headless,
   fully unit-tested. This is the spine of the [north star](roadmap.md) — folding now;
   promote/demote, agenda, and export later are all written once against the trait.
3. **`crates/tui`** — thin shell: owns the terminal (crossterm raw mode + alt screen),
   the blocking event loop, key→action mapping, the Save-As prompt mode, folding state, pure
   viewport math, and ratatui rendering. No buffer/editing logic of its own.
4. **Pure helpers in the TUI** — `viewport_top(...)` (scroll math) and `key_to_action(...)`
   are free functions with their own unit tests, so the genuinely untestable code is just
   terminal setup/teardown, the blocking `event::read()`, and the `terminal.draw()` call.

**Two separations are enforced** (see [`architecture.md`](architecture.md) for the full map):

- **core vs frontend** — `tui → core`, never the reverse; core compiles and is fully testable
  with no terminal. **Scroll and folding live in the TUI** (they depend on terminal height /
  presentation, which core must never know about); the *outline* they act on is derived and
  headless. Keeping scroll/folding as pure functions/state preserves the rule while staying
  testable.
- **UI vs terminal, *inside* tui** — all crossterm / raw-mode / `event::read` / panic-hook code
  is quarantined in `terminal.rs` + `main.rs`. Above it, `ui.rs` renders a ratatui `Frame` with
  no crossterm, and `app.rs`/`action.rs`/`viewport.rs` are pure and terminal-free. This is what
  lets the future GUI reuse the state + render tiers with only a new driver.

## Crate / module layout

New files:
```
crates/core/src/view.rs        # headless View: cursor + editing intent
crates/core/src/structure.rs   # Outline/Heading/TodoState + StructureProvider + OrgProvider
crates/tui/Cargo.toml          # new crate manifest (binary target `torg`)
crates/tui/src/main.rs         # CLI arg → initial Document → run; only crossterm-adjacent glue
crates/tui/src/terminal.rs     # terminal driver: raw mode/alt screen, event loop, panic hook
crates/tui/src/app.rs          # App: owns Document+View+mode+folded; tick/apply; key dispatch
crates/tui/src/action.rs       # Action enum + key_to_action(KeyEvent) -> Option<Action>
crates/tui/src/viewport.rs     # pure viewport_top(...) + tests
crates/tui/src/ui.rs           # ratatui render: body (fold-aware) + status line + prompt + cursor
```

Modified files:
- `crates/core/src/lib.rs` — add `pub mod view;` and `pub mod structure;` (each wired in the
  same commit as that module's first real code, so it is never an orphan module).
- `crates/core/src/document.rs` — add read-only accessors (below), test-first.
- `Cargo.toml` (root) — add `crates/tui` member; add `ratatui = "0.29"` and
  `crossterm = "0.28"` to `[workspace.dependencies]` (pin crossterm to the version
  ratatui 0.29 re-exports so there is one crossterm in the tree). `core`'s manifest is
  **untouched** — it never sees ratatui/crossterm.

`crates/tui/Cargo.toml`: package `textr-org-tui`, inheriting workspace version/edition/license;
`[[bin]] name = "textr"`; deps `textr-org-core = { path = "../core" }`, `ratatui.workspace`,
`crossterm.workspace`. Run as `cargo run -p textr-org-tui -- <file>`; ships as `textr <file>`.

## Document accessors (add to `document.rs`, test-first, before `view.rs`)

The View needs line geometry but must not reach into ropey directly. Add four thin wrappers
(map to `line`, `len_chars`, `line_to_char`, `char_to_line`); do **not** expose the rope:

```rust
pub fn line_len_chars(&self, line: usize) -> usize; // chars in line EXCLUDING trailing '\n'; OOB -> 0
pub fn line_to_char(&self, line: usize) -> usize;   // char idx of first char of line
pub fn char_to_line(&self, char_idx: usize) -> usize;
pub fn line_text(&self, line: usize) -> String;     // line incl. trailing '\n' if present (renderer)
```

The only non-trivial bit is `line_len_chars` stripping the trailing break — see edge cases.

## `View` design (`crates/core/src/view.rs`)

State: cursor as `(line, column)` in **char** units plus a separate `goal_column`
(gedit's preferred x). Vertical moves preserve `goal_column` so the caret returns to its
column after passing over short lines; horizontal moves and edits reset it. The flat char
index for `Document::insert`/`remove` is **derived on demand** (`line_to_char(line)+column`),
never stored — single source of truth.

```rust
pub struct View { line: usize, column: usize, goal_column: usize }

impl View {
    pub fn new() -> Self;                 // (0,0), goal 0
    pub fn cursor_line(&self) -> usize;
    pub fn cursor_column(&self) -> usize;
    pub fn cursor_char_idx(&self, doc: &Document) -> usize; // line_to_char(line)+column

    // horizontal (reset goal_column)
    pub fn move_left(&mut self, doc: &Document);   // col 0 wraps to end of prev line
    pub fn move_right(&mut self, doc: &Document);  // line end wraps to start of next line
    pub fn move_home(&mut self);
    pub fn move_end(&mut self, doc: &Document);

    // vertical (preserve goal_column, clamp column to target line len)
    pub fn move_up(&mut self, doc: &Document);
    pub fn move_down(&mut self, doc: &Document);
    pub fn move_page_up(&mut self, doc: &Document, page: usize);
    pub fn move_page_down(&mut self, doc: &Document, page: usize);

    // editing (compute char idx, call Document, update cursor, reset goal_column)
    pub fn insert_char(&mut self, doc: &mut Document, ch: char);
    pub fn insert_newline(&mut self, doc: &mut Document);
    pub fn backspace(&mut self, doc: &mut Document); // col 0 joins with previous line
    pub fn delete(&mut self, doc: &mut Document);     // line end joins next line up
}
```

An internal `clamp(doc)` runs after every move/edit so the cursor can never sit past the
buffer or a line's last char.

## Cursor ↔ char-index mapping & edge cases (each gets a test)

- `(line,column) -> char_idx` = `doc.line_to_char(line) + column`. Rope math stays in core
  (Document wrappers); the TUI does no buffer math.
- **Empty buffer**: `len_lines()==1`, line len 0, cursor (0,0); backspace/delete are no-ops.
- **ropey trailing-empty-line**: `"a\nb\n"` reports 3 lines (`"a\n"`,`"b\n"`,`""`). The caret
  may rest on the final empty line at `(2,0)` but no further. `line_len_chars` strips the
  trailing `\n`, so End on `"a\n"` lands at column 1.
- **End of buffer / line**: `move_down` on last line, `move_right` at last position, and
  `delete` at end-of-buffer are no-ops (clamp).
- **Vertical clamp**: `column = min(goal_column, line_len_chars(target))`, goal unchanged.
- **Unicode**: char semantics throughout (ropey is char-indexed) — no byte math. Grapheme
  clusters / wide-char display width are a documented simplification deferred past M2.

## `structure` design (`crates/core/src/structure.rs`)

The format-agnostic outline layer. A buffer parses into a flat, level-tagged list of headings;
a heading's subtree extent gives its fold range. **Format knowledge lives behind a trait**, so
folding (now) and promote/demote, agenda, and export (later) are written once and work for
every format. No new crate dependency — hand-rolled line scanning is enough for `*`-prefixed
headings and keeps core dependency-light.

```rust
pub struct Outline { pub headings: Vec<Heading> }

pub struct Heading {
    pub level: usize,            // 1-based: Org "* "=1, "** "=2  (Markdown "# "=1 in M3)
    pub line: usize,             // line index the heading sits on
    pub title: String,           // heading text, keyword + stars stripped
    pub todo: Option<TodoState>, // parsed leading TODO keyword, if any
    pub last_line: usize,        // last line of this heading's subtree → fold range line+1..=last_line
}

pub enum TodoState { Todo, Done } // custom keyword sets / priorities / tags deferred to M3

pub trait StructureProvider {
    fn parse(&self, doc: &Document) -> Outline;           // scan lines → headings
    fn cycle_todo(&self, doc: &mut Document, line: usize); // None → TODO → DONE → None on that line
}

pub struct OrgProvider; // recognizes ^(\*+)\s+(?:(TODO|DONE)\s+)?(rest)$
```

- **Subtree extent** (`last_line`): from a heading, everything up to (not including) the next
  heading of level ≤ its own, else end of buffer. Fold range = `line+1..=last_line`.
- **Navigation** — pure free functions over `&Outline` + a cursor line: `next_heading`,
  `prev_heading`, `parent_heading` return a target line (or `None` at the ends / top level).
  No `Document` mutation.
- **`cycle_todo`** — the one structural *edit* in M2. The Org provider rewrites the heading
  line's keyword via `Document::remove`/`insert` on char indices derived from `line_to_char`
  (char math only, no bytes), and the edit marks the Document modified. A no-op when the target
  line is not a heading.
- **Heading recognition edge cases** (each gets a test): `* ` with no title is body, not a
  heading; `*not a heading` (no space after the stars) is body; a `*` not at column 0 is body;
  `TODO`/`DONE` count only as the first whitespace-delimited word after the stars.

## TUI shell (`crates/tui`)

**Lifecycle (`main.rs` + `terminal.rs`, panic-safe):** `main.rs` parses
`std::env::args().nth(1)`. If a path is given and exists → `Document::open(path)`; if given
but missing → empty buffer with the intended `PathBuf` stashed in `App` (first Ctrl+S calls
`save_as` on it). No arg → untitled buffer. `terminal.rs` owns the driver: enable raw mode +
`EnterAlternateScreen`, build `ratatui::Terminal<CrosstermBackend>`, run the blocking event
loop, and restore on teardown. **Install a panic hook that restores the terminal first**
(disable raw mode, leave alt screen) and tear down unconditionally on every exit path (incl.
`Err`) — the single most important shell correctness detail. These two files are the *only*
ones that touch crossterm.

**Modes (`app.rs`):**
```rust
enum Mode { Edit, SaveAs { input: String } }
```
- **Edit** — `Ctrl+S`: if `doc.path()` is some (or a stashed path exists) → `save()`/
  `save_as(stashed)`, write result to the status line; else enter `SaveAs { input: "" }`.
- **SaveAs** — printable char appends to `input`, Backspace pops, **Enter** → `save_as(input)`
  then back to Edit (report success/error on the status line), **Esc** → cancel to Edit.

`App` dispatches keys by mode (`handle_key`), so in SaveAs mode keys edit the prompt instead
of the buffer. Editor keys in Edit mode go through `key_to_action`. All of this is pure
state mutation on `App` (a `tick(action)` / `handle_key` seam), so it is unit-testable
without a terminal.

**Event loop:** blocking `event::read()` (no polling → zero idle CPU); filter to
`KeyEventKind::Press` to avoid Windows key-repeat double-fire. `Event::Resize` just lets the
next draw re-read the size.

**`Action` enum + `key_to_action` (pure):** editing/nav — `MoveLeft/Right/Up/Down`,
`MoveHome/End`, `PageUp/PageDown`, `InsertChar(char)`, `Newline`, `Backspace`, `Delete`,
`Save`, `Quit`; structure — `ToggleFold`, `NextHeading`, `PrevHeading`, `CycleTodo`.
Arrows→Move*, Home/End, PageUp/PageDown→paging, Enter→Newline, Backspace/Delete,
`Char(c)` w/o CONTROL→InsertChar, `Ctrl+s`→Save, `Ctrl+q`→Quit, else `None`.

**Structure keymap (provisional).** Real Org chords (`C-c C-t`, …) are a later keymap concern;
M2 uses single-key bindings:
- `Tab` → `ToggleFold`. `key_to_action` emits a single `ToggleFold` intent; **`App` decides**
  whether the cursor is on a heading line (it has the `Outline`) — folds if so, else inserts a
  tab. This keeps the heading check out of the pure key map while giving the authentic
  `org-cycle` feel (Tab on a heading folds).
- `Ctrl+T` → `CycleTodo`; `Ctrl+N` → `NextHeading`; `Ctrl+P` → `PrevHeading`.

**Folding state.** `App` holds `folded: HashSet<usize>` (heading start lines) and re-derives
the `Outline` (via `OrgProvider::parse`) after any edit so fold ranges stay correct. Fold
toggling, next/prev-heading cursor moves, and TODO cycling are pure `App` mutations reusing the
Part-B core ops, so they are unit-tested without a terminal.

**Viewport (`viewport.rs`, pure):**
```rust
pub fn viewport_top(cursor_line: usize, top: usize, height: usize) -> usize {
    if height == 0 { top }
    else if cursor_line < top { cursor_line }
    else if cursor_line >= top + height { cursor_line - height + 1 }
    else { top }
}
```
`App` keeps `scroll_top`; before render `scroll_top = viewport_top(view.cursor_line(),
scroll_top, body_height)` with `body_height = term_height - 1` (1 row for the status line).

**Render (`ui.rs`, no crossterm):** a pure ratatui `Frame` render from `&App`/`&Document`.
`Layout` splits body (height-1) and a 1-row status line. Body draws lines
`scroll_top..scroll_top+body_height` via `doc.line_text(n)` (trailing `\n` stripped), no
wrapping (long lines clipped; horizontal scroll out of scope). **Fold-aware:** lines inside a
folded heading's `line+1..=last_line` range are skipped, and a folded heading is marked with a
trailing `…` (or a `▸`/`▾` twisty); heading lines may carry a subtle style. Status line
(reversed style): `" {name|[No Name]}{* if modified} — {line+1}:{col+1} "` plus transient save
messages. In SaveAs mode the status line instead shows `"Save as: {input}"`. Place the real
hardware cursor via `frame.set_cursor_position((col, screen_row))` in Edit mode (where
`screen_row` accounts for skipped folded lines), or at the prompt input end in SaveAs mode.
Wide-char cursor drift is a known M2 limitation.

The blocking `event::read()`, raw-mode/alt-screen enter/leave, `CrosstermBackend` wiring, and
the panic hook live in `terminal.rs` + `main.rs` — the *only* crossterm-touching files. `ui.rs`
above them is driver-agnostic, so a future GUI reuses it unchanged.

## TDD plan (red → green order)

### Part A — base runnable TUI

- **A0. Document accessors** (`document.rs`): `line_len_chars` strips trailing `\n`
  (`"ab\ncd\n"`→2,2,0; `"ab\ncd"` line1→2); `line_to_char` (0,3,6); `char_to_line`
  (0→0,3→1,6→2); `line_text` incl. `\n`; empty buffer (len 0, 1 line). *(done — M1)*
- **A1. View movement**: new→(0,0); right wraps at line end; left wraps at col 0, no-op at
  (0,0); up/down clamp column; **goal column preserved** across a short line and **reset**
  by a horizontal move; home→0 / end→line len; up on line 0 & down on last line no-op;
  caret can reach the phantom trailing line and no further; **PageUp/PageDown** move by
  `page` lines and clamp at top/bottom.
- **A2. View editing**: insert_char on empty → "x", (0,1), modified; insert mid-line shifts +
  advances; insert_newline splits, cursor→(line+1,0); backspace mid-line removes preceding;
  backspace at col 0 **joins** previous line; backspace at (0,0) no-op; delete removes char
  at cursor; delete at line end **joins** next line; delete at end-of-buffer no-op; Unicode
  `'é'` advances by one char; `cursor_char_idx == line_to_char(line)+column`.
- **A3. Workspace wiring**: add `crates/tui` member + `ratatui`/`crossterm` deps; crate
  skeleton builds.
- **A4. `action::key_to_action`**: arrows→Move*, PageUp/Down, `Ctrl+s`→Save, `Ctrl+q`→Quit,
  Enter→Newline, Backspace, plain char→InsertChar, `Ctrl+a`→None, key-release ignored.
- **A5. `viewport::viewport_top`**: visible→unchanged; below→`cursor-height+1`; above→`cursor`;
  height 0→unchanged; exactly one off bottom→scroll by one.
- **A6. `App` mode logic** (pure, no terminal): Ctrl+S with a path saves & clears modified;
  Ctrl+S without a path enters SaveAs; in SaveAs, chars build input, Backspace pops, Enter
  calls `save_as` & returns to Edit (file written, `*` cleared), Esc cancels leaving the
  buffer unsaved. Drive via `App::handle_key` / `tick` on an in-memory temp path.
- **A7. Render + driver** (`ui.rs` render + `terminal.rs`/`main.rs` lifecycle + panic hook) —
  manual verification only; kept logic-free so the untested surface is a few dozen lines of
  glue, all in the driver tier.

### Part B — Org structure core

- **B1. Outline + Org `parse`**: `"* A\n** B\n* C"` → 3 headings, levels `[1,2,1]`, titles
  `["A","B","C"]`; `"* TODO write\n"` → `todo == Some(Todo)`; `"* DONE ship"` → `Some(Done)`;
  `"* "` (no title) and `"*not a heading"` (no space) and a non-column-0 `*` are body, not
  headings; empty buffer → no headings.
- **B2. Subtree extent + navigation**: for `"* A\ntext\n** B\n* C"`, A spans lines 0–2, B
  spans line 2 only, C spans line 3; `next_heading`/`prev_heading` jump to the adjacent
  heading line and are no-ops at the ends; `parent_heading` from a level-2 heading returns the
  nearest preceding level-1 line, `None` at top level.
- **B3. `cycle_todo`**: `"* task"` → `"* TODO task"` → `"* DONE task"` → `"* task"`; marks the
  Document modified; a no-op leaving the buffer unchanged when the target line is not a heading.

### Part C — TUI structure surface

- **C1. Actions + folding state** (pure `App`, no terminal): `key_to_action` maps `Tab`→
  `ToggleFold`, `Ctrl+T`→`CycleTodo`, `Ctrl+N`/`Ctrl+P`→`NextHeading`/`PrevHeading`;
  `ToggleFold` on a heading line adds/removes it from `App.folded`, on a non-heading line
  inserts a tab; `NextHeading`/`PrevHeading` move the cursor via the Part-B nav helpers;
  `CycleTodo` calls the provider and re-derives the outline; an edit that shifts lines
  re-derives the outline so fold ranges stay correct.
- **C2. Render + cues** (`ui.rs`) — manual verification only: folded ranges skipped, fold
  marker shown, heading style applied, cursor placed at the right screen row.

**Honestly not unit-tested:** terminal enter/leave + panic hook, the blocking
`event::read()`, and the `terminal.draw()`/render calls. Everything with branching (View,
`structure`/`OrgProvider`, `key_to_action`, `viewport_top`, `App` state transitions) is pure
and tested.

## Verification

Automated (repo root):
```sh
cargo test                              # Document + View + structure/Org + tui pure units
cargo clippy --all-targets -- -D warnings
cargo build
cargo doc --no-deps                     # public items documented; builds clean
```

Manual:
```sh
cargo run -p textr-org-tui -- /tmp/notes.org   # existing file, or empty buffer if missing
cargo run -p textr-org-tui                      # untitled buffer
```
Acceptance checklist — base editor:
- File contents shown; cursor top-left; status `notes.org — 1:1`.
- Arrows / Home / End / PageUp / PageDown move correctly; Up/Down preserve goal column over
  short lines; cursor never leaves the buffer.
- Printable chars insert; Enter splits; Backspace deletes/joins; Delete removes forward/joins;
  after any edit the filename shows a trailing `*`.
- Cursor leaving the window scrolls the body to keep it visible; `line:col` updates live.
- `Ctrl+S` on a named buffer writes the file (verify by reopening) and clears `*`.
- `Ctrl+S` on an untitled buffer opens `Save as:`; typing a path + Enter writes it (Esc
  cancels); a write error shows on the status line instead of crashing.
- `Ctrl+Q` exits cleanly; the terminal is fully restored (prompt usable, echo on).
- A deliberate `panic!` during dev confirms the panic hook restores the terminal.

Acceptance checklist — Org structure (open a buffer with `*`/`**` headings):
- The outline is recognized; `Ctrl+N`/`Ctrl+P` jump between headings.
- `Tab` on a heading folds/unfolds its subtree; the fold marker reflects the state; `Tab`
  off a heading inserts a tab as usual.
- Folding hides exactly the heading's subtree lines and nothing else; the cursor lands on the
  right screen row afterward.
- `Ctrl+T` cycles a heading `none → TODO → DONE → none`, rewriting the line and setting `*`.
- After an edit that adds/removes lines, folds still cover the correct ranges (outline
  re-derived).
