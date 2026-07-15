//! The editor state and all of its transitions — pure, terminal-free, and unit-tested.
//!
//! `App` owns the open [`Buffer`]s (each a [`Document`] + [`View`] + presentation state) and
//! drives the active one in response to key presses. It knows nothing about ratatui or
//! crossterm beyond the `KeyEvent` *data* type, so every transition below is exercised
//! in-process without a real terminal.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use torg_core::document::Document;
use torg_core::structure::{
    detect_format, is_valid_tag, next_heading, prev_heading, shift_timestamp, EditOutcome, Format,
    Outline, Planning, StructureProvider,
};
use torg_core::timestamp::{parse_timestamp, Timestamp};
use torg_core::view::View;

use crate::action::{key_to_action, Action};
use crate::buffer::Buffer;
use crate::viewport::viewport_top;

/// What the editor is doing right now. In the prompt modes the keyboard drives the
/// bottom-line path prompt instead of the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Normal editing.
    Edit,
    /// The *Save As* prompt is open; `input` is the path typed so far.
    SaveAs { input: String },
    /// The *Open* prompt is open; `input` is the path typed so far.
    OpenFile { input: String },
    /// The *Tags* prompt is open; `input` is the space-separated tags typed so far.
    EditTags { input: String },
    /// The buffer list is open; `selected` is the highlighted entry.
    BufferList { selected: usize },
    /// Asking whether to close the active (unsaved) buffer: `y` discards, `n`/Esc cancels.
    ConfirmClose,
    /// Asking whether to quit despite unsaved buffers: `y` quits, `n`/Esc cancels.
    ConfirmQuit,
    /// A date prompt is open; `input` is the timestamp typed so far and `purpose` is what to
    /// do with it on submit.
    DatePrompt { input: String, purpose: DatePurpose },
}

/// What a [`Mode::DatePrompt`] does with the date the user types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePurpose {
    Scheduled,
    Deadline,
    InsertActive,
    InsertInactive,
}

/// What a key press did to a bottom-line prompt.
enum PromptEvent {
    /// The input changed (or the key was ignored) — stay in the prompt.
    Pending,
    /// Esc — leave the prompt without acting.
    Cancelled,
    /// Enter — act on the typed text.
    Submitted(String),
}

/// The longest common prefix of `names`.
fn common_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = names.next() else {
        return String::new();
    };
    let mut prefix = first.to_string();
    for n in names {
        while !n.starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}

/// Complete the final segment of a path typed into a prompt against the filesystem. Returns
/// the (possibly unchanged) new input and an optional status message. A leading `~`/`$VAR` in
/// the directory portion is expanded for the lookup but preserved verbatim in the result.
fn complete_path(input: &str) -> (String, Option<String>) {
    let (prefix, partial) = match input.rfind('/') {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => ("", input),
    };
    let dir = if prefix.is_empty() {
        PathBuf::from(".")
    } else {
        expand_path(prefix)
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return (input.to_string(), Some(format!("cannot read {}", dir.display())));
    };
    let mut names: Vec<(String, bool)> = read
        .filter_map(Result::ok)
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            (e.file_name().to_string_lossy().into_owned(), is_dir)
        })
        .filter(|(name, _)| name.starts_with(partial))
        .collect();
    if !partial.starts_with('.') {
        names.retain(|(n, _)| !n.starts_with('.')); // hide dotfiles unless asked for
    }
    names.sort();
    match names.as_slice() {
        [] => (input.to_string(), Some("(no match)".into())),
        [(name, is_dir)] => {
            let mut done = format!("{prefix}{name}");
            if *is_dir {
                done.push('/');
            }
            (done, None)
        }
        many => {
            let common = common_prefix(many.iter().map(|(n, _)| n.as_str()));
            if common.len() > partial.len() {
                (format!("{prefix}{common}"), None)
            } else {
                let mut shown: Vec<String> = many
                    .iter()
                    .take(15)
                    .map(|(n, d)| if *d { format!("{n}/") } else { n.clone() })
                    .collect();
                if many.len() > 15 {
                    shown.push(format!("(+{} more)", many.len() - 15));
                }
                (
                    input.to_string(),
                    Some(format!("{} matches: {}", many.len(), shown.join("  "))),
                )
            }
        }
    }
}

/// The user's home directory, from `$HOME` (or `%USERPROFILE%` on Windows).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Expand a path typed into a prompt: a leading `~`/`~/` becomes the home directory and
/// `$VAR` / `${VAR}` become their environment values (an undefined variable is left verbatim,
/// so a real `$` in a name is never silently dropped). The shell isn't involved, so this is
/// deliberately limited to the two expansions a user actually expects.
fn expand_path(text: &str) -> PathBuf {
    let expanded = expand_env(text.trim());
    if expanded == "~" {
        if let Some(home) = home_dir() {
            return home;
        }
    } else if let Some(rest) = expanded.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(expanded)
}

/// Replace `$VAR` and `${VAR}` with their environment values, leaving unknown variables (and
/// stray `$` characters) untouched.
fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let braced = matches!(chars.peek(), Some((_, '{')));
        if braced {
            chars.next(); // consume '{'
        }
        let mut name = String::new();
        while let Some(&(_, nc)) = chars.peek() {
            let ok = if braced { nc != '}' } else { nc == '_' || nc.is_ascii_alphanumeric() };
            if ok {
                name.push(nc);
                chars.next();
            } else {
                break;
            }
        }
        let closed = if braced {
            matches!(chars.peek(), Some((_, '}'))).then(|| chars.next()).is_some()
        } else {
            true
        };
        match std::env::var(&name) {
            Ok(val) if !name.is_empty() && closed => out.push_str(&val),
            _ => {
                // Not a resolvable variable — put the literal text back.
                out.push('$');
                if braced {
                    out.push('{');
                }
                out.push_str(&name);
                if braced && closed {
                    out.push('}');
                }
            }
        }
    }
    out
}

/// Parse a date typed into the prompt (with or without brackets) into a timestamp whose
/// active/inactive flag matches `purpose`. `None` if it isn't a valid, fully-consumed stamp.
fn parse_date_input(text: &str, purpose: DatePurpose) -> Option<Timestamp> {
    let active = !matches!(purpose, DatePurpose::InsertInactive);
    let wrapped = if text.starts_with(['<', '[']) {
        text.to_string()
    } else if active {
        format!("<{text}>")
    } else {
        format!("[{text}]")
    };
    let (ts, len) = parse_timestamp(&wrapped)?;
    (len == wrapped.len()).then_some(ts)
}

/// Drive a prompt's input with one key press. Ctrl/Alt-modified characters are ignored so
/// command chords never leak literal characters into a typed path.
fn prompt_event(input: &mut String, key: KeyEvent) -> PromptEvent {
    if key.kind != KeyEventKind::Press {
        return PromptEvent::Pending;
    }
    match key.code {
        KeyCode::Esc => PromptEvent::Cancelled,
        KeyCode::Enter => PromptEvent::Submitted(input.clone()),
        KeyCode::Backspace => {
            input.pop();
            PromptEvent::Pending
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            input.push(c);
            PromptEvent::Pending
        }
        _ => PromptEvent::Pending,
    }
}

/// The whole editor: the open buffers, which one is active, and the app-global mode/status.
pub struct App {
    /// The open files. Invariant: never empty.
    buffers: Vec<Buffer>,
    /// Index of the buffer being edited. Invariant: `< buffers.len()`.
    active: usize,
    mode: Mode,
    /// Lines per page, kept in sync with the terminal body height for PageUp/PageDown.
    page: usize,
    /// A transient status-line message (save result, error), cleared on the next key.
    status: String,
    should_quit: bool,
}

impl App {
    /// Build an editor over `buffers`; the first is active. An empty vec gets one untitled
    /// buffer, keeping the never-empty invariant.
    pub fn new(mut buffers: Vec<Buffer>) -> Self {
        if buffers.is_empty() {
            buffers.push(Buffer::untitled());
        }
        Self {
            buffers,
            active: 0,
            mode: Mode::Edit,
            page: 1,
            status: String::new(),
            should_quit: false,
        }
    }

    // ---- the active buffer -------------------------------------------------

    fn buf(&self) -> &Buffer {
        &self.buffers[self.active]
    }
    fn buf_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active]
    }

    // ---- read-only accessors for the renderer -----------------------------

    pub fn document(&self) -> &Document {
        &self.buf().doc
    }
    pub fn view(&self) -> &View {
        &self.buf().view
    }
    pub fn mode(&self) -> &Mode {
        &self.mode
    }
    pub fn outline(&self) -> &Outline {
        &self.buf().outline
    }
    pub fn scroll_top(&self) -> usize {
        self.buf().scroll_top
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
    /// Index of the active buffer (0-based).
    pub fn active_index(&self) -> usize {
        self.active
    }
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }
    /// `(display_name, is_dirty)` per open buffer, for the status line and buffer list.
    pub fn buffer_labels(&self) -> Vec<(String, bool)> {
        self.buffers
            .iter()
            .map(|b| (b.display_name(), b.is_dirty()))
            .collect()
    }

    /// Whether `line` is a collapsed heading (draw a fold marker on it).
    pub fn is_folded_heading(&self, line: usize) -> bool {
        self.buf().folded.contains(&line)
    }

    /// Whether `line` is hidden inside some collapsed heading's subtree (skip it when drawing).
    pub fn is_hidden(&self, line: usize) -> bool {
        let b = self.buf();
        b.outline
            .headings
            .iter()
            .any(|h| b.folded.contains(&h.line) && line > h.line && line <= h.last_line)
    }

    // ---- driver seam ------------------------------------------------------

    /// Keep the page size in step with the terminal body height (for PageUp/PageDown).
    pub fn set_page(&mut self, page: usize) {
        self.page = page.max(1);
    }

    /// Recompute the viewport top so the cursor stays visible in a `body_height`-row body.
    pub fn update_scroll(&mut self, body_height: usize) {
        let b = self.buf_mut();
        b.scroll_top = viewport_top(b.view.cursor_line(), b.scroll_top, body_height);
    }

    /// Handle one key press, dispatching by mode.
    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Edit => {
                self.status.clear();
                if let Some(action) = key_to_action(key) {
                    self.apply(action);
                }
            }
            Mode::SaveAs { .. }
            | Mode::OpenFile { .. }
            | Mode::EditTags { .. }
            | Mode::DatePrompt { .. } => self.handle_prompt_key(key),
            Mode::BufferList { .. } => self.handle_bufferlist_key(key),
            Mode::ConfirmClose | Mode::ConfirmQuit => self.handle_confirm_key(key),
        }
    }

    // ---- Edit-mode actions ------------------------------------------------

    fn apply(&mut self, action: Action) {
        match action {
            Action::MoveLeft => {
                let b = self.buf_mut();
                b.view.move_left(&b.doc);
            }
            Action::MoveRight => {
                let b = self.buf_mut();
                b.view.move_right(&b.doc);
            }
            Action::MoveUp => {
                let b = self.buf_mut();
                b.view.move_up(&b.doc);
            }
            Action::MoveDown => {
                let b = self.buf_mut();
                b.view.move_down(&b.doc);
            }
            Action::MoveHome => self.buf_mut().view.move_home(),
            Action::MoveEnd => {
                let b = self.buf_mut();
                b.view.move_end(&b.doc);
            }
            Action::PageUp => {
                let page = self.page;
                let b = self.buf_mut();
                b.view.move_page_up(&b.doc, page);
            }
            Action::PageDown => {
                let page = self.page;
                let b = self.buf_mut();
                b.view.move_page_down(&b.doc, page);
            }
            Action::InsertChar(c) => self.edit(|v, d| v.insert_char(d, c)),
            Action::Newline => self.edit(|v, d| v.insert_newline(d)),
            Action::Backspace => self.edit(|v, d| v.backspace(d)),
            Action::Delete => self.edit(|v, d| v.delete(d)),
            Action::Save => self.save(),
            Action::Quit => {
                if self.buffers.iter().any(Buffer::is_dirty) {
                    self.mode = Mode::ConfirmQuit;
                } else {
                    self.should_quit = true;
                }
            }
            Action::ToggleFold => self.toggle_fold(),
            Action::NextHeading => {
                let b = self.buf_mut();
                if let Some(line) = next_heading(&b.outline, b.view.cursor_line()) {
                    b.view.move_to_line(&b.doc, line);
                }
            }
            Action::PrevHeading => {
                let b = self.buf_mut();
                if let Some(line) = prev_heading(&b.outline, b.view.cursor_line()) {
                    b.view.move_to_line(&b.doc, line);
                }
            }
            Action::CycleTodo => {
                let b = self.buf_mut();
                b.format.cycle_todo(&mut b.doc, b.view.cursor_line());
                self.reparse();
            }
            Action::OpenFile => {
                self.mode = Mode::OpenFile {
                    input: String::new(),
                };
            }
            Action::NextBuffer => self.switch_to((self.active + 1) % self.buffers.len()),
            Action::PrevBuffer => {
                self.switch_to((self.active + self.buffers.len() - 1) % self.buffers.len())
            }
            Action::ListBuffers => {
                self.mode = Mode::BufferList {
                    selected: self.active,
                };
            }
            Action::CloseBuffer => {
                if self.buf().is_dirty() {
                    self.mode = Mode::ConfirmClose;
                } else {
                    self.close_active_buffer();
                }
            }
            Action::PromoteHeading => self.structure_edit(|f, d, l| f.promote_heading(d, l)),
            Action::DemoteHeading => self.structure_edit(|f, d, l| f.demote_heading(d, l)),
            Action::PromoteSubtree => self.structure_edit(|f, d, l| f.promote_subtree(d, l)),
            Action::DemoteSubtree => self.structure_edit(|f, d, l| f.demote_subtree(d, l)),
            Action::MoveSubtreeUp => self.structure_edit(|f, d, l| f.move_subtree_up(d, l)),
            Action::MoveSubtreeDown => self.structure_edit(|f, d, l| f.move_subtree_down(d, l)),
            Action::InsertSibling => self.insert_sibling(false),
            Action::InsertTodoSibling => self.insert_sibling(true),
            Action::PriorityUp => self.shift_date_or_priority(true),
            Action::PriorityDown => self.shift_date_or_priority(false),
            Action::EditTags => self.open_tags_prompt(),
            Action::SetScheduled => self.open_date_prompt(DatePurpose::Scheduled),
            Action::SetDeadline => self.open_date_prompt(DatePurpose::Deadline),
            Action::InsertActiveTs => self.open_date_prompt(DatePurpose::InsertActive),
            Action::InsertInactiveTs => self.open_date_prompt(DatePurpose::InsertInactive),
        }
    }

    /// `Shift+↑/↓`: shift the timestamp field under the cursor if there is one, else cycle the
    /// heading's priority — the Org overloading of these keys.
    fn shift_date_or_priority(&mut self, up: bool) {
        let line = self.buf().view.cursor_line();
        let col = self.buf().view.cursor_column();
        match shift_timestamp(&mut self.buf_mut().doc, line, col, up) {
            Some(EditOutcome::Changed { cursor_line }) => {
                let b = self.buf_mut();
                b.view.move_to_line(&b.doc, cursor_line);
                self.reparse();
            }
            Some(EditOutcome::NoOp(msg)) => self.status = msg.into(),
            None => self.structure_edit(|f, d, l| f.cycle_priority(d, l, up)),
        }
    }

    // ---- structural editing -------------------------------------------------

    /// Run a structural edit on the active buffer, then sync cursor, status, and outline.
    fn structure_edit(&mut self, op: impl FnOnce(Format, &mut Document, usize) -> EditOutcome) {
        let b = self.buf_mut();
        match op(b.format, &mut b.doc, b.view.cursor_line()) {
            EditOutcome::Changed { cursor_line } => {
                let b = self.buf_mut();
                b.view.move_to_line(&b.doc, cursor_line);
                self.reparse();
            }
            EditOutcome::NoOp(msg) => self.status = msg.into(),
        }
    }

    /// Insert a sibling heading and leave the cursor at the end of its (empty) title.
    fn insert_sibling(&mut self, todo: bool) {
        let b = self.buf_mut();
        match b.format.insert_sibling(&mut b.doc, b.view.cursor_line(), todo) {
            EditOutcome::Changed { cursor_line } => {
                let b = self.buf_mut();
                b.view.move_to_line(&b.doc, cursor_line);
                b.view.move_end(&b.doc);
                self.reparse();
            }
            EditOutcome::NoOp(msg) => self.status = msg.into(),
        }
    }

    /// `Ctrl+G`: open the tags prompt pre-filled with the enclosing heading's tags.
    fn open_tags_prompt(&mut self) {
        let b = self.buf();
        let line = b.view.cursor_line();
        match b.outline.headings.iter().rev().find(|h| h.line <= line) {
            Some(h) => {
                self.mode = Mode::EditTags {
                    input: h.tags.join(" "),
                };
            }
            None => self.status = "Not inside a heading's subtree".into(),
        }
    }

    /// Remove the active buffer, keeping the invariants: the list never empties (a fresh
    /// untitled buffer replaces the last one) and `active` stays in bounds.
    fn close_active_buffer(&mut self) {
        self.buffers.remove(self.active);
        if self.buffers.is_empty() {
            self.buffers.push(Buffer::untitled());
        }
        self.active = self.active.min(self.buffers.len() - 1);
    }

    /// Answer a y/n confirmation ([`Mode::ConfirmClose`] or [`Mode::ConfirmQuit`]).
    fn handle_confirm_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let quitting = self.mode == Mode::ConfirmQuit;
                self.mode = Mode::Edit;
                if quitting {
                    self.should_quit = true;
                } else {
                    self.close_active_buffer();
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.mode = Mode::Edit,
            _ => {}
        }
    }

    /// Make `index` the active buffer and announce it on the status line.
    fn switch_to(&mut self, index: usize) {
        self.active = index;
        if self.buffers.len() > 1 {
            self.status = format!(
                "{} ({}/{})",
                self.buf().display_name(),
                self.active + 1,
                self.buffers.len()
            );
        }
    }

    /// Run an editing closure on the active view+document, then re-derive the outline.
    fn edit(&mut self, f: impl FnOnce(&mut View, &mut Document)) {
        let b = self.buf_mut();
        f(&mut b.view, &mut b.doc);
        self.reparse();
    }

    /// `Tab`: fold/unfold when the caret sits on a heading, otherwise insert a tab.
    fn toggle_fold(&mut self) {
        let b = self.buf_mut();
        let line = b.view.cursor_line();
        if b.outline.headings.iter().any(|h| h.line == line) {
            if !b.folded.remove(&line) {
                b.folded.insert(line);
            }
        } else {
            self.edit(|v, d| v.insert_char(d, '\t'));
        }
    }

    /// Re-parse the outline after an edit and drop folds whose heading line no longer exists.
    fn reparse(&mut self) {
        let b = self.buf_mut();
        b.outline = b.format.parse(&b.doc);
        let heading_lines: HashSet<usize> = b.outline.headings.iter().map(|h| h.line).collect();
        b.folded.retain(|line| heading_lines.contains(line));
    }

    // ---- saving -----------------------------------------------------------

    fn save(&mut self) {
        if self.buf().doc.path().is_some() {
            match self.buf_mut().doc.save() {
                Ok(()) => self.status = "Saved".into(),
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        } else if let Some(path) = self.buf().stash_path.clone() {
            self.save_as(&path);
        } else {
            self.mode = Mode::SaveAs {
                input: String::new(),
            };
        }
    }

    fn save_as(&mut self, path: &Path) {
        match self.buf_mut().doc.save_as(path) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                let b = self.buf_mut();
                b.stash_path = None;
                // The new name may mean a new format (e.g. saved as .md) — re-detect and
                // re-parse so the outline (and any now-stale folds) follow the format.
                b.format = detect_format(b.doc.path());
                self.reparse();
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    // ---- the bottom-line prompts (Save As / Open) --------------------------

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        enum Kind {
            SaveAs,
            Open,
            Tags,
            Date(DatePurpose),
        }
        let kind = match &self.mode {
            Mode::SaveAs { .. } => Kind::SaveAs,
            Mode::OpenFile { .. } => Kind::Open,
            Mode::EditTags { .. } => Kind::Tags,
            Mode::DatePrompt { purpose, .. } => Kind::Date(*purpose),
            _ => return,
        };
        // Any prompt keystroke clears the previous transient message (completion hints, errors)
        // so it reads as feedback for the last action, not stale text.
        if key.kind == KeyEventKind::Press {
            self.status.clear();
        }
        // Tab completes a path in the Open / Save As prompts.
        if key.code == KeyCode::Tab
            && key.kind == KeyEventKind::Press
            && matches!(kind, Kind::SaveAs | Kind::Open)
        {
            let completed = match &self.mode {
                Mode::SaveAs { input } | Mode::OpenFile { input } => Some(complete_path(input)),
                _ => None,
            };
            if let Some((new_input, status)) = completed {
                match &mut self.mode {
                    Mode::SaveAs { input } | Mode::OpenFile { input } => *input = new_input,
                    _ => {}
                }
                self.status = status.unwrap_or_default();
            }
            return;
        }
        let event = match &mut self.mode {
            Mode::SaveAs { input }
            | Mode::OpenFile { input }
            | Mode::EditTags { input }
            | Mode::DatePrompt { input, .. } => prompt_event(input, key),
            _ => return,
        };
        match event {
            PromptEvent::Pending => {}
            PromptEvent::Cancelled => self.mode = Mode::Edit,
            PromptEvent::Submitted(text) => {
                self.mode = Mode::Edit;
                match kind {
                    Kind::Tags => self.apply_tags(&text),
                    Kind::Date(purpose) => self.apply_date(purpose, &text),
                    Kind::SaveAs | Kind::Open => {
                        if text.trim().is_empty() {
                            return;
                        }
                        let path = expand_path(&text);
                        if matches!(kind, Kind::SaveAs) {
                            self.save_as(&path);
                        } else {
                            self.open_path(path);
                        }
                    }
                }
            }
        }
    }

    /// Open the date prompt for `purpose`. For scheduled/deadline (Org only) the prompt is
    /// pre-filled with the heading's current stamp, if any.
    fn open_date_prompt(&mut self, purpose: DatePurpose) {
        if matches!(purpose, DatePurpose::Scheduled | DatePurpose::Deadline)
            && self.buf().format != Format::Org
        {
            self.status = "SCHEDULED/DEADLINE is an Org feature".into();
            return;
        }
        let input = self.existing_planning(purpose);
        self.mode = Mode::DatePrompt { input, purpose };
    }

    /// The heading's current SCHEDULED/DEADLINE stamp as text, to pre-fill the prompt.
    fn existing_planning(&self, purpose: DatePurpose) -> String {
        let b = self.buf();
        let line = b.view.cursor_line();
        let Some(h) = b.outline.headings.iter().rev().find(|h| h.line <= line) else {
            return String::new();
        };
        let ts = match purpose {
            DatePurpose::Scheduled => h.scheduled,
            DatePurpose::Deadline => h.deadline,
            _ => None,
        };
        ts.map(|t| t.to_string()).unwrap_or_default()
    }

    /// Parse the typed date and act on it. Invalid input keeps the prompt open with a message.
    fn apply_date(&mut self, purpose: DatePurpose, text: &str) {
        let trimmed = text.trim();
        // Empty input on a planning command removes that entry.
        let ts = if trimmed.is_empty() {
            None
        } else {
            match parse_date_input(trimmed, purpose) {
                Some(ts) => Some(ts),
                None => {
                    self.status = "Invalid date — try 2024-01-15 or 2024-01-15 09:30".into();
                    self.mode = Mode::DatePrompt {
                        input: text.to_string(),
                        purpose,
                    };
                    return;
                }
            }
        };
        match purpose {
            DatePurpose::Scheduled | DatePurpose::Deadline => {
                let which = if matches!(purpose, DatePurpose::Scheduled) {
                    Planning::Scheduled
                } else {
                    Planning::Deadline
                };
                let b = self.buf_mut();
                let line = b.view.cursor_line();
                match b.format.set_planning(&mut b.doc, line, which, ts) {
                    EditOutcome::Changed { .. } => self.reparse(),
                    EditOutcome::NoOp(msg) => self.status = msg.into(),
                }
            }
            DatePurpose::InsertActive | DatePurpose::InsertInactive => {
                let Some(ts) = ts else { return }; // empty insert does nothing
                let b = self.buf_mut();
                let at = b.doc.line_to_char(b.view.cursor_line()) + b.view.cursor_column();
                b.doc.insert(at, &ts.to_string());
                self.reparse();
            }
        }
    }

    /// Validate and write the space-separated tags typed into the prompt.
    fn apply_tags(&mut self, text: &str) {
        let tags: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        if let Some(bad) = tags.iter().find(|t| !is_valid_tag(t)) {
            self.status = format!("Invalid tag {bad:?} — use letters, digits, _ @ # %");
            self.mode = Mode::EditTags {
                input: text.to_string(),
            };
            return;
        }
        let b = self.buf_mut();
        match b.format.set_tags(&mut b.doc, b.view.cursor_line(), &tags) {
            EditOutcome::Changed { .. } => self.reparse(),
            EditOutcome::NoOp(msg) => self.status = msg.into(),
        }
    }

    // ---- the buffer-list picker --------------------------------------------

    fn handle_bufferlist_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        let Mode::BufferList { selected } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Edit,
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Down => *selected = (*selected + 1).min(self.buffers.len() - 1),
            KeyCode::Enter => {
                let index = *selected;
                self.mode = Mode::Edit;
                self.switch_to(index);
            }
            // 1-9 jump straight to that buffer (arrows cover a longer list).
            KeyCode::Char(c @ '1'..='9') => {
                let index = c as usize - '1' as usize;
                if index < self.buffers.len() {
                    self.mode = Mode::Edit;
                    self.switch_to(index);
                }
            }
            _ => {}
        }
    }

    /// Open `path`: switch to its buffer if it is already open (by document path *or* stashed
    /// first-save path), else load it from disk, else start an empty buffer that will first-save
    /// there — the same semantics as a CLI argument.
    fn open_path(&mut self, path: PathBuf) {
        if let Some(index) = self.buffers.iter().position(|b| b.matches_path(&path)) {
            self.switch_to(index);
            return;
        }
        let (buffer, opened) = if path.exists() {
            match Document::open(&path) {
                Ok(doc) => (Buffer::new(doc, None), true),
                Err(e) => {
                    self.status = format!("cannot open {}: {e}", path.display());
                    return;
                }
            }
        } else {
            (Buffer::new(Document::new(), Some(path)), false)
        };
        // Make the outcome explicit so an unresolved path can't masquerade as a loaded file.
        let name = buffer.display_name();
        self.buffers.push(buffer);
        self.switch_to(self.buffers.len() - 1);
        self.status = if opened {
            format!("Opened {name}")
        } else {
            format!("New file: {name}")
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    // Small helpers to drive the app the way the event loop does.
    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    fn ctrl(app: &mut App, c: char) {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
    }
    fn typ(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    /// An app over one buffer — the pre-multi-buffer constructor shape, kept for the tests.
    fn single(doc: Document, stash_path: Option<PathBuf>) -> App {
        App::new(vec![Buffer::new(doc, stash_path)])
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("textr_app_{}_{}.org", name, std::process::id()))
    }

    // ---- path tab-completion --------------------------------------------------

    fn common(names: &[&str]) -> String {
        common_prefix(names.iter().copied())
    }

    #[test]
    fn common_prefix_of_names() {
        assert_eq!(common(&["alpha", "alpine", "al"]), "al");
        assert_eq!(common(&["one"]), "one");
        assert_eq!(common(&["a", "b"]), "");
        assert_eq!(common(&[]), "");
    }

    /// Create a temp directory containing `entries` (name, is_dir), returning its path.
    fn make_dir(label: &str, entries: &[(&str, bool)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("torg_tab_{}_{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, is_dir) in entries {
            let p = dir.join(name);
            if *is_dir {
                std::fs::create_dir_all(&p).unwrap();
            } else {
                std::fs::write(&p, "").unwrap();
            }
        }
        dir
    }

    #[test]
    fn tab_completes_a_unique_prefix_and_slashes_directories() {
        let dir = make_dir("unique", &[("notes.org", false), ("other.txt", false), ("sub", true)]);
        let (out, status) = complete_path(&format!("{}/not", dir.display()));
        assert_eq!(out, format!("{}/notes.org", dir.display()));
        assert!(status.is_none());
        let (out, _) = complete_path(&format!("{}/su", dir.display()));
        assert_eq!(out, format!("{}/sub/", dir.display())); // directory → trailing slash
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_extends_to_the_common_prefix_when_ambiguous() {
        let dir = make_dir("ambig", &[("report-a.org", false), ("report-b.org", false)]);
        let (out, status) = complete_path(&format!("{}/rep", dir.display()));
        assert_eq!(out, format!("{}/report-", dir.display()));
        assert!(status.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_lists_candidates_when_it_cannot_extend() {
        let dir = make_dir("list", &[("apple", false), ("banana", false)]);
        let input = format!("{}/", dir.display());
        let (out, status) = complete_path(&input);
        assert_eq!(out, input); // unchanged
        let s = status.unwrap();
        assert!(s.contains("apple") && s.contains("banana"), "status was {s:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_reports_no_match() {
        let dir = make_dir("nomatch", &[("apple", false)]);
        let (out, status) = complete_path(&format!("{}/zzz", dir.display()));
        assert_eq!(out, format!("{}/zzz", dir.display()));
        assert!(status.unwrap().to_lowercase().contains("no match"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_hides_dotfiles_unless_the_partial_is_dotted() {
        let dir = make_dir("dot", &[(".hidden.org", false), ("visible.org", false)]);
        let (out, _) = complete_path(&format!("{}/", dir.display()));
        assert_eq!(out, format!("{}/visible.org", dir.display())); // dotfile skipped
        let (out2, _) = complete_path(&format!("{}/.hid", dir.display()));
        assert_eq!(out2, format!("{}/.hidden.org", dir.display())); // dotted partial reaches it
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_in_the_open_prompt_completes_the_input() {
        let dir = make_dir("appopen", &[("uniquename.org", false)]);
        let mut app = single(Document::from_text("home\n"), None);
        ctrl(&mut app, 'o');
        typ(&mut app, &format!("{}/uniq", dir.display()));
        press(&mut app, KeyCode::Tab);
        assert_eq!(
            app.mode(),
            &Mode::OpenFile {
                input: format!("{}/uniquename.org", dir.display())
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_in_the_save_as_prompt_also_completes() {
        let dir = make_dir("appsave", &[("draft.org", false)]);
        let mut app = single(Document::from_text("x"), None);
        ctrl(&mut app, 's'); // untitled → Save As prompt
        typ(&mut app, &format!("{}/dra", dir.display()));
        press(&mut app, KeyCode::Tab);
        assert_eq!(
            app.mode(),
            &Mode::SaveAs {
                input: format!("{}/draft.org", dir.display())
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_with_several_matches_shows_them_on_the_status_line() {
        let dir = make_dir("appmany", &[("apple", false), ("banana", false)]);
        let mut app = single(Document::from_text("home\n"), None);
        ctrl(&mut app, 'o');
        typ(&mut app, &format!("{}/", dir.display()));
        press(&mut app, KeyCode::Tab);
        assert!(matches!(app.mode(), Mode::OpenFile { .. })); // still prompting
        assert!(app.status().contains("apple") && app.status().contains("banana"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- path expansion in the Open / Save As prompts -------------------------

    #[test]
    fn expand_path_expands_a_leading_tilde() {
        let home = home_dir().expect("HOME set in the test environment");
        assert_eq!(expand_path("~"), home);
        assert_eq!(expand_path("~/scripts/foo.sh"), home.join("scripts/foo.sh"));
        // A tilde not at the start is left alone.
        assert_eq!(expand_path("/etc/~x"), PathBuf::from("/etc/~x"));
    }

    #[test]
    fn expand_path_expands_environment_variables() {
        // PATH always exists; compare against the real value without mutating the env.
        let path_val = std::env::var("PATH").unwrap();
        assert_eq!(expand_path("$PATH/bin"), PathBuf::from(format!("{path_val}/bin")));
        assert_eq!(expand_path("${PATH}/bin"), PathBuf::from(format!("{path_val}/bin")));
        // An undefined variable is left verbatim rather than blanked.
        assert_eq!(
            expand_path("$TORG_NOPE_UNDEFINED/x"),
            PathBuf::from("$TORG_NOPE_UNDEFINED/x")
        );
    }

    #[test]
    fn expand_path_leaves_a_plain_absolute_path_unchanged() {
        assert_eq!(expand_path("/tmp/a/b.sh"), PathBuf::from("/tmp/a/b.sh"));
    }

    #[test]
    fn ctrl_o_reports_opened_for_an_existing_file_and_new_for_a_missing_one() {
        let path = temp_path("open_status");
        std::fs::write(&path, "* content\n").unwrap();
        let mut app = single(Document::from_text("home\n"), None);

        ctrl(&mut app, 'o');
        typ(&mut app, path.to_str().unwrap());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.document().text(), "* content\n");
        assert!(app.status().starts_with("Opened "), "status was {:?}", app.status());

        let missing = temp_path("open_missing");
        let _ = std::fs::remove_file(&missing);
        ctrl(&mut app, 'o');
        typ(&mut app, missing.to_str().unwrap());
        press(&mut app, KeyCode::Enter);
        assert!(app.status().starts_with("New file: "), "status was {:?}", app.status());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ctrl_o_with_a_tilde_path_opens_the_real_file() {
        // Write a file under $HOME and open it via ~, proving expansion runs in the prompt.
        let home = home_dir().expect("HOME set");
        let name = format!("torg_tilde_{}.org", std::process::id());
        let path = home.join(&name);
        std::fs::write(&path, "* tilde\n").unwrap();
        let mut app = single(Document::from_text("home\n"), None);

        ctrl(&mut app, 'o');
        typ(&mut app, &format!("~/{name}"));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.document().text(), "* tilde\n");
        assert!(app.status().starts_with("Opened "));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ctrl_s_with_a_path_saves_and_clears_modified() {
        let path = temp_path("save");
        std::fs::write(&path, "* a\n").unwrap();
        let mut app = single(Document::open(&path).unwrap(), None);
        typ(&mut app, "x"); // modify
        assert!(app.document().is_modified());

        ctrl(&mut app, 's');

        assert!(!app.document().is_modified());
        assert_eq!(app.status(), "Saved");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ctrl_s_without_a_path_opens_the_saveas_prompt() {
        let mut app = single(Document::from_text("hello"), None);
        ctrl(&mut app, 's');
        assert!(matches!(app.mode(), Mode::SaveAs { .. }));
    }

    #[test]
    fn saveas_prompt_types_writes_on_enter_and_returns_to_edit() {
        let path = temp_path("saveas");
        let _ = std::fs::remove_file(&path);
        let mut app = single(Document::from_text("brand new"), None);

        ctrl(&mut app, 's'); // open prompt
        typ(&mut app, path.to_str().unwrap());
        typ(&mut app, "z"); // a stray char...
        press(&mut app, KeyCode::Backspace); // ...that Backspace removes → path restored
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "brand new");
        assert!(!app.document().is_modified()); // a successful save clears the dirty flag
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn saveas_esc_cancels_and_leaves_the_buffer_unsaved() {
        let mut app = single(Document::from_text("data"), None);
        typ(&mut app, "!"); // now modified
        ctrl(&mut app, 's'); // prompt
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.mode(), &Mode::Edit);
        assert!(app.document().is_modified()); // nothing was written
    }

    #[test]
    fn missing_file_buffer_saves_to_the_stashed_path_without_prompting() {
        let path = temp_path("stash");
        let _ = std::fs::remove_file(&path);
        let mut app = single(Document::new(), Some(path.clone()));
        typ(&mut app, "* hi\n");

        ctrl(&mut app, 's'); // should save_as(stash), NOT open a prompt

        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "* hi\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tab_on_a_heading_toggles_a_fold_and_hides_its_subtree() {
        let mut app = single(Document::from_text("* A\nbody\n* B\n"), None);
        // caret at (0,0), on heading A
        press(&mut app, KeyCode::Tab);
        assert!(app.is_folded_heading(0));
        assert!(app.is_hidden(1)); // "body" is inside A's subtree
        assert!(!app.is_hidden(2)); // heading B is not
        press(&mut app, KeyCode::Tab); // unfold
        assert!(!app.is_folded_heading(0));
        assert!(!app.is_hidden(1));
    }

    #[test]
    fn tab_off_a_heading_inserts_a_tab() {
        let mut app = single(Document::from_text("plain\n"), None);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.document().text(), "\tplain\n");
    }

    #[test]
    fn ctrl_n_and_ctrl_p_jump_between_headings() {
        let mut app = single(Document::from_text("* A\nx\n* B\ny\n* C\n"), None);
        ctrl(&mut app, 'n'); // A → B (line 2)
        assert_eq!(app.view().cursor_line(), 2);
        ctrl(&mut app, 'n'); // B → C (line 4)
        assert_eq!(app.view().cursor_line(), 4);
        ctrl(&mut app, 'p'); // C → B
        assert_eq!(app.view().cursor_line(), 2);
    }

    #[test]
    fn ctrl_t_cycles_the_heading_todo_keyword() {
        let mut app = single(Document::from_text("* task\n"), None);
        ctrl(&mut app, 't');
        assert_eq!(app.document().text(), "* TODO task\n");
        ctrl(&mut app, 't');
        assert_eq!(app.document().text(), "* DONE task\n");
    }

    #[test]
    fn editing_reparses_so_a_new_heading_is_recognized() {
        let mut app = single(Document::from_text("plain\n"), None);
        assert!(app.outline().headings.is_empty());
        press(&mut app, KeyCode::Home);
        typ(&mut app, "* "); // turn the line into a heading
        assert_eq!(app.outline().headings.len(), 1);
    }

    /// A buffer that torg treats as Markdown: an untitled doc stashed to a `.md` path.
    fn md_app(text: &str) -> App {
        single(Document::from_text(text), Some(PathBuf::from("virtual.md")))
    }

    #[test]
    fn tab_folds_and_ctrl_n_navigates_markdown_headings() {
        let mut app = md_app("# A\nbody\n# B\n");
        press(&mut app, KeyCode::Tab); // caret on "# A"
        assert!(app.is_folded_heading(0));
        assert!(app.is_hidden(1));
        ctrl(&mut app, 'n');
        assert_eq!(app.view().cursor_line(), 2); // jumped to "# B"
    }

    #[test]
    fn ctrl_t_cycles_todo_in_a_markdown_buffer() {
        let mut app = md_app("# task\n");
        ctrl(&mut app, 't');
        assert_eq!(app.document().text(), "# TODO task\n");
        ctrl(&mut app, 't');
        assert_eq!(app.document().text(), "# DONE task\n");
    }

    #[test]
    fn each_buffer_uses_its_own_provider() {
        let mut app = App::new(vec![
            Buffer::new(Document::from_text("# md-style\n* org-style\n"), None), // Org buffer
            Buffer::new(
                Document::from_text("# md-style\n* org-style\n"),
                Some(PathBuf::from("x.md")), // Markdown buffer, same text
            ),
        ]);
        assert_eq!(app.outline().headings[0].title, "org-style");
        alt(&mut app, 'n');
        assert_eq!(app.outline().headings[0].title, "md-style");
        ctrl(&mut app, 't'); // cursor on line 0 = "# md-style", a heading here
        assert_eq!(app.document().text(), "# TODO md-style\n* org-style\n");
    }

    #[test]
    fn save_as_to_md_redetects_the_format() {
        let path = temp_path("redetect").with_extension("md");
        let _ = std::fs::remove_file(&path);
        let mut app = single(Document::from_text("# A\nbody\n* B\n"), None);
        assert_eq!(app.outline().headings[0].title, "B"); // untitled → Org default

        ctrl(&mut app, 's'); // Save As prompt
        typ(&mut app, path.to_str().unwrap());
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.outline().headings.len(), 1);
        assert_eq!(app.outline().headings[0].title, "A"); // now parsed as Markdown
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_as_format_change_drops_stale_folds() {
        let path = temp_path("foldflip").with_extension("md");
        let _ = std::fs::remove_file(&path);
        let mut app = single(Document::from_text("* A\nbody\n"), None);
        press(&mut app, KeyCode::Tab); // fold the Org heading
        assert!(app.is_folded_heading(0));

        ctrl(&mut app, 's');
        typ(&mut app, path.to_str().unwrap());
        press(&mut app, KeyCode::Enter);

        assert!(!app.is_folded_heading(0)); // "* A" is not a Markdown heading
        assert!(!app.is_hidden(1));
        let _ = std::fs::remove_file(&path);
    }

    // ---- structural editing ---------------------------------------------------

    fn alt_key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::ALT));
    }
    fn alt_shift(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT));
    }
    fn shift_key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::SHIFT));
    }

    #[test]
    fn alt_arrows_promote_demote_and_move() {
        let mut app = single(Document::from_text("** A\nbody\n** B\n"), None);
        alt_key(&mut app, KeyCode::Left);
        assert_eq!(app.document().text(), "* A\nbody\n** B\n");
        alt_key(&mut app, KeyCode::Right);
        alt_key(&mut app, KeyCode::Down); // A's subtree past B's
        assert_eq!(app.document().text(), "** B\n** A\nbody\n");
        assert_eq!(app.view().cursor_line(), 1); // cursor followed A
    }

    #[test]
    fn alt_shift_arrows_shift_the_whole_subtree() {
        let mut app = single(Document::from_text("* A\n** child\n"), None);
        alt_shift(&mut app, KeyCode::Right);
        assert_eq!(app.document().text(), "** A\n*** child\n");
    }

    #[test]
    fn refused_edits_show_the_reason_on_the_status_line() {
        let mut app = single(Document::from_text("* top\n"), None);
        alt_key(&mut app, KeyCode::Left);
        assert_eq!(app.document().text(), "* top\n");
        assert!(!app.status().is_empty());
    }

    #[test]
    fn alt_enter_inserts_a_sibling_and_puts_the_cursor_on_it() {
        let mut app = single(Document::from_text("* A\nbody\n"), None);
        alt_key(&mut app, KeyCode::Enter);
        assert_eq!(app.document().text(), "* A\nbody\n* \n");
        assert_eq!(app.view().cursor_line(), 2);
        typ(&mut app, "typed title");
        assert_eq!(app.document().text(), "* A\nbody\n* typed title\n");
    }

    #[test]
    fn shift_arrows_cycle_priority() {
        let mut app = single(Document::from_text("* TODO t\n"), None);
        shift_key(&mut app, KeyCode::Up);
        assert_eq!(app.document().text(), "* TODO [#C] t\n");
        shift_key(&mut app, KeyCode::Down);
        assert_eq!(app.document().text(), "* TODO t\n");
    }

    #[test]
    fn structural_edits_respect_the_buffer_format() {
        let mut app = single(Document::from_text("# A\n"), Some(PathBuf::from("x.md")));
        alt_key(&mut app, KeyCode::Right);
        assert_eq!(app.document().text(), "## A\n");
    }

    #[test]
    fn ctrl_g_prefills_edits_and_writes_tags() {
        let mut app = single(Document::from_text("* task :old:\n"), None);
        ctrl(&mut app, 'g');
        assert_eq!(app.mode(), &Mode::EditTags { input: "old".into() });
        for _ in 0..3 {
            press(&mut app, KeyCode::Backspace); // clear "old"
        }
        typ(&mut app, "work urgent");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.document().text(), "* task :work:urgent:\n");
    }

    #[test]
    fn empty_tags_input_removes_the_tag_run() {
        let mut app = single(Document::from_text("* task :old:\n"), None);
        ctrl(&mut app, 'g');
        for _ in 0..3 {
            press(&mut app, KeyCode::Backspace);
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.document().text(), "* task\n");
    }

    #[test]
    fn invalid_tag_characters_keep_the_prompt_open_with_a_message() {
        let mut app = single(Document::from_text("* task\n"), None);
        ctrl(&mut app, 'g');
        typ(&mut app, "bad!tag");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode(), Mode::EditTags { .. })); // still prompting
        assert!(!app.status().is_empty());
        assert_eq!(app.document().text(), "* task\n");
    }

    #[test]
    fn ctrl_g_off_any_heading_is_a_status_noop() {
        let mut app = single(Document::from_text("prose only\n"), None);
        ctrl(&mut app, 'g');
        assert_eq!(app.mode(), &Mode::Edit);
        assert!(!app.status().is_empty());
    }

    // ---- dates and scheduling -------------------------------------------------

    #[test]
    fn alt_s_prompts_then_writes_an_indented_scheduled_line() {
        let mut app = single(Document::from_text("* Task\nbody\n"), None);
        alt_key(&mut app, KeyCode::Char('s'));
        assert!(matches!(app.mode(), Mode::DatePrompt { .. }));
        typ(&mut app, "2024-01-15 09:30");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(
            app.document().text(),
            "* Task\n  SCHEDULED: <2024-01-15 Mon 09:30>\nbody\n"
        );
    }

    #[test]
    fn an_invalid_date_keeps_the_prompt_open_with_a_message() {
        let mut app = single(Document::from_text("* Task\n"), None);
        alt_key(&mut app, KeyCode::Char('d'));
        typ(&mut app, "not a date");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode(), Mode::DatePrompt { .. }));
        assert!(!app.status().is_empty());
        assert_eq!(app.document().text(), "* Task\n");
    }

    #[test]
    fn empty_date_input_removes_the_planning_entry() {
        let mut app = single(Document::from_text("* T\n  SCHEDULED: <2024-01-15 Mon>\n"), None);
        alt_key(&mut app, KeyCode::Char('s'));
        assert_eq!(
            app.mode(),
            &Mode::DatePrompt {
                input: "<2024-01-15 Mon>".into(),
                purpose: DatePurpose::Scheduled,
            }
        );
        // clear the prefilled value
        for _ in 0.."<2024-01-15 Mon>".len() {
            press(&mut app, KeyCode::Backspace);
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.document().text(), "* T\n");
    }

    #[test]
    fn scheduling_in_a_markdown_buffer_is_a_noop() {
        let mut app = single(Document::from_text("# Task\n"), Some(PathBuf::from("x.md")));
        alt_key(&mut app, KeyCode::Char('s'));
        assert_eq!(app.mode(), &Mode::Edit);
        assert!(!app.status().is_empty());
    }

    #[test]
    fn alt_dot_inserts_an_active_timestamp_at_the_cursor() {
        let mut app = single(Document::from_text("* Task\n"), None);
        alt_key(&mut app, KeyCode::Char('.'));
        typ(&mut app, "2024-01-15");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.document().text(), "<2024-01-15 Mon>* Task\n");
    }

    #[test]
    fn shift_up_on_a_timestamp_shifts_the_date_not_the_priority() {
        let mut app = single(Document::from_text("* T\n  SCHEDULED: <2024-01-15 Mon>\n"), None);
        press(&mut app, KeyCode::Down); // onto the planning line
        press(&mut app, KeyCode::End);
        // cursor is at end; move back onto the day digits
        press(&mut app, KeyCode::Left);
        press(&mut app, KeyCode::Left); // onto "Mon"/day area
        shift_key(&mut app, KeyCode::Up);
        assert!(app.document().text().contains("2024-01-1")); // date changed, still a stamp
        assert!(!app.document().text().contains("[#")); // no priority cookie added
    }

    #[test]
    fn shift_up_off_a_timestamp_still_cycles_priority() {
        let mut app = single(Document::from_text("* TODO task\n"), None);
        shift_key(&mut app, KeyCode::Up);
        assert_eq!(app.document().text(), "* TODO [#C] task\n");
    }

    #[test]
    fn an_empty_buffer_list_gets_one_untitled_buffer() {
        let app = App::new(Vec::new());
        assert_eq!(app.buffer_count(), 1);
        assert_eq!(app.buffer_labels()[0], ("[No Name]".to_string(), false));
    }

    /// An app over three in-memory buffers with distinguishable text.
    fn three_buffers() -> App {
        App::new(vec![
            Buffer::new(Document::from_text("first\n"), None),
            Buffer::new(Document::from_text("second\n"), None),
            Buffer::new(Document::from_text("third\n"), None),
        ])
    }
    fn alt(app: &mut App, c: char) {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT));
    }

    #[test]
    fn alt_n_and_alt_p_cycle_through_buffers_and_wrap() {
        let mut app = three_buffers();
        assert_eq!(app.active_index(), 0);
        alt(&mut app, 'n');
        assert_eq!(app.active_index(), 1);
        assert_eq!(app.document().text(), "second\n");
        alt(&mut app, 'n');
        alt(&mut app, 'n'); // wraps 2 → 0
        assert_eq!(app.active_index(), 0);
        alt(&mut app, 'p'); // wraps 0 → 2
        assert_eq!(app.active_index(), 2);
        assert_eq!(app.document().text(), "third\n");
    }

    #[test]
    fn ctrl_o_prompt_opens_an_existing_file_and_activates_it() {
        let path = temp_path("open");
        std::fs::write(&path, "* opened\n").unwrap();
        let mut app = single(Document::from_text("home\n"), None);

        ctrl(&mut app, 'o');
        assert!(matches!(app.mode(), Mode::OpenFile { .. }));
        typ(&mut app, path.to_str().unwrap());
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.buffer_count(), 2);
        assert_eq!(app.active_index(), 1);
        assert_eq!(app.document().text(), "* opened\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn opening_an_already_open_path_switches_instead_of_duplicating() {
        let path = temp_path("reopen");
        std::fs::write(&path, "again\n").unwrap();
        let mut app = App::new(vec![
            Buffer::new(Document::open(&path).unwrap(), None),
            Buffer::new(Document::from_text("other\n"), None),
        ]);
        alt(&mut app, 'n'); // active = 1

        ctrl(&mut app, 'o');
        typ(&mut app, path.to_str().unwrap());
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.buffer_count(), 2); // no duplicate
        assert_eq!(app.active_index(), 0); // switched back
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn opening_the_same_missing_path_twice_reuses_the_stash_buffer() {
        let path = temp_path("missing_twice");
        let _ = std::fs::remove_file(&path);
        let mut app = single(Document::from_text("home\n"), None);

        for _ in 0..2 {
            ctrl(&mut app, 'o');
            typ(&mut app, path.to_str().unwrap());
            press(&mut app, KeyCode::Enter);
        }

        assert_eq!(app.buffer_count(), 2); // home + ONE stash buffer
        assert_eq!(app.active_index(), 1);
    }

    #[test]
    fn a_buffer_opened_on_a_missing_path_saves_there() {
        let path = temp_path("open_stash");
        let _ = std::fs::remove_file(&path);
        let mut app = single(Document::from_text("home\n"), None);

        ctrl(&mut app, 'o');
        typ(&mut app, path.to_str().unwrap());
        press(&mut app, KeyCode::Enter);
        typ(&mut app, "fresh");
        ctrl(&mut app, 's'); // saves to the stashed path, no prompt

        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_prompt_esc_cancels_without_a_new_buffer() {
        let mut app = single(Document::from_text("home\n"), None);
        ctrl(&mut app, 'o');
        typ(&mut app, "whatever.org");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.buffer_count(), 1);
    }

    #[test]
    fn ctrl_chords_in_a_prompt_are_not_inserted() {
        let mut app = single(Document::from_text("hello"), None);
        ctrl(&mut app, 's'); // untitled → SaveAs prompt
        ctrl(&mut app, 'w'); // must NOT type a literal 'w' into the path
        assert_eq!(app.mode(), &Mode::SaveAs { input: String::new() });
    }

    #[test]
    fn buffer_list_arrows_and_enter_switch() {
        let mut app = three_buffers();
        ctrl(&mut app, 'b');
        assert_eq!(app.mode(), &Mode::BufferList { selected: 0 });
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down); // clamps at the last entry
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.active_index(), 2);
    }

    #[test]
    fn buffer_list_esc_cancels_without_switching() {
        let mut app = three_buffers();
        ctrl(&mut app, 'b');
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.active_index(), 0);
    }

    #[test]
    fn buffer_list_digit_jumps_immediately() {
        let mut app = three_buffers();
        ctrl(&mut app, 'b');
        press(&mut app, KeyCode::Char('2'));
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.active_index(), 1);
    }

    #[test]
    fn buffer_list_ignores_a_digit_beyond_the_open_buffers() {
        let mut app = three_buffers();
        ctrl(&mut app, 'b');
        press(&mut app, KeyCode::Char('9'));
        assert_eq!(app.mode(), &Mode::BufferList { selected: 0 });
        assert_eq!(app.active_index(), 0);
    }

    #[test]
    fn closing_a_clean_buffer_clamps_the_active_index() {
        let mut app = three_buffers();
        alt(&mut app, 'p'); // active = 2 (last)
        ctrl(&mut app, 'w');
        assert_eq!(app.buffer_count(), 2);
        assert_eq!(app.active_index(), 1); // stepped back, not out of bounds
        assert_eq!(app.document().text(), "second\n");
    }

    #[test]
    fn closing_a_middle_buffer_lands_on_the_next_one() {
        let mut app = three_buffers();
        alt(&mut app, 'n'); // active = 1
        ctrl(&mut app, 'w');
        assert_eq!(app.buffer_count(), 2);
        assert_eq!(app.active_index(), 1);
        assert_eq!(app.document().text(), "third\n");
    }

    #[test]
    fn closing_a_dirty_buffer_asks_and_y_discards() {
        let mut app = three_buffers();
        typ(&mut app, "x"); // dirty
        ctrl(&mut app, 'w');
        assert_eq!(app.mode(), &Mode::ConfirmClose);
        assert_eq!(app.buffer_count(), 3); // nothing closed yet
        press(&mut app, KeyCode::Char('y'));
        assert_eq!(app.mode(), &Mode::Edit);
        assert_eq!(app.buffer_count(), 2);
        assert_eq!(app.document().text(), "second\n");
    }

    #[test]
    fn close_confirmation_n_and_esc_cancel() {
        let mut app = three_buffers();
        typ(&mut app, "x"); // dirty
        for code in [KeyCode::Char('n'), KeyCode::Esc] {
            ctrl(&mut app, 'w');
            assert_eq!(app.mode(), &Mode::ConfirmClose);
            press(&mut app, code);
            assert_eq!(app.mode(), &Mode::Edit);
            assert_eq!(app.buffer_count(), 3);
        }
    }

    #[test]
    fn closing_the_last_buffer_leaves_a_fresh_untitled_one() {
        let mut app = single(Document::from_text("only\n"), None);
        ctrl(&mut app, 'w');
        assert!(!app.should_quit());
        assert_eq!(app.buffer_count(), 1);
        assert_eq!(app.document().text(), "");
        assert_eq!(app.buffer_labels()[0].0, "[No Name]");
    }

    #[test]
    fn ctrl_q_quits_immediately_when_all_buffers_are_clean() {
        let mut app = three_buffers();
        ctrl(&mut app, 'q');
        assert!(app.should_quit());
    }

    #[test]
    fn ctrl_q_with_dirty_buffers_asks_and_y_quits() {
        let mut app = three_buffers();
        typ(&mut app, "x"); // one dirty buffer
        ctrl(&mut app, 'q');
        assert_eq!(app.mode(), &Mode::ConfirmQuit);
        assert!(!app.should_quit());
        press(&mut app, KeyCode::Char('y'));
        assert!(app.should_quit());
    }

    #[test]
    fn quit_confirmation_n_and_esc_cancel() {
        let mut app = three_buffers();
        typ(&mut app, "x");
        for code in [KeyCode::Char('n'), KeyCode::Esc] {
            ctrl(&mut app, 'q');
            press(&mut app, code);
            assert_eq!(app.mode(), &Mode::Edit);
            assert!(!app.should_quit());
        }
    }

    #[test]
    fn per_buffer_cursor_and_fold_state_survive_switching() {
        let mut app = App::new(vec![
            Buffer::new(Document::from_text("* A\nbody\n"), None),
            Buffer::new(Document::from_text("plain\n"), None),
        ]);
        press(&mut app, KeyCode::Tab); // fold heading A
        press(&mut app, KeyCode::Down); // cursor off line 0... (hidden line, but view allows)
        let line_before = app.view().cursor_line();
        assert!(app.is_folded_heading(0));

        alt(&mut app, 'n'); // away...
        assert!(!app.is_folded_heading(0)); // buffer 2 has no folds
        typ(&mut app, "x"); // edit buffer 2 independently
        alt(&mut app, 'p'); // ...and back

        assert!(app.is_folded_heading(0)); // fold intact
        assert_eq!(app.view().cursor_line(), line_before); // cursor intact
        assert_eq!(app.document().text(), "* A\nbody\n"); // content untouched
    }
}
