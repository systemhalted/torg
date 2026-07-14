# torg tutorial — Rust concepts, learned by building Milestone 2

Notes on the Rust ideas that showed up while building the M2 editor (the headless `View` and
`structure` layer in `crates/core`, and the terminal frontend in `crates/tui`). Each section
names the concept, shows where it lives in the code, and explains *why* it's written that way.
Read it alongside the source — every snippet is real.

---

## 1. Workspaces enforce architecture through the type system

textr is a Cargo **workspace**: several crates sharing one `Cargo.lock` and one dependency
list. The split isn't cosmetic — it's how the "core never depends on a terminal" rule is
*enforced by the compiler* rather than by discipline.

`crates/core/Cargo.toml` depends only on `ropey` + `thiserror`. `ratatui`/`crossterm` are
declared once in the root `[workspace.dependencies]` and pulled in **only** by
`crates/tui/Cargo.toml`:

```toml
# crates/tui/Cargo.toml
[dependencies]
torg-core = { path = "../core" }   # tui -> core
ratatui.workspace = true            # terminal stack, tui only
crossterm.workspace = true
```

Because `core` never lists the terminal crates, any accidental `use crossterm::…` inside it is
a compile error. The dependency arrow physically cannot point the wrong way.

**Takeaway:** crate boundaries are a design tool. If two things must never depend on each
other, put them in separate crates and the borrow-checker's bigger cousin — the module/crate
system — will hold the line for you.

---

## 2. Borrowing instead of owning: `View` does not hold the `Document`

A cursor needs to read the text to move sensibly, but if `View` *owned* the `Document`, the
`App` couldn't also hand it to the renderer. So every `View` method **borrows** the document:

```rust
// crates/core/src/view.rs
pub fn move_right(&mut self, doc: &Document) { … }          // reads geometry
pub fn insert_char(&mut self, doc: &mut Document, ch: char) // mutates the buffer
```

`&Document` is a shared (read-only) borrow; `&mut Document` is an exclusive one. The `App`
owns both the `View` and the `Document` and lends them out per call. This is the single most
important ownership decision in the codebase — it's what keeps the model layer composable.

### The split-borrow trick

Inside `App`, an edit needs `&mut self.view` **and** `&mut self.doc` at once. Rust allows
borrowing two *different fields* mutably, but a method taking `&mut self` hides that. The fix
is a small helper that borrows the fields directly and takes a closure:

```rust
// crates/tui/src/app.rs
fn edit(&mut self, f: impl FnOnce(&mut View, &mut Document)) {
    f(&mut self.view, &mut self.doc);   // two disjoint field borrows — allowed
    self.reparse();
}
// used as: self.edit(|v, d| v.insert_char(d, c));
```

`impl FnOnce(&mut View, &mut Document)` means "any closure I can call once with these two
arguments." See §7 for closures in general.

---

## 3. `usize` underflows panic — reach for saturating math and `.min()`

Positions are `usize` (unsigned). `0usize - 1` doesn't wrap to −1; in debug builds it
**panics**. Cursor code is full of "move back one, but not past zero," so it uses saturating
and clamping helpers instead of raw `-`:

```rust
// crates/core/src/view.rs
fn last_line(doc: &Document) -> usize {
    doc.line_count().saturating_sub(1)   // 0 - 1 saturates to 0, never panics
}

pub fn move_page_up(&mut self, doc: &Document, page: usize) {
    self.line = self.line.saturating_sub(page);                 // clamp at the top
    self.column = self.goal_column.min(doc.line_len_chars(self.line)); // clamp into the line
}
```

`a.saturating_sub(b)` floors at 0; `a.min(b)` caps a value. Together they express "stay in
bounds" without a single `if`.

**Takeaway:** on unsigned types, prefer `saturating_sub`/`checked_sub`/`min`/`max` over `-`
whenever a value could hit an edge.

---

## 4. Characters are not bytes — ropey is char-indexed, and so are we

`ropey` indexes by **character**, not byte. A `'é'` is two UTF-8 bytes but **one** character
and one screen column. Keeping every position in char units avoids an entire class of bugs:

```rust
// crates/core/src/view.rs
pub fn insert_char(&mut self, doc: &mut Document, ch: char) {
    let idx = self.cursor_char_idx(doc);   // a CHAR index
    let mut buf = [0u8; 4];                 // max UTF-8 length of one char
    doc.insert(idx, ch.encode_utf8(&mut buf)); // encode the char into a &str, insert it
    self.set_column(self.column + 1);       // advanced by ONE column
}
```

`char::encode_utf8` writes the char into a stack buffer (`[0u8; 4]`, the largest a UTF-8 char
can be) and returns a `&str` view of it — no heap allocation.

The Org parser leans on the fact that `*` and space are ASCII (one byte = one char), so it can
freely mix byte counting and char indexing *only* across that ASCII prefix — and comments say
so where it matters:

```rust
// crates/core/src/structure.rs — stars & spaces are ASCII, so byte len == char len here
let after_stars = &text[heading.level..];
let spaces = after_stars.len() - after_stars.trim_start().len();
let rest_start = doc.line_to_char(line) + heading.level + spaces;
```

**Takeaway:** decide your unit (char vs byte) once and hold it everywhere. Slicing a `&str`
uses byte offsets, so only slice at boundaries you know are ASCII or came from char APIs.

---

## 5. `Option`, `let … else`, and combinator chains

Rust has no `null`; "maybe absent" is `Option<T>`, and the compiler forces you to handle the
`None`. Three idioms from the code:

**Early return with `let … else`** — parse, or bail if it isn't a heading:

```rust
// crates/core/src/structure.rs
let Some(heading) = parse_org_heading(&raw, line) else {
    return; // not a heading — leave the buffer untouched
};
```

**The `?` operator** on `Option` — propagate `None` up one level:

```rust
// crates/core/src/structure.rs
pub fn parent_heading(outline: &Outline, line: usize) -> Option<usize> {
    let idx = outline.enclosing_index(line)?; // None here => function returns None
    …
}
```

**Combinator chains** — transform an optional value without unwrapping it:

```rust
// crates/tui/src/ui.rs
let name = app.document().path()          // Option<&Path>
    .and_then(|p| p.file_name())          // Option<&OsStr>
    .map(|s| s.to_string_lossy().into_owned())
    .unwrap_or_else(|| "[No Name]".to_string());
```

**Takeaway:** `let … else` for "handle the miss and leave," `?` for "pass the miss up,"
combinators for "keep going if present."

---

## 6. Enums with data + exhaustive `match` model state precisely

An editor mode is either normal editing or a Save-As prompt *with the text typed so far*. That
"with" is why `Mode` is an enum carrying data, not a bool:

```rust
// crates/tui/src/app.rs
pub enum Mode {
    Edit,
    SaveAs { input: String },   // the prompt owns its buffer
}
```

`match` must cover every variant, so adding a mode later forces you to handle it everywhere —
the compiler makes the omission impossible to forget. Binding into a variant while mutating
needs a borrow of the right kind:

```rust
if let Mode::SaveAs { input } = &mut self.mode {
    input.push(c);   // &mut into the variant's field
}
```

The `Action` enum (§ `action.rs`) and `TodoState { Todo, Done }` are the same idea: make
illegal states unrepresentable, then `match` on them.

---

## 7. Closures and higher-order functions

A closure is an anonymous function that can capture its environment. We pass one to `edit`
(§2), and the panic hook captures a value and moves it into a `'static` closure:

```rust
// crates/tui/src/terminal.rs
pub fn install_panic_hook() {
    let original = std::panic::take_hook();          // captured by the closure below
    std::panic::set_hook(Box::new(move |info| {      // `move` => closure owns `original`
        let _ = restore();
        original(info);
    }));
}
```

`Box::new(move |info| …)` is a heap-allocated closure that **owns** `original` (via `move`),
so it stays valid for the whole program. The three closure traits — `FnOnce` (callable once,
may consume captures), `FnMut` (callable repeatedly, may mutate), `Fn` (callable repeatedly,
read-only) — describe how a closure uses its captures; `edit` asks for the most permissive one
it can, `FnOnce`.

---

## 8. Traits are the format-agnostic seam

The whole "works for any format" promise rests on one trait:

```rust
// crates/core/src/structure.rs
pub trait StructureProvider {
    fn parse(&self, doc: &Document) -> Outline;
    fn cycle_todo(&self, doc: &mut Document, line: usize);
}

pub struct OrgProvider;                       // a zero-sized type: no fields, no runtime cost
impl StructureProvider for OrgProvider { … }
```

`OrgProvider` holds no data — it's a **zero-sized type**, existing only to attach behaviour to
the trait. Everything above structure calls `parse`/`cycle_todo`; none of it names Org. When
the Markdown provider arrives, it's a second `impl` and the callers don't change. Today the
`App` uses the concrete `OrgProvider` directly; to pick a provider at runtime later you'd store
a `Box<dyn StructureProvider>` (a trait object) instead — same trait, dynamic dispatch.

---

## 9. Iterators express "search / filter / transform" without loops

The outline is built and queried almost entirely with iterator adapters:

```rust
// crates/core/src/structure.rs
let mut headings: Vec<Heading> = (0..doc.line_count())
    .filter_map(|line| parse_org_heading(&doc.line_text(line), line)) // keep the Some(_)s
    .collect();                                                       // gather into a Vec
```

`filter_map` maps and drops `None`s in one pass. Navigation is just searches over the vector:

```rust
pub fn next_heading(outline: &Outline, line: usize) -> Option<usize> {
    outline.headings.iter().find(|h| h.line > line).map(|h| h.line)  // first below
}
// prev_heading uses .iter().rev().find(…);  parent uses .rposition(…) and slicing [..idx]
```

`find` returns the first match as an `Option`; `rev()` searches from the end; `rposition`
returns the last matching index. No manual indexing, no off-by-one.

---

## 10. `derive` macros generate the boring code

`#[derive(...)]` writes trait impls for you:

```rust
// crates/core/src/view.rs
#[derive(Debug, Clone, Default)]
pub struct View { line: usize, column: usize, goal_column: usize }

pub fn new() -> Self { Self::default() }  // Default derived => all fields 0 => cursor at (0,0)
```

`Default` gives every field its zero value, so `View::new` is a one-liner. `Debug` enables
`{:?}` printing (used in test failure messages). `Clone` allows copies. On `Heading` we also
derive `PartialEq, Eq` so tests can assert `heading == expected` and compare whole `Vec`s.

---

## 11. Errors are values: `Result`, `thiserror`, `?`, and `ExitCode`

The core surfaces typed errors (`DocumentError`) built with `thiserror`; the frontend turns
them into messages. Nothing panics on an expected failure like a bad path.

```rust
// crates/tui/src/main.rs
let doc = Document::open(&path)
    .map_err(|e| format!("cannot open {}: {e}", path.display()))?; // Result<_, DocumentError> -> Result<_, String>
```

`?` returns the `Err` early; `map_err` rewrites the error type on the way out. `main` returns
`std::process::ExitCode`, so a handled failure becomes a clean non-zero exit, not a panic:

```rust
fn main() -> ExitCode {
    let (doc, stash_path) = match load() {
        Ok(pair) => pair,
        Err(msg) => { eprintln!("torg: {msg}"); return ExitCode::FAILURE; }
    };
    …
}
```

A save error follows the same path all the way to the status line (`"Save failed: {e}"`) — the
UI reports it instead of crashing.

---

## 12. Panic safety and terminal restoration (a poor-man's RAII)

A raw-mode alternate-screen terminal *must* be restored on exit, including on panic, or the
user's shell is left broken. Two mechanisms guarantee it:

1. **Unconditional teardown on the normal path** — `main` calls `terminal::restore()` after the
   event loop *regardless of whether it returned `Ok` or `Err`*:
   ```rust
   let result = terminal::run(&mut term, &mut app);
   let _ = terminal::restore();          // always, even if the loop errored
   ```
2. **A panic hook that restores first** — installed before entering raw mode:
   ```rust
   std::panic::set_hook(Box::new(move |info| {
       let _ = restore();                // fix the terminal…
       original(info);                   // …then let the default hook print the panic
   }));
   ```

This is the manual version of **RAII** (Resource Acquisition Is Initialization): in idiomatic
Rust a guard type would implement `Drop` to restore in its destructor. We use explicit
teardown + a panic hook here because the terminal is a process-global resource and the panic
path needs special handling. Either way, the principle is the same: *tie cleanup to an event
that always happens.*

---

## 13. Tests live next to the code and drive pure logic directly

Every module ends in `#[cfg(test)] mod tests { … }` — compiled only for `cargo test`. Because
the state tier is terminal-free, the `App` tests drive real key presses as **data**, with no
terminal at all:

```rust
// crates/tui/src/app.rs (tests)
fn ctrl(app: &mut App, c: char) {
    app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

#[test]
fn ctrl_t_cycles_the_heading_todo_keyword() {
    let mut app = App::new(Document::from_text("* task\n"), None);
    ctrl(&mut app, 't');
    assert_eq!(app.document().text(), "* TODO task\n");
}
```

A `KeyEvent` is just a struct, so the entire editor behaviour — modes, folding, saving — is
testable in-process. The genuinely untestable glue (raw mode, the blocking `event::read`, the
`draw` call) is quarantined in `terminal.rs`; everything with a branch is tested. That
separation (see `architecture.md`) is what makes ~70 fast unit tests possible.

---

## Where to go next

- The full M2 design and TDD breakdown: [`milestone-2-tui.md`](milestone-2-tui.md).
- The architecture and its two separations: [`architecture.md`](architecture.md).
- Where this is all heading (Org agenda, babel, export): [`roadmap.md`](roadmap.md).
