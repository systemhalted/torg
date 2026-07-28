# Categorized help menu

Date: 2026-07-26 · Status: approved design, implemented
Reference behavior: none in Org; the design follows torg's own `Mode::BufferList` overlay
pattern.

## Goal

Replace the `Ctrl+H` quick-reference *doc buffer* with a navigable, categorized menu of
every command, generated from a single command table so help can never drift from the real
keymap again — and let `Enter` execute the selected command, which doubles as an escape
hatch for chords some terminals swallow (`Ctrl+Space`, `Alt+Shift+Enter`).

## Scope

In: the overlay menu on `Ctrl+H`/`Ctrl+K`, category navigation, command execution on
`Enter`, a drift-guard test tying the table to the `Action` enum, docs updates.
Out: fuzzy search/filter inside the menu (a natural follow-up once incremental search
ships), user-configurable keybindings, mouse support, changes to `Ctrl+U` (the full guide
stays a doc buffer).

## Behavior

- `Ctrl+H` (or the `Ctrl+K` fallback, unchanged pairing) opens the menu; the old
  `*Quick reference*` doc buffer is retired (`docs/usage.md` itself remains the shipped
  reference documentation).
- Layout: a centered overlay; category tabs across the top — **File · Edit · Navigate ·
  Structure · Dates · Search · Buffers · Help** — and the active category's commands
  listed below as `key  name — description`, one per line, with the selected row
  highlighted. A footer line shows `←/→ category · ↑/↓ select · Enter run · Esc close`.
  **Deviation, locked in:** the shipped menu has **seven** categories, not eight — File,
  Navigate, Structure, Dates, Search, Buffers, Help. Edit is dropped: the spec's own
  exclusion list (`InsertChar`, `Newline`, `Backspace`, `Delete`, plain cursor motion) leaves
  it with no commands to hold, so a category tab for it would render empty.
- `←`/`→` (and `Tab`) switch category, wrapping; `↑`/`↓` move the selection, wrapping;
  selection resets to the first row on category switch.
- `Enter` closes the menu and dispatches the selected command exactly as if its chord had
  been pressed in Edit mode (prompt-opening commands open their prompt; cursor-contextual
  commands act on the current cursor).
- `Esc`, `Ctrl+H`, `Ctrl+K` close the menu with no action.
- The menu never scrolls the buffer behind it; on tiny terminals the overlay clamps to the
  frame the way the buffer list does.
  **Deviation, locked in:** the menu isn't a floating overlay — it replaces the document body
  while `Mode::HelpMenu` is active, the same body-replacement pattern `Mode::BufferList` already
  uses (`draw_buffer_list`/`draw_help_menu` in `ui.rs`). torg has no floating-overlay machinery
  to draw on top of the buffer, so this reuses what already exists instead of building one.

## Design

### TUI: new module `crates/tui/src/commands.rs`

The single source of truth:

```rust
pub enum Category { File, Edit, Navigate, Structure, Dates, Search, Buffers, Help }

pub struct CommandInfo {
    pub action: Action,
    pub keys: &'static str,        // display form, e.g. "Alt+Shift+←"
    pub name: &'static str,        // short, e.g. "Promote subtree"
    pub description: &'static str, // one line
    pub category: Category,
}

pub static COMMANDS: &[CommandInfo] = &[ /* every user-facing command */ ];
```

Excluded from the table by design: `InsertChar`, plain cursor keys (`MoveLeft`…,
Home/End/Page), `Newline`, `Backspace`, `Delete` — the Navigate category documents
Page/heading motion but raw typing keys stay out of the menu.

**Drift guard:** a test walks every `Action` variant (a small exhaustive `match` that
fails to compile when a variant is added) and asserts it is either in `COMMANDS` or in the
explicit excluded list. Adding an action without deciding its help entry becomes a compile
or test failure. **Deviation, as-built, worth recording precisely:** the exhaustive
`requires_entry` match is what forces a decision on every *new* Action variant — that half of
the guard is automatic. But coverage of the *included* side (does `COMMANDS` actually have an
entry for every action `requires_entry` says it must?) is checked against a second, hand-
maintained list, `menu_actions()`, in `commands.rs`'s test module. `requires_entry` returning
`true` for an action doesn't by itself guarantee `COMMANDS` contains it; that guarantee only
holds for actions also listed in `menu_actions()`. Adding an action to `requires_entry`'s
`true` arm without also adding it to `menu_actions()` and to `COMMANDS` will not be caught.

### TUI wiring

- `app.rs`: `Mode::HelpMenu { category: usize, selected: usize }`; `Action::Help` opens it
  (replacing the `open_doc("*Quick reference*", …)` arm at `app.rs:478`); the mode handler
  implements the key table above; `Enter` leaves `Mode::Edit` first, then feeds the chosen
  action back through the normal dispatch path so behavior is identical to pressing the
  chord.
- `ui.rs`: a new overlay arm rendering tabs, rows, and footer from `COMMANDS`, reusing the
  buffer-list overlay's frame/clamping helpers and styles.
- `action.rs`: no new actions and no key changes (`Ctrl+H`/`Ctrl+K` already map to
  `Action::Help`).

### Docs

`usage.md`: describe the menu (and that `Enter` runs commands); `guide.md` help section
updated; README bullet. The keys tables in the docs stay — they are the offline copy of
the same information.

## Testing

- `commands.rs`: the drift-guard test; every table row's category is one of the eight;
  keys strings are non-empty and unique per category.
- `app.rs`: `Ctrl+H` opens the menu (no `*Quick reference*` buffer appears); tab/arrow
  navigation wraps in both axes; `Enter` on "Save As" opens the SaveAs prompt; `Enter` on
  a structural command edits the document exactly as the chord does; `Esc` closes with no
  change; the existing `ctrl('u')` guide test keeps passing untouched.
- End-to-end via the project verify skill (tmux): open the menu, arrow to the Structure
  category, assert the promote/demote rows render with their chords; `Esc`; assert the
  editor is unchanged.
- `cargo test --workspace` + `clippy -D warnings` green; core and `crates/ffi` untouched.

## Sub-project sequencing note

Built after incremental search so the Search category exists from day one. The parked
`m4-lists` chunk follows; its new chords (`Ctrl+Space`/`Alt+X`) will land in `COMMANDS`
as part of that chunk's plan.
