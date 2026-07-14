# textr-org

An Org-mode–flavored terminal text editor in Rust — the structure-editing **sibling** of
[`textr`](../textr), a from-scratch [gedit](https://gedit-text-editor.org/) clone. Where
`textr` stays a minimal gedit-style editor, `textr-org` treats Org-mode–class structure
editing (outline, folding, `TODO`/`DONE` cycling) as a first-class part of the editor.

It keeps gedit's architecture — a headless, UI-agnostic core with thin frontends — and shares
`textr`'s rope-buffer core. It began as an Ultraplan-generated implementation of `textr`'s
Milestone 2 and was split into its own project so that work could live and run independently.
The binary is `torg`. (In `textr` proper, this Org functionality is instead planned to return
later as an installable *package* over a stable core API.)

## Status

**Runnable.** `textr-org` is a terminal editor that opens, edits, and saves multiple buffers —
several files in one session, switched without quitting — and it understands both Org `*`
headings and Markdown `#` headings: folding, navigation, `TODO`/`DONE` cycling, structural
editing (promote/demote, move, priorities, tags), and Org timestamps with scheduling. This is
Milestones 2 and 3 complete, plus the multi-file machinery from M5 and the first slice of M4
(dates); the longer arc toward full Org (agenda, source blocks, export) is in
[`docs/roadmap.md`](docs/roadmap.md).

```sh
cargo run -p textr-org-tui -- notes.org
```

## Install

Prebuilt binaries ship for macOS (Apple Silicon + Intel) and Linux (x86-64 + ARM64) on every
[release](https://github.com/systemhalted/textr-org/releases):

```sh
brew install systemhalted/tap/torg                 # macOS (Homebrew)
sudo apt install ./torg-x86_64-unknown-linux-gnu.deb  # Debian/Ubuntu (.deb from the release)
cargo install --path crates/tui                    # from source (Rust 1.96+)
```

Full instructions — direct downloads, ARM64, and the one-time macOS Gatekeeper step for
unsigned binaries — are in [`docs/install.md`](docs/install.md). Cutting a release is
[`docs/releasing.md`](docs/releasing.md).

## What works today

**The terminal editor (`textr-org-tui`)** — see [`docs/usage.md`](docs/usage.md) for the full key
list:

- open a file (or start untitled / create-on-save), move with arrows / Home / End / PageUp /
  PageDown (Up/Down keep a goal column), insert, split, backspace/delete with line joining
- **multiple files in one session** — `torg a.org b.org` or `Ctrl+O` to open, `Alt+N`/`Alt+P`
  to cycle buffers, `Ctrl+B` for a buffer list, `Ctrl+W` to close one; each buffer keeps its
  own cursor, folds, and scroll, and closing or quitting past unsaved changes asks first
- save with `Ctrl+S`; an untitled buffer prompts for a path (*Save As*); write errors show on
  the status line instead of crashing
- **structure, per format**: `Tab` folds/unfolds a heading's subtree, `Ctrl+N`/`Ctrl+P` jump
  between headings, `Ctrl+T` cycles a heading none → `TODO` → `DONE` → none — for Org `*`
  headings and, in `.md` files, Markdown `#` headings (fenced code blocks are ignored)
- **structural editing**: promote/demote a heading or whole subtree (`Alt+←/→`, `+Shift`),
  move subtrees among siblings (`Alt+↑/↓`), insert sibling headings (`Alt+Enter`), cycle
  `[#A]`/`[#B]`/`[#C]` priorities (`Shift+↑/↓`), and edit `:tags:` (`Ctrl+G`) — same
  operations in both formats, written once against the structure trait
- **dates**: Org timestamps parse as data — active/inactive, times, ranges, repeaters; set a
  heading's `SCHEDULED`/`DEADLINE` (`Alt+S`/`Alt+D`), insert a timestamp (`Alt+.`/`Alt+i`),
  and shift the field under the cursor with `Shift+↑/↓`; timestamps are highlighted

**The headless core (`textr-org-core`)** — a `Document` on a [`ropey`](https://crates.io/crates/ropey)
rope (load/save/*Save As* with typed errors, char-indexed edits, a modified flag), a `View`
(cursor + editing, goal column), a format-agnostic `structure` layer (outline, fold
extents, TODO cycling, structural edits) behind a `StructureProvider` trait with Org and
Markdown implementations selected per file by `detect_format`, and a `timestamp` module
parsing the full Org timestamp grammar (no external date crate).

Everything with a branch is unit-tested (~200 tests); the terminal glue is the only untested
surface. New to the codebase? [`docs/tutorial.md`](docs/tutorial.md) walks through the Rust
concepts it uses.

## Architecture

The project mirrors gedit's model/UI separation. The core knows nothing about how it is
displayed; frontends render it through a shared interface. Full detail — including the second
separation *inside* the frontend (rendering vs the raw terminal driver) — is in
[`docs/architecture.md`](docs/architecture.md).

```
textr-org/
├── crates/
│   ├── core/        # UI-agnostic heart: Document, View, structure (Org + Markdown).
│   │                #   Tab, Window, commands planned. gedit's model layer, minus GTK.
│   ├── tui/         # terminal frontend (ratatui + crossterm) — the `torg` binary
│   └── gui/         # planned — desktop frontend (gtk4-rs), reusing the same core
└── docs/            # roadmap, architecture, per-milestone design, usage, tutorial
```

## Build, test, run

Requires a recent stable Rust toolchain (1.96+).

```sh
cargo build
cargo test                                    # ~200 unit tests
cargo clippy --all-targets -- -D warnings
cargo run -p textr-org-tui -- notes.org           # launch the editor
```

## Roadmap

Built in small, independently runnable milestones, each ending in a working program. Full
detail — including the north star of Org-mode–class structure editing for any format — is in
[`docs/roadmap.md`](docs/roadmap.md).

1. **Core document model** — rope buffer, open/save, edits — *done*
2. **TUI + Org outline core** — open/edit/save; folding, heading nav, TODO cycling; multiple
   buffers with switching — *done*
3. **Markdown provider + structural editing** — 2nd provider; promote/demote, move
   subtrees, priorities, tags — *done*
4. **Rich content** — timestamps *(done)*; tables, lists/checkboxes, links, markup, drawers
5. **Agenda** — multi-file views, sparse trees, custom keywords, dependencies
6. **Organize** — capture, refile, archive
7. **Time** — clocking, clock tables, repeaters, effort estimates
8. **Babel** — sandboxed source-block execution, tangle
9. **Export & publish** — HTML / LaTeX / Markdown / iCalendar, publishing projects
10. **Extensibility & advanced views** — custom links, column view, package API
11. **GUI** — a desktop frontend reusing the *same* core

The full map from [Org mode's feature set](https://orgmode.org/features.html) to these
milestones is the coverage table in [`docs/roadmap.md`](docs/roadmap.md).

## License

GPL-2.0-or-later, the same family as gedit, on which this is modeled.
