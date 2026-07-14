//! The format-agnostic outline layer — the spine of textr's Org-mode–style structure
//! editing (see `docs/roadmap.md`).
//!
//! A buffer parses into an [`Outline`]: a flat, level-tagged list of [`Heading`]s, each
//! carrying the extent of its subtree so the frontend can fold it. **Format knowledge lives
//! behind the [`StructureProvider`] trait**, so every capability built on top — folding now;
//! promote/demote, agenda, and export later — is written once against the trait and works for
//! every format. [`OrgProvider`] (Org `*` headings) and [`MarkdownProvider`] (ATX `#`
//! headings) are the implementers; [`detect_format`] picks one from a file extension via the
//! [`Format`] enum.

use std::path::Path;

use crate::document::Document;
use crate::timestamp::{field_at, find_timestamps, parse_timestamp, shift_field, Timestamp};

/// A TODO workflow keyword on a heading. M2 ships the two canonical states; custom keyword
/// sets, priorities, and tags come later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoState {
    /// An open item (`TODO`).
    Todo,
    /// A completed item (`DONE`).
    Done,
}

/// One heading in a parsed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Nesting depth, 1-based: Org `* ` = 1, `** ` = 2.
    pub level: usize,
    /// The line the heading sits on (0-based).
    pub line: usize,
    /// The heading text, with the markers, TODO keyword, priority cookie, and trailing
    /// tags stripped.
    pub title: String,
    /// The heading's TODO state, if it carries a keyword.
    pub todo: Option<TodoState>,
    /// The `[#X]` priority cookie (A–C) after the keyword, if present.
    pub priority: Option<char>,
    /// The `:tag:` run at the end of the headline, colons stripped.
    pub tags: Vec<String>,
    /// A `SCHEDULED:` timestamp from the planning line below the heading (Org only).
    pub scheduled: Option<Timestamp>,
    /// A `DEADLINE:` timestamp from the planning line below the heading (Org only).
    pub deadline: Option<Timestamp>,
    /// The last line of this heading's subtree. The foldable body is the (possibly empty)
    /// range `line + 1 ..= last_line`.
    pub last_line: usize,
}

/// A document's headings in document order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// The headings, top to bottom.
    pub headings: Vec<Heading>,
}

impl Outline {
    /// The heading the caret is "inside": the last heading at or above `line`, if any.
    fn enclosing_index(&self, line: usize) -> Option<usize> {
        self.headings.iter().rposition(|h| h.line <= line)
    }
}

/// What a structural edit did — either the document changed (and where the cursor should
/// land) or nothing happened (and why, for the status line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOutcome {
    Changed { cursor_line: usize },
    NoOp(&'static str),
}

const NOT_ON_HEADING: &str = "Not inside a heading's subtree";

/// Which planning keyword an edit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Planning {
    Scheduled,
    Deadline,
}

/// The heading whose subtree contains `line`: the nearest heading at or above it.
fn enclosing(outline: &Outline, line: usize) -> Option<&Heading> {
    outline.headings.iter().rev().find(|h| h.line <= line)
}

/// The heading lines of `h` and every heading inside its subtree.
fn subtree_member_lines(outline: &Outline, h: &Heading) -> Vec<usize> {
    outline
        .headings
        .iter()
        .filter(|m| m.line >= h.line && m.line <= h.last_line)
        .map(|m| m.line)
        .collect()
}

/// Swap the block of lines starting at `first_top` (running to `second_top - 1`) with the
/// block `second_top..=second_last` (two adjacent line blocks). Returns the line where the
/// second block now starts (= `first_top`). Handles a missing trailing newline at EOF.
fn swap_blocks(doc: &mut Document, first_top: usize, second_top: usize, second_last: usize) -> usize {
    let start_a = doc.line_to_char(first_top);
    let start_b = doc.line_to_char(second_top);
    let end_b = if second_last + 1 < doc.line_count() {
        doc.line_to_char(second_last + 1)
    } else {
        doc.text().chars().count()
    };
    let text = doc.text();
    let mut a: String = text.chars().skip(start_a).take(start_b - start_a).collect();
    let mut b: String = text.chars().skip(start_b).take(end_b - start_b).collect();
    if !b.ends_with('\n') {
        b.push('\n');
        if a.ends_with('\n') {
            a.pop(); // keep the total newline count identical
        }
    }
    doc.remove(start_a..end_b);
    doc.insert(start_a, &(b + &a));
    first_top
}

/// A parser + editor of a particular document format's structure. One implementer per
/// format (Org and Markdown today). Everything else in textr talks to structure through
/// this trait, never to a concrete format — the structural-edit operations below are
/// default methods, written once over `parse` + the two format primitives.
pub trait StructureProvider {
    /// Scan `doc` into an [`Outline`].
    fn parse(&self, doc: &Document) -> Outline;

    /// Cycle the TODO keyword on the heading at `line`: none → `TODO` → `DONE` → none.
    /// A no-op if that line is not a heading.
    fn cycle_todo(&self, doc: &mut Document, line: usize);

    /// The heading marker byte (`*` for Org, `#` for Markdown).
    fn marker(&self) -> u8;

    /// The deepest legal heading level, if the format has one.
    fn max_level(&self) -> Option<usize>;

    /// Promote the enclosing heading one level (children keep theirs).
    fn promote_heading(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if h.level == 1 {
            return EditOutcome::NoOp("Already at top level");
        }
        let start = doc.line_to_char(h.line);
        doc.remove(start..start + 1);
        EditOutcome::Changed { cursor_line: line }
    }

    /// Demote the enclosing heading one level (children keep theirs).
    fn demote_heading(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if Some(h.level) == self.max_level() {
            return EditOutcome::NoOp("Markdown headings stop at level 6");
        }
        let start = doc.line_to_char(h.line);
        doc.insert(start, &(self.marker() as char).to_string());
        EditOutcome::Changed { cursor_line: line }
    }

    /// Promote the enclosing heading and every heading in its subtree.
    fn promote_subtree(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if h.level == 1 {
            return EditOutcome::NoOp("Already at top level");
        }
        for l in subtree_member_lines(&outline, h) {
            let start = doc.line_to_char(l);
            doc.remove(start..start + 1);
        }
        EditOutcome::Changed { cursor_line: line }
    }

    /// Demote the enclosing heading and every heading in its subtree.
    fn demote_subtree(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        if let Some(max) = self.max_level() {
            let too_deep = outline
                .headings
                .iter()
                .any(|m| m.line >= h.line && m.line <= h.last_line && m.level == max);
            if too_deep {
                return EditOutcome::NoOp("Markdown headings stop at level 6");
            }
        }
        let m = (self.marker() as char).to_string();
        for l in subtree_member_lines(&outline, h) {
            let start = doc.line_to_char(l);
            doc.insert(start, &m);
        }
        EditOutcome::Changed { cursor_line: line }
    }

    /// Swap the enclosing subtree with the previous same-level sibling subtree.
    fn move_subtree_up(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        // The previous sibling is the heading whose subtree ends right above ours.
        let Some(prev) = outline
            .headings
            .iter()
            .find(|p| p.level == h.level && p.last_line + 1 == h.line)
        else {
            return EditOutcome::NoOp("No previous sibling at this level");
        };
        let offset = line - h.line;
        let new_top = swap_blocks(doc, prev.line, h.line, h.last_line);
        EditOutcome::Changed { cursor_line: new_top + offset }
    }

    /// Swap the enclosing subtree with the next same-level sibling subtree.
    fn move_subtree_down(&self, doc: &mut Document, line: usize) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let Some(next) = outline
            .headings
            .iter()
            .find(|n| n.line == h.last_line + 1 && n.level == h.level)
        else {
            return EditOutcome::NoOp("No next sibling at this level");
        };
        let next_len = next.last_line - next.line + 1;
        swap_blocks(doc, h.line, next.line, next.last_line);
        EditOutcome::Changed { cursor_line: line + next_len }
    }

    /// Insert a new (possibly `TODO`) sibling heading after the enclosing subtree; with no
    /// enclosing heading, append a level-1 heading at the end of the document.
    fn insert_sibling(&self, doc: &mut Document, line: usize, todo: bool) -> EditOutcome {
        let outline = self.parse(doc);
        let (level, target_line) = match enclosing(&outline, line) {
            Some(h) => (h.level, h.last_line + 1),
            None => (1, doc.line_count()),
        };
        let markers = (self.marker() as char).to_string().repeat(level);
        let keyword = if todo { "TODO " } else { "" };
        let (at, prefix) = if target_line < doc.line_count() {
            (doc.line_to_char(target_line), "")
        } else {
            let end = doc.text().chars().count();
            let needs_nl = end > 0 && !doc.text().ends_with('\n');
            (end, if needs_nl { "\n" } else { "" })
        };
        doc.insert(at, &format!("{prefix}{markers} {keyword}\n"));
        let cursor_line = doc.char_to_line(at + prefix.chars().count());
        EditOutcome::Changed { cursor_line }
    }

    /// Cycle the `[#X]` cookie on the enclosing heading: up = none→C→B→A, down = A→B→C→none.
    fn cycle_priority(&self, doc: &mut Document, line: usize, up: bool) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let new = match (h.priority, up) {
            (None, true) => Some('C'),
            (Some('C'), true) => Some('B'),
            (Some('B'), true) => Some('A'),
            (Some(_), true) => return EditOutcome::NoOp("Already at highest priority"),
            (None, false) => return EditOutcome::NoOp("No priority to lower"),
            (Some('A'), false) => Some('B'),
            (Some('B'), false) => Some('C'),
            (Some(_), false) => None,
        };
        // Everything before the cookie is ASCII (markers, spaces, keyword), so byte
        // offsets equal char offsets.
        let raw = doc.line_text(h.line);
        let text = raw.strip_suffix('\n').unwrap_or(&raw);
        let after_markers = &text[h.level..];
        let mut pos = h.level + (after_markers.len() - after_markers.trim_start().len());
        let rest = after_markers.trim_start();
        if split_todo_keyword(rest).0.is_some() {
            let after_kw = &rest[4..]; // TODO and DONE are both 4 ASCII bytes
            pos += 4 + (after_kw.len() - after_kw.trim_start().len());
        }
        let start = doc.line_to_char(h.line) + pos;
        // A cookie is 4 chars: `[#X]`.
        match (h.priority, new) {
            (None, Some(p)) => doc.insert(start, &format!("[#{p}] ")),
            (Some(_), Some(p)) => {
                doc.remove(start..start + 4);
                doc.insert(start, &format!("[#{p}]"));
            }
            (Some(_), None) => {
                let followed_by_space = text.get(pos + 4..).is_some_and(|t| t.starts_with(' '));
                doc.remove(start..start + if followed_by_space { 5 } else { 4 });
            }
            (None, None) => unreachable!(),
        }
        EditOutcome::Changed { cursor_line: line }
    }

    /// Replace the enclosing heading's trailing tag run with `tags` (empty = remove).
    /// Tags must already satisfy [`is_valid_tag`].
    fn set_tags(&self, doc: &mut Document, line: usize, tags: &[String]) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let raw = doc.line_text(h.line);
        let text = raw.strip_suffix('\n').unwrap_or(&raw).to_string();
        let prefix = match tag_run_start(&text) {
            Some(i) => text[..i].trim_end(),
            None => text.trim_end(),
        };
        let mut new_line = prefix.to_string();
        if !tags.is_empty() {
            new_line.push_str(&format!(" :{}:", tags.join(":")));
        }
        let start = doc.line_to_char(h.line);
        doc.remove(start..start + text.chars().count());
        doc.insert(start, &new_line);
        EditOutcome::Changed { cursor_line: line }
    }

    /// Set, replace, or remove (`ts = None`) the enclosing heading's `SCHEDULED`/`DEADLINE`
    /// timestamp on the planning line directly below it. `SCHEDULED` is written before
    /// `DEADLINE`; an empty planning line is removed entirely.
    fn set_planning(
        &self,
        doc: &mut Document,
        line: usize,
        which: Planning,
        ts: Option<Timestamp>,
    ) -> EditOutcome {
        let outline = self.parse(doc);
        let Some(h) = enclosing(&outline, line) else {
            return EditOutcome::NoOp(NOT_ON_HEADING);
        };
        let (hline, level) = (h.line, h.level);
        let below = doc.line_text(hline + 1);
        let below_trim = below.trim_start();
        let is_planning =
            below_trim.starts_with("SCHEDULED:") || below_trim.starts_with("DEADLINE:");
        let (mut scheduled, mut deadline) = if is_planning {
            parse_planning(&below)
        } else {
            (None, None)
        };
        match which {
            Planning::Scheduled => scheduled = ts,
            Planning::Deadline => deadline = ts,
        }
        let mut parts: Vec<String> = Vec::new();
        if let Some(s) = scheduled {
            parts.push(format!("SCHEDULED: {s}"));
        }
        if let Some(d) = deadline {
            parts.push(format!("DEADLINE: {d}"));
        }
        let start = doc.line_to_char(hline + 1);
        if is_planning {
            doc.remove(start..start + below.chars().count());
        }
        if !parts.is_empty() {
            let indent = " ".repeat(level + 1);
            doc.insert(start, &format!("{indent}{}\n", parts.join(" ")));
        }
        EditOutcome::Changed { cursor_line: line }
    }
}

/// Whether `tag` is a legal tag name (non-empty, only letters/digits/`_@#%`).
pub fn is_valid_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.chars().all(is_tag_char)
}

/// Shift the timestamp field under the cursor (`line`, char column `col`) by one step,
/// rewriting it in place. Returns `None` when the cursor is not inside a timestamp, so the
/// caller can fall through to another meaning of the key (e.g. priority cycling). Timestamps
/// are ASCII, so char columns index them directly.
pub fn shift_timestamp(
    doc: &mut Document,
    line: usize,
    col: usize,
    up: bool,
) -> Option<EditOutcome> {
    let raw = doc.line_text(line);
    let text = raw.strip_suffix('\n').unwrap_or(&raw);
    let (start, end) = find_timestamps(text)
        .into_iter()
        .find(|&(s, e)| col >= s && col < e)?;
    let tsraw = &text[start..end];
    let field = field_at(tsraw, col - start)?;
    // A range's second stamp: shift it instead when the cursor is past the `--`.
    let (ts, _) = parse_timestamp(tsraw)?;
    let sep = tsraw.find("--");
    let in_end = matches!(sep, Some(i) if col - start > i);
    let new_ts = if in_end {
        let end_stamp = shift_field(ts.end?, field, up);
        Timestamp { start: ts.start, end: Some(end_stamp) }
    } else {
        Timestamp { start: shift_field(ts.start, field, up), end: ts.end }
    };
    let base = doc.line_to_char(line);
    doc.remove(base + start..base + end);
    doc.insert(base + start, &new_ts.to_string());
    Some(EditOutcome::Changed { cursor_line: line })
}

// ---- navigation (pure free functions over an Outline) ---------------------

/// The line of the first heading strictly below `line`, if any.
pub fn next_heading(outline: &Outline, line: usize) -> Option<usize> {
    outline.headings.iter().find(|h| h.line > line).map(|h| h.line)
}

/// The line of the last heading strictly above `line`, if any.
pub fn prev_heading(outline: &Outline, line: usize) -> Option<usize> {
    outline
        .headings
        .iter()
        .rev()
        .find(|h| h.line < line)
        .map(|h| h.line)
}

/// The line of the parent of the heading enclosing `line` — the nearest preceding heading
/// of a smaller level. `None` at the top level or outside any heading.
pub fn parent_heading(outline: &Outline, line: usize) -> Option<usize> {
    let idx = outline.enclosing_index(line)?;
    let level = outline.headings[idx].level;
    outline.headings[..idx]
        .iter()
        .rev()
        .find(|h| h.level < level)
        .map(|h| h.line)
}

// ---- Org format -----------------------------------------------------------

/// The Org-syntax structure provider: headings are `*`-prefixed lines
/// (`^(\*+) +(?:(TODO|DONE) +)?title`).
pub struct OrgProvider;

impl StructureProvider for OrgProvider {
    fn parse(&self, doc: &Document) -> Outline {
        let mut headings: Vec<Heading> = (0..doc.line_count())
            .filter_map(|line| parse_org_heading(&doc.line_text(line), line))
            .collect();
        fill_subtree_extents(&mut headings, last_content_line(doc));
        // The planning line, if any, sits directly below the heading.
        for h in &mut headings {
            let (scheduled, deadline) = parse_planning(&doc.line_text(h.line + 1));
            h.scheduled = scheduled;
            h.deadline = deadline;
        }
        Outline { headings }
    }

    fn cycle_todo(&self, doc: &mut Document, line: usize) {
        // An Org heading is fully determined by its own line, so a line-local check suffices.
        let raw = doc.line_text(line);
        let Some(heading) = parse_org_heading(&raw, line) else {
            return; // not a heading — leave the buffer untouched
        };
        cycle_keyword(doc, line, heading.level);
    }

    fn marker(&self) -> u8 {
        b'*'
    }

    fn max_level(&self) -> Option<usize> {
        None
    }
}

/// The last line holding content. A text ending in `\n` has a phantom empty final line in
/// the rope's count; subtree extents (and the cursor math built on them) must not include it.
fn last_content_line(doc: &Document) -> usize {
    let last = doc.line_count().saturating_sub(1);
    if last > 0 && doc.line_text(last).is_empty() {
        last - 1
    } else {
        last
    }
}

/// Fill in each heading's subtree extent now that its successors are known: a subtree runs
/// until the next heading of equal-or-shallower level, else to `last_doc_line`.
fn fill_subtree_extents(headings: &mut [Heading], last_doc_line: usize) {
    for i in 0..headings.len() {
        let level = headings[i].level;
        let end = headings[i + 1..]
            .iter()
            .find(|h| h.level <= level)
            .map(|h| h.line - 1)
            .unwrap_or(last_doc_line);
        headings[i].last_line = end;
    }
}

/// Rotate the TODO keyword (none → `TODO` → `DONE` → none) on the heading line whose marker
/// run (`*` stars or `#` hashes — ASCII, so byte count == char count == level) is
/// `marker_len` bytes long. The caller has already established that `line` is a heading.
fn cycle_keyword(doc: &mut Document, line: usize, marker_len: usize) {
    let raw = doc.line_text(line);
    let text = raw.strip_suffix('\n').unwrap_or(&raw);
    let after_markers = &text[marker_len..];
    let spaces = after_markers.len() - after_markers.trim_start().len();
    let rest_start = doc.line_to_char(line) + marker_len + spaces;
    let rest = after_markers.trim_start();

    match split_todo_keyword(rest).0 {
        None => doc.insert(rest_start, "TODO "),
        Some(TodoState::Todo) => {
            doc.remove(rest_start..rest_start + "TODO".len());
            doc.insert(rest_start, "DONE");
        }
        Some(TodoState::Done) => {
            // Drop "DONE" plus the one space before the title, if there is a title.
            let len = if rest == "DONE" { 4 } else { 5 };
            doc.remove(rest_start..rest_start + len);
        }
    }
}

/// Parse a single raw line (newline included) into a heading, or `None` if it isn't one.
/// `last_line` is left as `line` and filled in later by the caller.
fn parse_org_heading(raw: &str, line: usize) -> Option<Heading> {
    let text = raw.strip_suffix('\n').unwrap_or(raw);
    let level = text.bytes().take_while(|&b| b == b'*').count();
    if level == 0 {
        return None; // no stars, or a '*' not at column 0
    }
    let after = &text[level..];
    if !after.starts_with(' ') {
        return None; // "*bold" — stars must be followed by a space
    }
    let rest = after.trim_start();
    if rest.is_empty() {
        return None; // "* " with no title is body, not a heading
    }
    let (todo, priority, title, tags) = parse_headline_rest(rest);
    Some(Heading {
        level,
        line,
        title,
        todo,
        priority,
        tags,
        scheduled: None,
        deadline: None,
        last_line: line,
    })
}

/// Parse a planning line (`SCHEDULED: <…>  DEADLINE: <…>`, either or both, any order, any
/// leading whitespace) into its two timestamps. Returns `(None, None)` if `text` is not a
/// planning line.
fn parse_planning(text: &str) -> (Option<Timestamp>, Option<Timestamp>) {
    let trimmed = text.trim_start();
    let mut scheduled = None;
    let mut deadline = None;
    for token in ["SCHEDULED:", "DEADLINE:"] {
        if let Some(pos) = trimmed.find(token) {
            let after = trimmed[pos + token.len()..].trim_start();
            if let Some((ts, _)) = parse_timestamp(after) {
                if token == "SCHEDULED:" {
                    scheduled = Some(ts);
                } else {
                    deadline = Some(ts);
                }
            }
        }
    }
    (scheduled, deadline)
}

// ---- format detection --------------------------------------------------------

/// A document format torg understands, dispatching to the matching provider. This is the
/// closed set of built-in formats; everything above it still talks to [`StructureProvider`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// Org (`*` headings) — also the default for unknown and untitled buffers.
    #[default]
    Org,
    /// Markdown (ATX `#` headings).
    Markdown,
}

impl StructureProvider for Format {
    fn parse(&self, doc: &Document) -> Outline {
        match self {
            Format::Org => OrgProvider.parse(doc),
            Format::Markdown => MarkdownProvider.parse(doc),
        }
    }

    fn cycle_todo(&self, doc: &mut Document, line: usize) {
        match self {
            Format::Org => OrgProvider.cycle_todo(doc, line),
            Format::Markdown => MarkdownProvider.cycle_todo(doc, line),
        }
    }

    fn marker(&self) -> u8 {
        match self {
            Format::Org => OrgProvider.marker(),
            Format::Markdown => MarkdownProvider.marker(),
        }
    }

    fn max_level(&self) -> Option<usize> {
        match self {
            Format::Org => OrgProvider.max_level(),
            Format::Markdown => MarkdownProvider.max_level(),
        }
    }
}

/// The format for a file path: `.md`/`.markdown` (ASCII-case-insensitive) is Markdown;
/// everything else — including no extension and no path at all — is Org.
pub fn detect_format(path: Option<&Path>) -> Format {
    let is_markdown = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
    if is_markdown {
        Format::Markdown
    } else {
        Format::Org
    }
}

// ---- Markdown format --------------------------------------------------------

/// The Markdown-syntax structure provider: ATX headings — `#{1,6}` at column 0, a required
/// space, and a non-empty title (`^(#{1,6}) +(?:(TODO|DONE) +)?title`). Setext (underline)
/// headings are not recognized, and closing hash runs (`## title ##`) stay in the title.
pub struct MarkdownProvider;

impl StructureProvider for MarkdownProvider {
    fn parse(&self, doc: &Document) -> Outline {
        // Unlike Org, the scan is stateful: a line only counts as a heading outside fenced
        // code blocks (```/~~~), where a column-0 `# comment` is code, not structure.
        let mut headings: Vec<Heading> = Vec::new();
        let mut fence: Option<(u8, usize)> = None; // open fence: (fence byte, opener run length)
        for line in 0..doc.line_count() {
            let raw = doc.line_text(line);
            let text = raw.strip_suffix('\n').unwrap_or(&raw);
            match fence {
                Some((ch, len)) => {
                    if is_fence_closer(text, ch, len) {
                        fence = None;
                    }
                }
                None => {
                    if let Some(opened) = fence_opener(text) {
                        fence = Some(opened);
                    } else if let Some(h) = parse_md_heading(&raw, line) {
                        headings.push(h);
                    }
                }
            }
        }
        fill_subtree_extents(&mut headings, last_content_line(doc));
        Outline { headings }
    }

    fn cycle_todo(&self, doc: &mut Document, line: usize) {
        let Some(heading) = self
            .parse(doc)
            .headings
            .into_iter()
            .find(|h| h.line == line)
        else {
            return; // not a heading — leave the buffer untouched
        };
        cycle_keyword(doc, line, heading.level);
    }

    fn marker(&self) -> u8 {
        b'#'
    }

    fn max_level(&self) -> Option<usize> {
        Some(6)
    }
}

/// The fence run at the start of `text` after at most 3 leading spaces: the fence byte and
/// its run length, if the run is at least 3 characters of `` ` `` or `~`.
fn fence_run(text: &str) -> Option<(u8, usize, &str)> {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 {
        return None; // 4+ spaces is an indented code line, not a fence
    }
    let body = &text[indent..];
    let ch = *body.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let run = body.bytes().take_while(|&b| b == ch).count();
    if run < 3 {
        return None;
    }
    Some((ch, run, &body[run..]))
}

/// Whether `text` opens a fenced code block, and with what fence. A backtick fence's info
/// string must not itself contain a backtick (that's an inline-code line, not a fence).
fn fence_opener(text: &str) -> Option<(u8, usize)> {
    let (ch, run, rest) = fence_run(text)?;
    if ch == b'`' && rest.contains('`') {
        return None;
    }
    Some((ch, run))
}

/// Whether `text` closes the open fence `(ch, len)`: the same fence character, a run at
/// least as long as the opener's, and nothing but whitespace after it.
fn is_fence_closer(text: &str, ch: u8, len: usize) -> bool {
    matches!(fence_run(text), Some((c, run, rest)) if c == ch && run >= len && rest.trim().is_empty())
}

/// Parse a single raw line (newline included) into an ATX heading, or `None` if it isn't one.
/// `last_line` is left as `line` and filled in later by the caller.
fn parse_md_heading(raw: &str, line: usize) -> Option<Heading> {
    let text = raw.strip_suffix('\n').unwrap_or(raw);
    let level = text.bytes().take_while(|&b| b == b'#').count();
    if level == 0 || level > 6 {
        return None; // no hashes at column 0, or beyond ATX's six levels
    }
    let after = &text[level..];
    if !after.starts_with(' ') {
        return None; // "#no-space" — hashes must be followed by a space
    }
    let rest = after.trim_start();
    if rest.is_empty() {
        return None; // "# " with no title is body, not a heading
    }
    let (todo, priority, title, tags) = parse_headline_rest(rest);
    Some(Heading {
        level,
        line,
        title,
        todo,
        priority,
        tags,
        scheduled: None,
        deadline: None,
        last_line: line,
    })
}

/// Split keyword, priority cookie, title, and trailing tags off a headline's text
/// (everything after the markers and their space).
fn parse_headline_rest(rest: &str) -> (Option<TodoState>, Option<char>, String, Vec<String>) {
    let (todo, rest) = split_todo_keyword(rest);
    let (priority, rest) = split_priority(rest);
    let (title, tags) = split_tags(rest);
    (todo, priority, title, tags)
}

/// Split a leading `[#A]`/`[#B]`/`[#C]` cookie. It must be followed by a space or the end
/// of the line; anything else stays in the title.
fn split_priority(rest: &str) -> (Option<char>, &str) {
    let Some(tail) = rest.strip_prefix("[#") else {
        return (None, rest);
    };
    let bytes = tail.as_bytes();
    match (bytes.first(), bytes.get(1)) {
        (Some(p @ b'A'..=b'C'), Some(b']')) => {
            let after = &tail[2..];
            if after.is_empty() {
                (Some(*p as char), "")
            } else if let Some(t) = after.strip_prefix(' ') {
                (Some(*p as char), t.trim_start())
            } else {
                (None, rest)
            }
        }
        _ => (None, rest),
    }
}

/// A character allowed inside a tag: letters, digits, `_ @ # %` (the Org manual's rule).
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// Whether `s` is a complete tag run like `:a:b:`.
fn is_tag_run(s: &str) -> bool {
    s.len() >= 3
        && s.starts_with(':')
        && s.ends_with(':')
        && s[1..s.len() - 1]
            .split(':')
            .all(|t| !t.is_empty() && t.chars().all(is_tag_char))
}

/// Byte index where a trailing tag run starts in `text` (after trailing-whitespace trim),
/// or `None` if the line doesn't end in one.
fn tag_run_start(text: &str) -> Option<usize> {
    let trimmed = text.trim_end();
    if !trimmed.ends_with(':') {
        return None;
    }
    let start = trimmed
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    is_tag_run(&trimmed[start..]).then_some(start)
}

/// Split a trailing `:tag:` run off the title.
fn split_tags(rest: &str) -> (String, Vec<String>) {
    match tag_run_start(rest) {
        Some(start) => {
            let tags = rest.trim_end()[start..]
                .trim_matches(':')
                .split(':')
                .map(str::to_string)
                .collect();
            (rest[..start].trim_end().to_string(), tags)
        }
        None => (rest.trim_end().to_string(), Vec::new()),
    }
}

/// Split a leading `TODO`/`DONE` keyword off the heading text. The keyword only counts as
/// the first whole word — `TODOitem` is a plain title.
fn split_todo_keyword(rest: &str) -> (Option<TodoState>, &str) {
    for (state, word) in [(TodoState::Todo, "TODO"), (TodoState::Done, "DONE")] {
        if let Some(tail) = rest.strip_prefix(word) {
            if tail.is_empty() || tail.starts_with(' ') {
                return (Some(state), tail.trim_start());
            }
        }
    }
    (None, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(text: &str) -> Outline {
        OrgProvider.parse(&Document::from_text(text))
    }

    // ---- B1: parsing ------------------------------------------------------

    #[test]
    fn parses_levels_and_titles() {
        let o = outline("* A\n** B\n* C\n");
        let levels: Vec<_> = o.headings.iter().map(|h| h.level).collect();
        let titles: Vec<_> = o.headings.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(levels, [1, 2, 1]);
        assert_eq!(titles, ["A", "B", "C"]);
        assert!(o.headings.iter().all(|h| h.todo.is_none()));
    }

    #[test]
    fn parses_todo_and_done_keywords() {
        assert_eq!(outline("* TODO write\n").headings[0].todo, Some(TodoState::Todo));
        assert_eq!(outline("* TODO write\n").headings[0].title, "write");
        assert_eq!(outline("* DONE ship").headings[0].todo, Some(TodoState::Done));
        assert_eq!(outline("* DONE ship").headings[0].title, "ship");
    }

    #[test]
    fn non_headings_are_body() {
        assert!(outline("* \n").headings.is_empty()); // no title
        assert!(outline("*not a heading\n").headings.is_empty()); // no space after stars
        assert!(outline("  * indented\n").headings.is_empty()); // star not at column 0
        assert!(outline("TODOitem is prose\n").headings.is_empty());
        assert!(outline("").headings.is_empty()); // empty buffer
    }

    // ---- B2: subtree extent + navigation ----------------------------------

    #[test]
    fn subtree_extent_stops_at_the_next_equal_or_shallower_heading() {
        let o = outline("* A\ntext\n** B\n* C");
        // A(line0) owns through line 2; B(line2) has no body; C(line3) runs to the end.
        assert_eq!(o.headings[0].last_line, 2); // A
        assert_eq!(o.headings[1].last_line, 2); // B (line 2 only)
        assert_eq!(o.headings[2].last_line, 3); // C
    }

    #[test]
    fn next_and_prev_heading_skip_to_adjacent_headings() {
        let o = outline("* A\ntext\n** B\n* C");
        assert_eq!(next_heading(&o, 0), Some(2)); // A → B
        assert_eq!(next_heading(&o, 2), Some(3)); // B → C
        assert_eq!(next_heading(&o, 3), None); // past the last heading
        assert_eq!(prev_heading(&o, 3), Some(2)); // C → B
        assert_eq!(prev_heading(&o, 0), None); // before the first heading
    }

    #[test]
    fn parent_heading_climbs_one_level() {
        let o = outline("* A\ntext\n** B\n* C");
        assert_eq!(parent_heading(&o, 2), Some(0)); // B (level 2) → A (level 1)
        assert_eq!(parent_heading(&o, 0), None); // A is top level
        assert_eq!(parent_heading(&o, 3), None); // C is top level
    }

    // ---- B3: cycle_todo ---------------------------------------------------

    #[test]
    fn cycle_todo_rotates_none_todo_done() {
        let mut doc = Document::from_text("* task\n");
        OrgProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "* TODO task\n");
        OrgProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "* DONE task\n");
        OrgProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "* task\n");
        assert!(doc.is_modified());
    }

    #[test]
    fn cycle_todo_on_a_non_heading_is_a_noop() {
        let mut doc = Document::from_text("just prose\n");
        OrgProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "just prose\n");
        assert!(!doc.is_modified()); // nothing changed, so the buffer stays clean
    }

    #[test]
    fn cycle_todo_preserves_nesting_level() {
        let mut doc = Document::from_text("** deep\n");
        OrgProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "** TODO deep\n");
    }

    // ---- Markdown: parsing --------------------------------------------------

    fn md_outline(text: &str) -> Outline {
        MarkdownProvider.parse(&Document::from_text(text))
    }

    #[test]
    fn md_parses_levels_and_titles() {
        let o = md_outline("# A\n## B\n# C\n");
        let levels: Vec<_> = o.headings.iter().map(|h| h.level).collect();
        let titles: Vec<_> = o.headings.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(levels, [1, 2, 1]);
        assert_eq!(titles, ["A", "B", "C"]);
        assert!(o.headings.iter().all(|h| h.todo.is_none()));
    }

    #[test]
    fn md_parses_todo_and_done_keywords() {
        assert_eq!(md_outline("# TODO write\n").headings[0].todo, Some(TodoState::Todo));
        assert_eq!(md_outline("# TODO write\n").headings[0].title, "write");
        assert_eq!(md_outline("## DONE ship").headings[0].todo, Some(TodoState::Done));
        assert_eq!(md_outline("## DONE ship").headings[0].title, "ship");
    }

    #[test]
    fn md_non_headings_are_body() {
        assert!(md_outline("#no space\n").headings.is_empty());
        assert!(md_outline("# \n").headings.is_empty()); // no title
        assert!(md_outline("#\n").headings.is_empty()); // bare hash
        assert!(md_outline("  # indented\n").headings.is_empty()); // hash not at column 0
        assert!(md_outline("TODOitem is prose\n").headings.is_empty());
        assert!(md_outline("").headings.is_empty()); // empty buffer
    }

    #[test]
    fn md_seven_hashes_is_body_six_is_a_heading() {
        assert_eq!(md_outline("###### six\n").headings[0].level, 6);
        assert!(md_outline("####### seven\n").headings.is_empty());
    }

    #[test]
    fn md_subtree_extent_stops_at_the_next_equal_or_shallower_heading() {
        let o = md_outline("# A\ntext\n## B\n# C");
        assert_eq!(o.headings[0].last_line, 2); // A owns through B's subtree
        assert_eq!(o.headings[1].last_line, 2); // B (line 2 only)
        assert_eq!(o.headings[2].last_line, 3); // C runs to the end
    }

    // ---- Markdown: fenced code blocks ---------------------------------------

    #[test]
    fn md_headings_inside_backtick_fences_are_ignored() {
        let o = md_outline("# A\n```sh\n# comment\n```\n# B\n");
        let lines: Vec<_> = o.headings.iter().map(|h| h.line).collect();
        assert_eq!(lines, [0, 4]); // the fenced "# comment" is body
        assert_eq!(o.headings[0].last_line, 3); // A's subtree spans the whole fence
    }

    #[test]
    fn md_tilde_fences_also_hide_headings() {
        let o = md_outline("~~~\n# hidden\n~~~\n# real\n");
        let lines: Vec<_> = o.headings.iter().map(|h| h.line).collect();
        assert_eq!(lines, [3]);
    }

    #[test]
    fn md_unclosed_fence_runs_to_eof() {
        assert!(md_outline("```\n# a\n# b\n").headings.is_empty());
    }

    #[test]
    fn md_fence_close_requires_same_char_and_sufficient_length() {
        // ```` (4) is not closed by ``` (3) or ~~~; only by a run >= 4 of backticks.
        let o = md_outline("````\n# x\n```\n~~~\n# y\n`````\n# z\n");
        let lines: Vec<_> = o.headings.iter().map(|h| h.line).collect();
        assert_eq!(lines, [6]); // only "# z", after the ````` closer
        // An info string is fine on the opener; trailing text invalidates a closer.
        let o = md_outline("```rust\n# in code\n``` not a closer\n```\n# out\n");
        let lines: Vec<_> = o.headings.iter().map(|h| h.line).collect();
        assert_eq!(lines, [4]);
        // A backtick fence whose info string contains a backtick is not an opener.
        let o = md_outline("```a```\n# heading\n");
        let lines: Vec<_> = o.headings.iter().map(|h| h.line).collect();
        assert_eq!(lines, [1]);
    }

    #[test]
    fn md_fence_opener_tolerates_up_to_three_leading_spaces() {
        assert!(md_outline("   ```\n# hidden\n```\n").headings.is_empty());
        // Four leading spaces is an indented code line, not a fence opener.
        let o = md_outline("    ```\n# visible\n");
        let lines: Vec<_> = o.headings.iter().map(|h| h.line).collect();
        assert_eq!(lines, [1]);
    }

    // ---- Markdown: cycle_todo ------------------------------------------------

    #[test]
    fn md_cycle_todo_rotates_none_todo_done() {
        let mut doc = Document::from_text("# task\n");
        MarkdownProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "# TODO task\n");
        MarkdownProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "# DONE task\n");
        MarkdownProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "# task\n");
        assert!(doc.is_modified());
    }

    #[test]
    fn md_cycle_todo_is_a_noop_on_non_headings_and_inside_fences() {
        let mut doc = Document::from_text("just prose\n");
        MarkdownProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "just prose\n");
        assert!(!doc.is_modified());

        // The fence-awareness regression test: a line-local check would mangle this code.
        let mut doc = Document::from_text("```sh\n# comment\n```\n");
        MarkdownProvider.cycle_todo(&mut doc, 1);
        assert_eq!(doc.text(), "```sh\n# comment\n```\n");
        assert!(!doc.is_modified());
    }

    #[test]
    fn md_cycle_todo_preserves_level() {
        let mut doc = Document::from_text("### deep\n");
        MarkdownProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "### TODO deep\n");
    }

    #[test]
    fn md_cycle_todo_done_with_no_title_removes_the_bare_keyword() {
        let mut doc = Document::from_text("# DONE\n");
        MarkdownProvider.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "# \n");
    }

    // ---- headline metadata: priorities and tags -------------------------------

    #[test]
    fn parses_priority_cookie_after_the_keyword() {
        let o = outline("* TODO [#A] write\n");
        let h = &o.headings[0];
        assert_eq!(h.todo, Some(TodoState::Todo));
        assert_eq!(h.priority, Some('A'));
        assert_eq!(h.title, "write");
        let o = outline("* [#B] plain\n"); // cookie without keyword
        assert_eq!(o.headings[0].priority, Some('B'));
        assert_eq!(o.headings[0].title, "plain");
    }

    #[test]
    fn a_malformed_cookie_stays_in_the_title() {
        assert_eq!(outline("* [#D] x\n").headings[0].priority, None); // only A-C
        assert_eq!(outline("* [#D] x\n").headings[0].title, "[#D] x");
        assert_eq!(outline("* [#A]x\n").headings[0].priority, None); // no space after
    }

    #[test]
    fn parses_trailing_tags_out_of_the_title() {
        let o = outline("* TODO [#A] fix bug :work:urgent:\n");
        assert_eq!(o.headings[0].tags, vec!["work", "urgent"]);
        assert_eq!(o.headings[0].title, "fix bug");
        assert!(outline("* plain title\n").headings[0].tags.is_empty());
    }

    #[test]
    fn tag_run_must_be_whole_valid_and_trailing() {
        assert!(outline("* a :not a tag:\n").headings[0].tags.is_empty()); // space inside
        assert!(outline("* a :b: c\n").headings[0].tags.is_empty()); // not trailing
        assert_eq!(
            outline("* x :a_1:@b:#c:%d:\n").headings[0].tags,
            vec!["a_1", "@b", "#c", "%d"]
        );
    }

    #[test]
    fn markdown_headlines_share_the_metadata_chain() {
        let o = md_outline("## DONE [#C] ship :rel:\n");
        let h = &o.headings[0];
        assert_eq!(h.todo, Some(TodoState::Done));
        assert_eq!(h.priority, Some('C'));
        assert_eq!(h.title, "ship");
        assert_eq!(h.tags, vec!["rel"]);
    }

    // ---- planning lines ---------------------------------------------------------

    #[test]
    fn parses_a_scheduled_planning_line_below_the_heading() {
        let o = outline("* Task\nSCHEDULED: <2024-01-15 Mon>\nbody\n");
        let h = &o.headings[0];
        assert_eq!(h.scheduled.map(|t| t.to_string()), Some("<2024-01-15 Mon>".to_string()));
        assert!(h.deadline.is_none());
    }

    #[test]
    fn parses_scheduled_and_deadline_on_one_indented_line_either_order() {
        let o = outline("* T\n  DEADLINE: <2024-02-01> SCHEDULED: <2024-01-15>\n");
        let h = &o.headings[0];
        assert_eq!(h.scheduled.map(|t| t.to_string()), Some("<2024-01-15 Mon>".to_string()));
        assert_eq!(h.deadline.map(|t| t.to_string()), Some("<2024-02-01 Thu>".to_string()));
    }

    #[test]
    fn a_heading_with_no_planning_line_has_none() {
        let o = outline("* Task\njust body\n");
        assert!(o.headings[0].scheduled.is_none());
        assert!(o.headings[0].deadline.is_none());
    }

    #[test]
    fn markdown_headings_never_carry_planning() {
        let o = md_outline("# Task\nSCHEDULED: <2024-01-15>\n");
        assert!(o.headings[0].scheduled.is_none());
        assert!(o.headings[0].deadline.is_none());
    }

    // ---- date shifting under the cursor -----------------------------------------

    /// Shift the timestamp at `col` on line 0 of a one-line doc and return the new text.
    fn shifted(text: &str, col: usize, up: bool) -> Option<String> {
        let mut doc = Document::from_text(text);
        shift_timestamp(&mut doc, 0, col, up).map(|_| doc.text())
    }

    #[test]
    fn shift_day_up_rolls_over_and_updates_weekday() {
        // "<2024-01-31 Wed>": the day digits sit at columns 9-10.
        assert_eq!(
            shifted("<2024-01-31 Wed>", 9, true).as_deref(),
            Some("<2024-02-01 Thu>")
        );
    }

    #[test]
    fn shift_month_up_clamps_the_day() {
        // Jan 31 + 1 month → Feb 29 in a leap year (month digits at cols 6-7).
        assert_eq!(
            shifted("<2024-01-31 Wed>", 6, true).as_deref(),
            Some("<2024-02-29 Thu>")
        );
        // …and Feb 28 in a common year.
        assert_eq!(
            shifted("<2023-01-31 Tue>", 6, true).as_deref(),
            Some("<2023-02-28 Tue>")
        );
    }

    #[test]
    fn shift_year_down_on_feb_29_clamps() {
        assert_eq!(
            shifted("<2024-02-29 Thu>", 1, false).as_deref(),
            Some("<2023-02-28 Tue>")
        );
    }

    #[test]
    fn shift_preserves_inactive_brackets_and_time() {
        // Minute field of an inactive stamp with a time (col 19 is a minute digit).
        assert_eq!(
            shifted("[2024-01-15 Mon 09:59]", 19, true).as_deref(),
            Some("[2024-01-15 Mon 10:00]") // minute carries into the hour
        );
    }

    #[test]
    fn shift_a_weekday_column_moves_the_day() {
        // Cursor on the "Mon" weekday shifts the day.
        assert_eq!(
            shifted("<2024-01-15 Mon>", 12, true).as_deref(),
            Some("<2024-01-16 Tue>")
        );
    }

    #[test]
    fn shift_off_a_timestamp_returns_none() {
        assert!(shifted("no stamp here", 3, true).is_none());
        assert!(shifted("x <2024-01-15 Mon>", 0, true).is_none()); // before the stamp
    }

    // ---- set_planning -----------------------------------------------------------

    fn stamp(s: &str) -> Timestamp {
        parse_timestamp(s).unwrap().0
    }

    #[test]
    fn set_planning_inserts_an_indented_line_below_the_heading() {
        let mut doc = Document::from_text("* Task\nbody\n");
        OrgProvider.set_planning(&mut doc, 0, Planning::Scheduled, Some(stamp("<2024-01-15>")));
        assert_eq!(doc.text(), "* Task\n  SCHEDULED: <2024-01-15 Mon>\nbody\n");
    }

    #[test]
    fn set_planning_updates_in_place_and_keeps_the_other_keyword() {
        let mut doc = Document::from_text("* T\n  SCHEDULED: <2024-01-15 Mon>\n");
        OrgProvider.set_planning(&mut doc, 0, Planning::Deadline, Some(stamp("<2024-02-01>")));
        assert_eq!(
            doc.text(),
            "* T\n  SCHEDULED: <2024-01-15 Mon> DEADLINE: <2024-02-01 Thu>\n"
        );
        // Replacing scheduled updates just that stamp.
        OrgProvider.set_planning(&mut doc, 0, Planning::Scheduled, Some(stamp("<2024-01-20>")));
        assert_eq!(
            doc.text(),
            "* T\n  SCHEDULED: <2024-01-20 Sat> DEADLINE: <2024-02-01 Thu>\n"
        );
    }

    #[test]
    fn set_planning_none_removes_the_keyword_and_the_line_when_empty() {
        let mut doc = Document::from_text("* T\n  SCHEDULED: <2024-01-15 Mon>\nbody\n");
        OrgProvider.set_planning(&mut doc, 0, Planning::Scheduled, None);
        assert_eq!(doc.text(), "* T\nbody\n"); // the whole planning line goes
    }

    #[test]
    fn set_planning_off_a_heading_is_a_noop() {
        let mut doc = Document::from_text("plain\n");
        assert!(matches!(
            OrgProvider.set_planning(&mut doc, 0, Planning::Scheduled, Some(stamp("<2024-01-15>"))),
            EditOutcome::NoOp(_)
        ));
    }

    // ---- structural edits -------------------------------------------------------

    #[test]
    fn providers_expose_marker_and_max_level() {
        assert_eq!(OrgProvider.marker(), b'*');
        assert_eq!(OrgProvider.max_level(), None);
        assert_eq!(MarkdownProvider.marker(), b'#');
        assert_eq!(MarkdownProvider.max_level(), Some(6));
        assert_eq!(Format::Markdown.marker(), b'#'); // enum delegates
    }

    #[test]
    fn promote_and_demote_change_one_heading_level() {
        let mut doc = Document::from_text("** A\n*** child\n");
        assert_eq!(
            OrgProvider.promote_heading(&mut doc, 0),
            EditOutcome::Changed { cursor_line: 0 }
        );
        assert_eq!(doc.text(), "* A\n*** child\n"); // child untouched
        assert_eq!(
            OrgProvider.demote_heading(&mut doc, 0),
            EditOutcome::Changed { cursor_line: 0 }
        );
        assert_eq!(doc.text(), "** A\n*** child\n");
    }

    #[test]
    fn edits_target_the_enclosing_heading_from_a_body_line() {
        let mut doc = Document::from_text("** A\nbody\n");
        OrgProvider.demote_heading(&mut doc, 1); // cursor on "body"
        assert_eq!(doc.text(), "*** A\nbody\n");
    }

    #[test]
    fn promote_refuses_at_level_1_and_demote_at_markdown_max() {
        let mut doc = Document::from_text("* top\n");
        assert!(matches!(OrgProvider.promote_heading(&mut doc, 0), EditOutcome::NoOp(_)));
        let mut doc = Document::from_text("###### deep\n");
        assert!(matches!(MarkdownProvider.demote_heading(&mut doc, 0), EditOutcome::NoOp(_)));
        let mut doc = Document::from_text("plain\n");
        assert!(matches!(OrgProvider.promote_heading(&mut doc, 0), EditOutcome::NoOp(_)));
    }

    #[test]
    fn subtree_promote_and_demote_shift_every_member() {
        let mut doc = Document::from_text("** A\nbody\n*** B\n** next\n");
        OrgProvider.demote_subtree(&mut doc, 0);
        assert_eq!(doc.text(), "*** A\nbody\n**** B\n** next\n"); // sibling untouched
        OrgProvider.promote_subtree(&mut doc, 0);
        assert_eq!(doc.text(), "** A\nbody\n*** B\n** next\n");
    }

    #[test]
    fn subtree_demote_refuses_if_any_member_would_pass_markdown_max() {
        let mut doc = Document::from_text("##### A\n###### deep child\n");
        assert!(matches!(MarkdownProvider.demote_subtree(&mut doc, 0), EditOutcome::NoOp(_)));
        assert_eq!(doc.text(), "##### A\n###### deep child\n"); // untouched
    }

    #[test]
    fn move_subtree_swaps_with_the_adjacent_same_level_sibling() {
        let text = "* A\na body\n** A child\n* B\nb body\n";
        let mut doc = Document::from_text(text);
        // Cursor on "a body" (line 1): A's 3-line subtree swaps with B's 2-line one.
        assert_eq!(
            OrgProvider.move_subtree_down(&mut doc, 1),
            EditOutcome::Changed { cursor_line: 3 }
        );
        assert_eq!(doc.text(), "* B\nb body\n* A\na body\n** A child\n");
        assert_eq!(
            OrgProvider.move_subtree_up(&mut doc, 3),
            EditOutcome::Changed { cursor_line: 1 }
        );
        assert_eq!(doc.text(), text);
    }

    #[test]
    fn move_refuses_at_the_edges_and_across_parents() {
        let mut doc = Document::from_text("* A\n* B\n");
        assert!(matches!(OrgProvider.move_subtree_up(&mut doc, 0), EditOutcome::NoOp(_)));
        assert!(matches!(OrgProvider.move_subtree_down(&mut doc, 1), EditOutcome::NoOp(_)));
        // A child may not escape its parent in either direction.
        let mut doc = Document::from_text("* P1\n** only child\n* P2\n");
        assert!(matches!(OrgProvider.move_subtree_up(&mut doc, 1), EditOutcome::NoOp(_)));
        assert!(matches!(OrgProvider.move_subtree_down(&mut doc, 1), EditOutcome::NoOp(_)));
    }

    #[test]
    fn move_down_at_end_of_buffer_without_trailing_newline_keeps_text_intact() {
        let mut doc = Document::from_text("* A\n* B no newline");
        OrgProvider.move_subtree_down(&mut doc, 0);
        assert_eq!(doc.text(), "* B no newline\n* A");
    }

    #[test]
    fn insert_sibling_lands_after_the_current_subtree_at_the_same_level() {
        let mut doc = Document::from_text("** A\nbody\n* next\n");
        // Cursor on "body": sibling of A (level 2) goes before "* next".
        assert_eq!(
            OrgProvider.insert_sibling(&mut doc, 1, false),
            EditOutcome::Changed { cursor_line: 2 }
        );
        assert_eq!(doc.text(), "** A\nbody\n** \n* next\n");
    }

    #[test]
    fn insert_todo_sibling_carries_the_keyword() {
        let mut doc = Document::from_text("# A\n");
        MarkdownProvider.insert_sibling(&mut doc, 0, true);
        assert_eq!(doc.text(), "# A\n# TODO \n");
    }

    #[test]
    fn insert_at_end_of_buffer_without_trailing_newline() {
        let mut doc = Document::from_text("* A\nbody no newline");
        assert_eq!(
            OrgProvider.insert_sibling(&mut doc, 1, false),
            EditOutcome::Changed { cursor_line: 2 }
        );
        assert_eq!(doc.text(), "* A\nbody no newline\n* \n");
    }

    #[test]
    fn insert_with_no_enclosing_heading_appends_a_level_1_heading() {
        let mut doc = Document::from_text("just prose\n");
        assert_eq!(
            OrgProvider.insert_sibling(&mut doc, 0, false),
            EditOutcome::Changed { cursor_line: 1 }
        );
        assert_eq!(doc.text(), "just prose\n* \n");
    }

    #[test]
    fn priority_up_walks_none_c_b_a_and_stops() {
        let mut doc = Document::from_text("* TODO task\n");
        OrgProvider.cycle_priority(&mut doc, 0, true);
        assert_eq!(doc.text(), "* TODO [#C] task\n"); // cookie after the keyword
        OrgProvider.cycle_priority(&mut doc, 0, true);
        OrgProvider.cycle_priority(&mut doc, 0, true);
        assert_eq!(doc.text(), "* TODO [#A] task\n");
        assert!(matches!(OrgProvider.cycle_priority(&mut doc, 0, true), EditOutcome::NoOp(_)));
    }

    #[test]
    fn priority_down_walks_a_b_c_none_and_stops() {
        let mut doc = Document::from_text("## [#A] task\n"); // Markdown, no keyword
        MarkdownProvider.cycle_priority(&mut doc, 0, false);
        assert_eq!(doc.text(), "## [#B] task\n");
        MarkdownProvider.cycle_priority(&mut doc, 0, false);
        MarkdownProvider.cycle_priority(&mut doc, 0, false);
        assert_eq!(doc.text(), "## task\n"); // cookie and its space removed
        assert!(matches!(
            MarkdownProvider.cycle_priority(&mut doc, 0, false),
            EditOutcome::NoOp(_)
        ));
    }

    #[test]
    fn priority_on_a_bare_cookie_headline_removes_cleanly() {
        let mut doc = Document::from_text("* DONE [#C]\n"); // keyword, cookie, no title
        OrgProvider.cycle_priority(&mut doc, 0, false);
        assert_eq!(doc.text(), "* DONE \n");
    }

    #[test]
    fn set_tags_appends_replaces_and_removes() {
        let mut doc = Document::from_text("* TODO task\n");
        OrgProvider.set_tags(&mut doc, 0, &["work".into(), "urgent".into()]);
        assert_eq!(doc.text(), "* TODO task :work:urgent:\n");
        OrgProvider.set_tags(&mut doc, 0, &["home".into()]); // replaces, not appends
        assert_eq!(doc.text(), "* TODO task :home:\n");
        OrgProvider.set_tags(&mut doc, 0, &[]); // removes
        assert_eq!(doc.text(), "* TODO task\n");
    }

    #[test]
    fn is_valid_tag_enforces_the_character_set() {
        assert!(is_valid_tag("a_1@b"));
        assert!(!is_valid_tag("has space"));
        assert!(!is_valid_tag(""));
    }

    // ---- Format detection -----------------------------------------------------

    #[test]
    fn detect_format_maps_md_and_markdown_extensions() {
        assert_eq!(detect_format(Some(Path::new("a.md"))), Format::Markdown);
        assert_eq!(detect_format(Some(Path::new("dir/b.markdown"))), Format::Markdown);
        assert_eq!(detect_format(Some(Path::new("a.org"))), Format::Org);
        assert_eq!(detect_format(Some(Path::new("a.txt"))), Format::Org);
        assert_eq!(detect_format(Some(Path::new("README"))), Format::Org);
        assert_eq!(detect_format(None), Format::Org);
    }

    #[test]
    fn detect_format_is_case_insensitive() {
        assert_eq!(detect_format(Some(Path::new("A.MD"))), Format::Markdown);
        assert_eq!(detect_format(Some(Path::new("b.Markdown"))), Format::Markdown);
    }

    #[test]
    fn format_enum_delegates_to_the_right_provider() {
        let doc = Document::from_text("* org\n# md\n");
        let org = Format::Org.parse(&doc);
        assert_eq!(org.headings.len(), 1);
        assert_eq!(org.headings[0].title, "org");
        let md = Format::Markdown.parse(&doc);
        assert_eq!(md.headings.len(), 1);
        assert_eq!(md.headings[0].title, "md");

        let mut doc = Document::from_text("# task\n");
        Format::Markdown.cycle_todo(&mut doc, 0);
        assert_eq!(doc.text(), "# TODO task\n");
    }
}
