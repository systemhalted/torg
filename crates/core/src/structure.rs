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

/// A parser + editor of a particular document format's structure. One implementer per
/// format (Org today; Markdown next). Everything else in textr talks to structure through
/// this trait, never to a concrete format.
pub trait StructureProvider {
    /// Scan `doc` into an [`Outline`].
    fn parse(&self, doc: &Document) -> Outline;

    /// Cycle the TODO keyword on the heading at `line`: none → `TODO` → `DONE` → none.
    /// A no-op if that line is not a heading.
    fn cycle_todo(&self, doc: &mut Document, line: usize);
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
        fill_subtree_extents(&mut headings, doc.line_count().saturating_sub(1));
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
        last_line: line,
    })
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
        fill_subtree_extents(&mut headings, doc.line_count().saturating_sub(1));
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
