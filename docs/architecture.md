# torg architecture

> The developer map: how the pieces fit and, more importantly, **what is allowed to depend on
> what**. For *why* textr is built this way and where it is going, see
> [`roadmap.md`](roadmap.md); for the current milestone's design detail, see
> [`milestone-2-tui.md`](milestone-2-tui.md).
>
> Status note: `crates/core::document` exists and is tested (M1). `crates/core::{view,
> structure}` and the whole `crates/tui` crate are being built in M2 — this document is the
> map they are being built *to*. Items not yet on disk are marked *(M2)*.

## The one rule

textr mirrors gedit's model/UI split: a headless core that knows nothing about how it is
displayed, with thin frontends on top. That gives **one hard, non-negotiable rule**:

> **`tui → core`, never the reverse.** `crates/core` must compile and be fully unit-testable
> with **no** terminal or windowing dependency. It never sees `ratatui` or `crossterm`.

This is enforced by the manifests: `crates/core/Cargo.toml` depends only on `ropey` +
`thiserror`; the terminal crates live only in `crates/tui/Cargo.toml`.

## Two separations, not one

The crate boundary is the first separation. Inside the TUI there is a **second** one, just as
deliberate: the rendering and app state know nothing about the *raw terminal driver*. All the
`crossterm` / raw-mode / alt-screen / `event::read` / panic-hook code is quarantined in one
place, so a future GUI reuses the state and render logic without inheriting the terminal.

```
        ┌─────────────────────────────────────────────────────────┐
        │ crates/tui  (M2)                                         │
        │  ┌───────────────────────────────────────────────────┐  │
        │  │ terminal.rs + main.rs  ← ONLY crossterm lives here │  │  driver
        │  └──────────────────────────┬────────────────────────┘  │
        │  ┌──────────────────────────▼────────────────────────┐  │
        │  │ ui.rs  (ratatui Frame render; no crossterm)        │  │  UI
        │  └──────────────────────────┬────────────────────────┘  │
        │  ┌──────────────────────────▼────────────────────────┐  │
        │  │ app.rs · buffer.rs · action.rs · viewport.rs       │  │  state
        │  │ (pure, tested)                                     │  │
        │  └──────────────────────────┬────────────────────────┘  │
        └─────────────────────────────┼───────────────────────────┘
                         depends on ▼ (never the reverse)
        ┌─────────────────────────────────────────────────────────┐
        │ crates/core  (headless — no terminal, no ratatui)        │
        │  document::Document (rope I/O)   view::View (cursor+edit) │
        │  structure::{Outline, Heading, TodoState}                │
        │  timestamp::{Timestamp, Stamp, …}  (Org date grammar)    │
        │  StructureProvider ──impl── OrgProvider · MarkdownProvider │
        │                     (Format + detect_format pick one)     │
        └─────────────────────────────────────────────────────────┘
```

### The three TUI tiers

| Tier | Files | Knows about | Tested? |
|------|-------|-------------|---------|
| **State** | `app.rs`, `buffer.rs`, `action.rs`, `viewport.rs` *(M2)* | neither ratatui nor crossterm | yes — pure unit tests |
| **UI / render** | `ui.rs` *(M2)* | ratatui `Frame` only; no I/O, no `event::read` | via manual checklist |
| **Terminal driver** | `terminal.rs`, `main.rs` *(M2)* | crossterm: raw mode, alt screen, event loop, panic hook | no — the single untested surface |

Dependencies point strictly inward (driver → UI → state → core). The genuinely untestable
code — terminal enter/leave, the blocking `event::read()`, the `terminal.draw()` call, the
panic hook — is confined to the driver tier and kept to a few dozen lines of glue. Everything
with branching (cursor logic, key mapping, scroll math, mode transitions, outline parsing) is
pure and tested.

The `KeyEvent → Action` seam runs *across* the driver boundary but stays in the state tier:
`key_to_action` maps the `crossterm::KeyEvent` type yet performs no terminal I/O, so swapping
the driver (e.g. for the GUI) changes only how `KeyEvent`s are *produced*, not the mapping.

## `crates/core` — the headless heart

- **`document::Document`** — the text model (gedit's `GeditDocument`). A `ropey` rope plus a
  modified flag and the associated file path; open / save / *Save As* with typed errors; edits
  and read-only line/char accessors so nothing above it touches the rope directly. All edit
  positions are **char** indices, never bytes. *(exists — M1)*
- **`view::View`** *(M2)* — the headless cursor + editing-intent model (gedit's `GeditView` /
  insert-mark over the buffer). Holds `(line, column)` and a `goal_column`; exposes movement
  (arrows / home / end / paging) and editing intent (insert / newline / backspace / delete).
  It does **not** own the `Document` — methods take `&Document` / `&mut Document` (the `App`
  owns both). The flat char index for `Document::insert`/`remove` is derived on demand
  (`line_to_char(line) + column`), never stored — single source of truth.
- **`structure`** *(M2)* — the format-agnostic outline layer (see below).

## The structure layer — one model, many formats

This is the spine of the [north star](roadmap.md). A buffer parses into a flat, level-tagged
outline; a heading's subtree extent gives its fold range. **Format knowledge lives behind a
trait**, so every capability built on top — folding now; promote/demote, agenda, and export
later — is written once and works for every format.

```rust
pub struct Outline { pub headings: Vec<Heading> }

pub struct Heading {
    pub level: usize,            // 1-based: Org "* "=1, "** "=2  (Markdown "# "=1)
    pub line: usize,             // line index the heading sits on
    pub title: String,           // heading text, keyword/priority/markers/tags stripped
    pub todo: Option<TodoState>, // parsed leading TODO keyword, if any
    pub priority: Option<char>,  // [#A]..[#C]
    pub tags: Vec<String>,       // trailing :tag: run
    pub scheduled: Option<Timestamp>, // planning line below the heading (Org only)
    pub deadline: Option<Timestamp>,
    pub last_line: usize,        // last line of this heading's subtree → fold range line+1..=last_line
}

pub enum TodoState { Todo, Done } // custom keyword sets → M5

pub trait StructureProvider {
    fn parse(&self, doc: &Document) -> Outline;
    fn cycle_todo(&self, doc: &mut Document, line: usize); // None → TODO → DONE → None
    fn marker(&self) -> u8;                // '*' / '#' — the format primitives that let…
    fn max_level(&self) -> Option<usize>;  // None / Some(6)
    // …the structural-edit operations be DEFAULT METHODS, written once for every format:
    // promote/demote_heading, promote/demote_subtree, move_subtree_up/down,
    // insert_sibling, cycle_priority, set_tags, set_planning — all returning an EditOutcome
    // (Changed { cursor_line } | NoOp(reason)).
}

// The date layer (M4) lives in a sibling module and is format-agnostic. Timestamps are
// parsed as data; `shift_timestamp` rewrites the field under the cursor; planning
// (SCHEDULED:/DEADLINE:) is Org-only.
pub fn shift_timestamp(doc: &mut Document, line: usize, col: usize, up: bool) -> Option<EditOutcome>;

pub struct OrgProvider;      // recognizes ^(\*+) +(?:(TODO|DONE) +)?(rest)$
pub struct MarkdownProvider; // recognizes ^(#{1,6}) +(?:(TODO|DONE) +)?(rest)$, skipping
                             // fenced code blocks (```/~~~)

pub enum Format { Org, Markdown }               // impl StructureProvider by delegation
pub fn detect_format(path: Option<&Path>) -> Format; // .md/.markdown → Markdown, else Org
```

- **Navigation** (`next_heading` / `prev_heading` / `parent_heading`) is pure free functions
  over `&Outline` + a cursor line — no `Document` mutation.
- **`cycle_todo`** is the one structural *edit* in M2; the provider rewrites the heading line
  via `Document::remove`/`insert` on char indices from `line_to_char`.
- `MarkdownProvider` (M3) is the second implementer — the test that the trait is genuinely
  format-agnostic, which folding, navigation, and TODO cycling passed without change. Its
  parse is stateful (headings inside fenced code blocks are skipped), so its `cycle_todo`
  checks the full parse rather than one line. Agenda (M5) and export (M9) consume this same
  model, so they too are written once against the trait rather than per-format.
- **Which format a buffer uses is per-buffer TUI state** (`Buffer.format`), chosen by
  `detect_format` from the file extension at buffer creation and re-detected on *Save As*.
  The detection rule itself (".md means Markdown") is format knowledge and lives in core.

## Where folding lives — and why not in core

Folding state (which headings are collapsed) is **presentation state**, like scroll position,
so it lives in the TUI (`Buffer.folded: HashSet<usize>` of heading start lines), **not** in
core. The same reasoning places the whole multi-file machinery in the TUI state tier: a
`Buffer` (`buffer.rs`) groups one `Document` with its `View`, folds, outline cache, and scroll
position, and the `App` holds a `Vec<Buffer>` plus the active index — core stays a
single-`Document` model with no notion of which files an editor happens to have open.
The *outline it folds against* is derived from the buffer by the (headless) structure layer and
re-derived after edits. Same reasoning as scroll: `viewport_top` is a pure TUI function because
it depends on terminal height, which core must never know about. The rule holds — anything that
depends on how the buffer is *displayed* stays above the core boundary; anything intrinsic to
the *text and its structure* stays in core.

## Generalizing to the GUI (M11)

The desktop frontend reuses `crates/core` unchanged **and** the driver-agnostic state + render
tiers; only the terminal driver is replaced by a windowing driver. That the same `Outline`,
`View`, and `Action` types serve both frontends is the payoff of keeping both separations
strict from M2 onward.
