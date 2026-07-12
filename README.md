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
several files in one session, switched without quitting — and it already understands Org `*`
headings: folding, heading navigation, and `TODO`/`DONE` cycling. This is Milestone 2 plus the
first cut of the multi-file machinery the roadmap points at M5; the longer arc toward full Org
(agenda, source blocks, export) is in [`docs/roadmap.md`](docs/roadmap.md).

```sh
cargo run -p textr-org-tui -- notes.org
```

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
- **Org structure**: `Tab` folds/unfolds a heading's subtree, `Ctrl+N`/`Ctrl+P` jump between
  headings, `Ctrl+T` cycles a heading none → `TODO` → `DONE` → none

**The headless core (`textr-org-core`)** — a `Document` on a [`ropey`](https://crates.io/crates/ropey)
rope (load/save/*Save As* with typed errors, char-indexed edits, a modified flag), a `View`
(cursor + editing, goal column), and a format-agnostic `structure` layer (outline, fold
extents, TODO cycling) behind a `StructureProvider` trait with an Org implementation.

Everything with a branch is unit-tested (~100 tests); the terminal glue is the only untested
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
│   ├── core/        # UI-agnostic heart: Document, View, structure (Org provider).
│   │                #   Tab, Window, commands planned. gedit's model layer, minus GTK.
│   ├── tui/         # terminal frontend (ratatui + crossterm) — the `torg` binary
│   └── gui/         # planned — desktop frontend (gtk4-rs), reusing the same core
└── docs/            # roadmap, architecture, per-milestone design, usage, tutorial
```

## Build, test, run

Requires a recent stable Rust toolchain (1.96+).

```sh
cargo build
cargo test                                    # ~100 unit tests
cargo clippy --all-targets -- -D warnings
cargo run -p textr-org-tui -- notes.org           # launch the editor
```

## Roadmap

Built in small, independently runnable milestones, each ending in a working program. Full
detail — including the north star of Org-mode–class structure editing for any format — is in
[`docs/roadmap.md`](docs/roadmap.md).

1. **Core document model** — rope buffer, open/save, edits — *done*
2. **TUI + Org outline core** — open/edit/save; folding, heading nav, TODO cycling; multiple
   buffers with switching — *done (current)*
3. **Markdown provider + structural editing** — 2nd provider; promote/demote, move subtrees
4. **Rich content** — tables, lists, links, timestamps
5. **Agenda** — multi-file collection, a date model
6. **Babel** — sandboxed source-block execution
7. **Export** — HTML / Markdown / LaTeX from the structure model
8. **GUI** — a desktop frontend reusing the *same* core

## License

GPL-2.0-or-later, the same family as gedit, on which this is modeled.
