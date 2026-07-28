//! Plain lists: bullets, checkboxes, statistics cookies, and their edits.
//! Free functions over `Document` + `Format`, like `timestamp` and `search` — list syntax
//! differs per format by only the bullet rules, so there is no new trait.

use crate::document::Document;
use crate::structure::{line_in_fence, EditOutcome, Format, Heading, Outline, StructureProvider};

/// The bullet marker that opens a list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bullet {
    Dash,
    Plus,
    Star,
    /// `1.` or `1)` — `number` as written, `paren` true for `)`.
    Ordered { number: usize, paren: bool },
}

/// A checkbox's state: `[ ]`, `[X]`/`[x]`, or `[-]` (partial — parsed and displayed, but
/// torg never writes it; see the design doc's scope notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    Partial,
}

/// A parsed list item line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListItem {
    pub line: usize,
    /// Leading indentation, in chars. A tab counts as one char, same as any other
    /// character — nesting comparisons are purely by this count, not by display width.
    pub indent: usize,
    pub bullet: Bullet,
    pub checkbox: Option<CheckState>,
    /// Char col where the content starts (after bullet/checkbox), the cursor target.
    pub content_col: usize,
}

/// Parse the list item at `line`, or `None` if that line is not one: blank, prose, a
/// heading, a bullet with no following space, or (Markdown only) inside a fenced code block.
///
/// A single-query convenience: it computes `line`'s own fence membership on the spot
/// (`line_in_fence` is O(line)). Code that queries many lines in a row — like
/// `update_cookies`'s region/section walks below — should instead precompute fence
/// membership once with [`fence_flags_for`] and go through [`item_at_cached`], or repeated
/// calls to this function turn an O(lines) walk into O(lines²) on a large Markdown document.
pub fn item_at(doc: &Document, line: usize, format: Format) -> Option<ListItem> {
    if line >= doc.line_count() {
        return None;
    }
    let in_fence = format == Format::Markdown && line_in_fence(doc, line);
    item_at_with_fence(doc, line, format, in_fence)
}

/// [`item_at`]'s guts, taking the fence-membership answer instead of computing it — the
/// shared core for both the single-query and cached-batch paths.
fn item_at_with_fence(doc: &Document, line: usize, format: Format, in_fence: bool) -> Option<ListItem> {
    if line >= doc.line_count() || in_fence {
        return None;
    }
    let raw = doc.line_text(line);
    let text = raw.strip_suffix('\n').unwrap_or(&raw);
    parse_item(text, line, format)
}

/// [`item_at`], but reading fence membership from a precomputed table (see
/// [`fence_flags_for`]) instead of rescanning from line 0 — an O(1) lookup per line.
fn item_at_cached(doc: &Document, line: usize, format: Format, fences: Option<&[bool]>) -> Option<ListItem> {
    if line >= doc.line_count() {
        return None;
    }
    let in_fence = fences.map(|f| f[line]).unwrap_or(false);
    item_at_with_fence(doc, line, format, in_fence)
}

/// Markdown fence membership for every line of `doc`, computed once via a single forward
/// pass ([`crate::structure::fence_flags`]) — `None` for Org, which has no fence concept and
/// so needs no table at all. Built once per [`update_cookies`] call and threaded through its
/// region/section walks so they each do an O(1) fence lookup per line instead of the O(line)
/// rescan `item_at` does for a one-off query.
fn fence_flags_for(doc: &Document, format: Format) -> Option<Vec<bool>> {
    if format != Format::Markdown || doc.line_count() == 0 {
        return None;
    }
    Some(crate::structure::fence_flags(doc, doc.line_count() - 1))
}

/// Parse a single raw line (no trailing newline) into a [`ListItem`].
fn parse_item(text: &str, line: usize, format: Format) -> Option<ListItem> {
    let indent = text.chars().take_while(|&c| c == ' ' || c == '\t').count();
    // Indentation is spaces/tabs only, so byte offset == char offset up to here.
    let after = &text[indent..];
    let mut chars = after.chars();
    let first = chars.next()?;

    let (bullet, rest_start) = if first == '-' || first == '+' || first == '*' {
        if first == '*' && format == Format::Org && indent == 0 {
            return None; // column-0 `*` in Org is a heading, not a bullet
        }
        let bullet = match first {
            '-' => Bullet::Dash,
            '+' => Bullet::Plus,
            _ => Bullet::Star,
        };
        (bullet, 1)
    } else if first.is_ascii_digit() {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let delim = after[digits.len()..].chars().next();
        match delim {
            Some('.') | Some(')') => {
                let number = digits.parse().ok()?;
                let paren = delim == Some(')');
                (Bullet::Ordered { number, paren }, digits.len() + 1)
            }
            _ => return None,
        }
    } else {
        return None;
    };

    // The bullet must be followed by exactly one space; anything else (no space, or a
    // bracket glued straight on) means the line is not a list item at all.
    if !after[rest_start..].starts_with(' ') {
        return None;
    }
    let content_start = indent + rest_start + 1;
    let rest = &text[content_start..];

    let (checkbox, content_col) = match parse_checkbox(rest, format) {
        Some((state, consumed)) => (Some(state), content_start + consumed),
        None => (None, content_start),
    };

    Some(ListItem { line, indent, bullet, checkbox, content_col })
}

/// Parse a leading `[ ]`/`[X]`/`[x]`/`[-]` checkbox off `rest` (the text right after the
/// bullet's one space). Returns the state and how many chars it (plus one trailing space,
/// if present) consumed. `None` if `rest` doesn't open with a recognized checkbox token, OR
/// if it does but isn't in the required space-delimited form: a checkbox is only a checkbox
/// when the closing bracket is followed by a space or is the end of the line (`- [ ] task`,
/// `- [ ]`); glued-on content (`- [ ]x`) means the brackets are ordinary text instead.
fn parse_checkbox(rest: &str, _format: Format) -> Option<(CheckState, usize)> {
    let mut chars = rest.chars();
    if chars.next() != Some('[') {
        return None;
    }
    let state = match chars.next()? {
        ' ' => CheckState::Unchecked,
        'X' | 'x' => CheckState::Checked,
        '-' => CheckState::Partial,
        _ => return None,
    };
    if chars.next() != Some(']') {
        return None;
    }
    // "[ ]" is 3 chars. What follows must be a space (consumed too — content resumes after
    // it) or nothing at all (end of line); anything else disqualifies the whole match.
    match rest[3..].chars().next() {
        None => Some((state, 3)),
        Some(' ') => Some((state, 4)),
        Some(_) => None,
    }
}

/// Status text for [`toggle_checkbox`] off a checkbox.
const NO_CHECKBOX: &str = "No checkbox here";

/// Flip the checkbox at `line`: `[ ]`/`[-]` → checked, `[X]`/`[x]` → unchecked, written back
/// format-aware (`[X]` in Org, `[x]` in Markdown — the design doc's syntax table). A no-op
/// with [`NO_CHECKBOX`] off an item line or off an item with no checkbox. Recomputes the
/// affected cookies afterward via [`update_cookies`]; the cursor stays on `line`.
pub fn toggle_checkbox(doc: &mut Document, line: usize, format: Format) -> EditOutcome {
    let Some(item) = item_at(doc, line, format) else {
        return EditOutcome::NoOp(NO_CHECKBOX);
    };
    let Some(state) = item.checkbox else {
        return EditOutcome::NoOp(NO_CHECKBOX);
    };
    let raw = doc.line_text(line);
    let text = raw.strip_suffix('\n').unwrap_or(&raw);
    let state_col = checkbox_state_col(text, item.content_col)
        .expect("item_at reported a checkbox, so its state char must be locatable");
    let new_char = match state {
        CheckState::Checked => ' ',
        CheckState::Unchecked | CheckState::Partial => checked_char(format),
    };
    let base = doc.line_to_char(line);
    doc.remove(base + state_col..base + state_col + 1);
    doc.insert(base + state_col, &new_char.to_string());
    update_cookies(doc, line, format);
    EditOutcome::Changed { cursor_line: line }
}

/// The char written for a checked box: `X` in Org, `x` in Markdown (GFM convention).
fn checked_char(format: Format) -> char {
    match format {
        Format::Org => 'X',
        Format::Markdown => 'x',
    }
}

/// The char column of the state character inside a line's checkbox (`[` col+1 `]`), derived
/// from `content_col` (the column just past the checkbox and its one trailing space, if the
/// line has content after it) without needing to reconstruct the bullet's width. `None` if
/// the chars immediately before `content_col` don't spell out a checkbox — callers only
/// reach this once `item_at` has already confirmed one exists, so this should not happen.
fn checkbox_state_col(text: &str, content_col: usize) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    if content_col >= 1 && content_col <= chars.len() && chars[content_col - 1] == ']' {
        return Some(content_col - 2); // "- [ ]" at EOL: no trailing space was consumed
    }
    if content_col >= 2
        && content_col <= chars.len()
        && chars[content_col - 2] == ']'
        && chars[content_col - 1] == ' '
    {
        return Some(content_col - 3); // "- [ ] …": the trailing space was consumed too
    }
    None
}

/// Recompute every `[n/m]` / `[p%]` statistics cookie affected by a change on `line`: every
/// ancestor item's cookie (counting that ancestor's DIRECT children only) and the enclosing
/// heading's cookie (counting TOP-LEVEL checkbox items in the heading's own section only).
/// Never inserts a cookie that wasn't already there — see the design doc's cookie rules.
///
/// Cost profile: this runs once per user edit (toggle/insert/indent), not on every keystroke.
/// On Markdown it first builds a fence-membership table for the whole document with one
/// forward pass ([`fence_flags_for`]) — without it, each of the many `item_at`-style line
/// checks below would rescan from line 0 on its own, turning this into an O(lines²) walk on
/// a large document (that regression was caught by a 4000-line-checklist timing probe; the
/// fix is this shared table). With the table in hand, it does one pass over the contiguous
/// item region containing `line` — [`region_items`] parses each of those lines exactly once,
/// however many ancestors `line` has — then one direct-children scan per ancestor over that
/// same in-memory list (no further per-line line-parses there), plus one pass over the
/// enclosing heading's own section to count top-level checkbox items (again one parse per
/// line in that section). So the total cost is one O(document) fence pass (Markdown only)
/// plus work linear in the affected list region and one heading section — not quadratic in
/// document size, and no line is ever re-parsed once for each ancestor.
pub fn update_cookies(doc: &mut Document, line: usize, format: Format) {
    let fences = fence_flags_for(doc, format);
    let items = region_items(doc, line, format, fences.as_deref());
    for parent_line in parent_chain(&items, line) {
        let (checked, total) = direct_children(&items, parent_line);
        rewrite_cookies_on_line(doc, parent_line, checked, total);
    }

    let outline = format.parse(doc);
    if let Some(h) = enclosing_heading(&outline, line) {
        let section_end = heading_own_section_end(&outline, h);
        let (checked, total) =
            top_level_checkbox_counts(doc, h.line + 1, section_end, format, fences.as_deref());
        rewrite_cookies_on_line(doc, h.line, checked, total);
    }
}

/// The maximal contiguous run of item lines containing `line` (empty if `line` itself isn't
/// one): expands outward from `line` while items keep being found, so each line in the
/// region is parsed exactly once regardless of how many ancestors are later queried against
/// the result. `fences` is the precomputed table from [`fence_flags_for`] (or `None` on Org).
fn region_items(doc: &Document, line: usize, format: Format, fences: Option<&[bool]>) -> Vec<ListItem> {
    let Some(cur) = item_at_cached(doc, line, format, fences) else {
        return Vec::new();
    };
    let mut items = vec![cur];
    let mut l = line;
    while l > 0 {
        match item_at_cached(doc, l - 1, format, fences) {
            Some(it) => {
                items.push(it);
                l -= 1;
            }
            None => break,
        }
    }
    items.reverse();
    let mut l = line;
    while let Some(it) = item_at_cached(doc, l + 1, format, fences) {
        items.push(it);
        l += 1;
    }
    items
}

/// The lines of every ancestor of the item at `line` within `items` (nearest first): repeat
/// "nearest preceding item with strictly smaller indent" until none remains.
fn parent_chain(items: &[ListItem], line: usize) -> Vec<usize> {
    let mut chain = Vec::new();
    let Some(mut idx) = items.iter().position(|it| it.line == line) else {
        return chain;
    };
    while let Some(p) = items[..idx].iter().rposition(|it| it.indent < items[idx].indent) {
        chain.push(items[p].line);
        idx = p;
    }
    chain
}

/// `(checked, total)` among the direct children of the item at `parent_line` within `items`:
/// the items below it, before the next item at or above its indent, restricted to the
/// MINIMUM indent found in that span — so grandchildren nested deeper than the direct
/// children are excluded — and, of those, only the ones that are themselves checkbox items.
fn direct_children(items: &[ListItem], parent_line: usize) -> (usize, usize) {
    let Some(p_idx) = items.iter().position(|it| it.line == parent_line) else {
        return (0, 0);
    };
    let parent_indent = items[p_idx].indent;
    let span: Vec<&ListItem> = items[p_idx + 1..]
        .iter()
        .take_while(|it| it.indent > parent_indent)
        .collect();
    let Some(min_indent) = span.iter().map(|it| it.indent).min() else {
        return (0, 0);
    };
    let total = span.iter().filter(|it| it.indent == min_indent && it.checkbox.is_some()).count();
    let checked = span
        .iter()
        .filter(|it| it.indent == min_indent && it.checkbox == Some(CheckState::Checked))
        .count();
    (checked, total)
}

/// The heading whose subtree contains `line` — the nearest heading at or above it. Mirrors
/// `structure::enclosing`, which is private to that module; kept local since list.rs is the
/// only other place that needs it.
fn enclosing_heading(outline: &Outline, line: usize) -> Option<&Heading> {
    outline.headings.iter().rev().find(|h| h.line <= line)
}

/// The end of `h`'s own section text: its subtree body, but stopping before any child
/// heading's own section — i.e. before the next heading at ANY level, not just `h`'s level.
fn heading_own_section_end(outline: &Outline, h: &Heading) -> usize {
    outline
        .headings
        .iter()
        .find(|other| other.line > h.line)
        .map(|next| if next.line <= h.last_line { next.line - 1 } else { h.last_line })
        .unwrap_or(h.last_line)
}

/// `(checked, total)` among the TOP-LEVEL checkbox items in `start..=end`: items with no
/// item above them, within the same contiguous item run, at a strictly smaller indent —
/// nested items (and whole nested lists under them) are excluded. Each line in the range is
/// parsed exactly once. `fences` is the precomputed table from [`fence_flags_for`] (or `None`
/// on Org).
fn top_level_checkbox_counts(
    doc: &Document,
    start: usize,
    end: usize,
    format: Format,
    fences: Option<&[bool]>,
) -> (usize, usize) {
    let last = doc.line_count().saturating_sub(1);
    if start > last {
        return (0, 0);
    }
    let end = end.min(last);
    let mut run: Vec<ListItem> = Vec::new(); // the current contiguous item run
    let mut checked = 0usize;
    let mut total = 0usize;
    for l in start..=end {
        match item_at_cached(doc, l, format, fences) {
            Some(item) => {
                let has_parent = run.iter().any(|prev| prev.indent < item.indent);
                if !has_parent && item.checkbox.is_some() {
                    total += 1;
                    if item.checkbox == Some(CheckState::Checked) {
                        checked += 1;
                    }
                }
                run.push(item);
            }
            None => run.clear(), // a non-item line ends the current run
        }
    }
    (checked, total)
}

/// Rewrite every `[n/m]` / `[p%]` (including the unfilled `[/]` / `[%]` forms) cookie token
/// found in `line`'s text to reflect `(checked, total)`; a no-op if the line has none, so a
/// cookie is never inserted. Percentages truncate (`100 * checked / total`); `total == 0`
/// writes `0` literally (`[0/0]`, `[0%]`), matching Org. Edits are rightmost-first so
/// earlier spans stay valid as later ones are applied.
fn rewrite_cookies_on_line(doc: &mut Document, line: usize, checked: usize, total: usize) {
    let raw = doc.line_text(line);
    let text = raw.strip_suffix('\n').unwrap_or(&raw).to_string();
    let spans = cookie_spans(&text);
    if spans.is_empty() {
        return;
    }
    let base = doc.line_to_char(line);
    let pct = (100 * checked).checked_div(total).unwrap_or(0);
    for (start, end, is_percent) in spans.into_iter().rev() {
        let replacement =
            if is_percent { format!("[{pct}%]") } else { format!("[{checked}/{total}]") };
        doc.remove(base + start..base + end);
        doc.insert(base + start, &replacement);
    }
}

/// Char spans of every `[...]` token in `text` that is a statistics cookie — `[n/m]`, `[p%]`,
/// or their unfilled forms `[/]`/`[%]` — as `(start, end_exclusive, is_percent)`. Anything
/// else in brackets (a checkbox, a priority cookie, plain text) matches neither shape and is
/// left alone.
fn cookie_spans(text: &str) -> Vec<(usize, usize, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(rel_close) = chars[i + 1..].iter().position(|&c| c == ']') {
                let close = i + 1 + rel_close;
                let content: String = chars[i + 1..close].iter().collect();
                if is_fraction_cookie(&content) {
                    spans.push((i, close + 1, false));
                    i = close + 1;
                    continue;
                } else if is_percent_cookie(&content) {
                    spans.push((i, close + 1, true));
                    i = close + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    spans
}

/// Whether `s` (the text between a cookie's brackets) is an `[n/m]` fraction, filled or not
/// (`3/5`, `/5`, `3/`, or the fully unfilled `/`).
fn is_fraction_cookie(s: &str) -> bool {
    match s.split_once('/') {
        Some((a, b)) => {
            a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Whether `s` is a `[p%]` percentage, filled or not (`50%` or the unfilled `%`).
fn is_percent_cookie(s: &str) -> bool {
    s.strip_suffix('%').is_some_and(|digits| digits.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::from_text(text)
    }

    // ---- unordered bullets --------------------------------------------------

    #[test]
    fn dash_bullet_parses_at_various_indents() {
        let item = item_at(&doc("- task\n"), 0, Format::Org).unwrap();
        assert_eq!(item.indent, 0);
        assert_eq!(item.bullet, Bullet::Dash);
        assert_eq!(item.content_col, 2);

        let item = item_at(&doc("  - task\n"), 0, Format::Org).unwrap();
        assert_eq!(item.indent, 2);
        assert_eq!(item.content_col, 4);

        let item = item_at(&doc("     - task\n"), 0, Format::Org).unwrap();
        assert_eq!(item.indent, 5);
        assert_eq!(item.content_col, 7);
    }

    #[test]
    fn plus_bullet_parses() {
        let item = item_at(&doc("+ task\n"), 0, Format::Org).unwrap();
        assert_eq!(item.bullet, Bullet::Plus);
        assert_eq!(item.content_col, 2);
    }

    #[test]
    fn star_at_column_zero_is_a_heading_in_org_but_a_bullet_in_markdown() {
        assert!(item_at(&doc("* task\n"), 0, Format::Org).is_none());
        let item = item_at(&doc("* task\n"), 0, Format::Markdown).unwrap();
        assert_eq!(item.bullet, Bullet::Star);
        assert_eq!(item.indent, 0);
        assert_eq!(item.content_col, 2);
    }

    #[test]
    fn indented_star_parses_in_org() {
        let item = item_at(&doc("  * task\n"), 0, Format::Org).unwrap();
        assert_eq!(item.bullet, Bullet::Star);
        assert_eq!(item.indent, 2);
        assert_eq!(item.content_col, 4);
    }

    // ---- ordered bullets ------------------------------------------------------

    #[test]
    fn ordered_bullet_parses_dot_and_paren_styles() {
        let item = item_at(&doc("3. x\n"), 0, Format::Org).unwrap();
        assert_eq!(item.bullet, Bullet::Ordered { number: 3, paren: false });
        assert_eq!(item.content_col, 3);

        let item = item_at(&doc("3) x\n"), 0, Format::Org).unwrap();
        assert_eq!(item.bullet, Bullet::Ordered { number: 3, paren: true });
        assert_eq!(item.content_col, 3);
    }

    #[test]
    fn ordered_bullet_without_a_space_is_not_an_item() {
        assert!(item_at(&doc("1.x\n"), 0, Format::Org).is_none());
    }

    // ---- checkboxes -------------------------------------------------------------

    #[test]
    fn checkbox_states_parse_including_lowercase_x() {
        let item = item_at(&doc("- [ ] t\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, Some(CheckState::Unchecked));
        assert_eq!(item.content_col, 6);

        let item = item_at(&doc("- [X] t\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, Some(CheckState::Checked));
        assert_eq!(item.content_col, 6);

        let item = item_at(&doc("- [x] t\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, Some(CheckState::Checked));
        assert_eq!(item.content_col, 6);

        let item = item_at(&doc("- [-] t\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, Some(CheckState::Partial));
        assert_eq!(item.content_col, 6);
    }

    #[test]
    fn bullet_glued_to_a_bracket_with_no_space_is_not_an_item() {
        // `-[ ]` has no space after the bullet, so the bullet rule fails outright — this is
        // not "an item with no checkbox", it is not an item at all.
        assert!(item_at(&doc("-[ ] t\n"), 0, Format::Org).is_none());
    }

    #[test]
    fn a_bracket_later_in_the_content_is_not_a_checkbox() {
        let item = item_at(&doc("- x [ ] y\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, None);
        assert_eq!(item.content_col, 2);
    }

    // ---- non-items ----------------------------------------------------------------

    #[test]
    fn blank_line_is_not_an_item() {
        assert!(item_at(&doc("\n"), 0, Format::Org).is_none());
    }

    #[test]
    fn plain_text_is_not_an_item() {
        assert!(item_at(&doc("just prose\n"), 0, Format::Org).is_none());
    }

    #[test]
    fn heading_lines_are_not_items() {
        assert!(item_at(&doc("* Heading\n"), 0, Format::Org).is_none());
        assert!(item_at(&doc("# Heading\n"), 0, Format::Markdown).is_none());
    }

    // ---- Markdown fence guard ----------------------------------------------------

    #[test]
    fn markdown_list_syntax_inside_a_fenced_block_is_not_an_item() {
        let text = "# A\n```sh\n- item\n```\n- real\n";
        assert!(item_at(&doc(text), 2, Format::Markdown).is_none()); // fenced "- item"
        let item = item_at(&doc(text), 4, Format::Markdown).unwrap(); // real item after the fence
        assert_eq!(item.bullet, Bullet::Dash);
    }

    // ---- carry-over: checkbox requires the space-delimited form -------------------

    #[test]
    fn checkbox_at_end_of_line_with_nothing_after_still_parses() {
        // "- [ ]" with nothing following the closing bracket is a checkbox with empty
        // content, not a rejected match — content_col lands at end of line.
        let item = item_at(&doc("- [ ]\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, Some(CheckState::Unchecked));
        assert_eq!(item.content_col, 5);
    }

    #[test]
    fn checkbox_glued_to_trailing_content_is_not_a_checkbox() {
        // "- [ ]x" has no space delimiting the checkbox from what follows, so the whole
        // bracket is content, not a checkbox.
        let item = item_at(&doc("- [ ]x\n"), 0, Format::Org).unwrap();
        assert_eq!(item.checkbox, None);
        assert_eq!(item.content_col, 2);
    }

    // ---- toggle_checkbox ------------------------------------------------------------

    #[test]
    fn toggle_unchecked_to_checked_writes_uppercase_x_in_org() {
        let mut d = doc("- [ ] task\n");
        let outcome = toggle_checkbox(&mut d, 0, Format::Org);
        assert_eq!(outcome, EditOutcome::Changed { cursor_line: 0 });
        assert_eq!(d.text(), "- [X] task\n");
    }

    #[test]
    fn toggle_unchecked_to_checked_writes_lowercase_x_in_markdown() {
        let mut d = doc("- [ ] task\n");
        toggle_checkbox(&mut d, 0, Format::Markdown);
        assert_eq!(d.text(), "- [x] task\n");
    }

    #[test]
    fn toggle_checked_to_unchecked_in_both_formats() {
        let mut d = doc("- [X] task\n");
        toggle_checkbox(&mut d, 0, Format::Org);
        assert_eq!(d.text(), "- [ ] task\n");

        let mut d = doc("- [x] task\n");
        toggle_checkbox(&mut d, 0, Format::Markdown);
        assert_eq!(d.text(), "- [ ] task\n");
    }

    #[test]
    fn toggle_partial_becomes_checked() {
        let mut d = doc("- [-] task\n");
        toggle_checkbox(&mut d, 0, Format::Org);
        assert_eq!(d.text(), "- [X] task\n");
    }

    #[test]
    fn toggle_on_an_item_with_no_checkbox_is_a_noop() {
        let mut d = doc("- plain item\n");
        let outcome = toggle_checkbox(&mut d, 0, Format::Org);
        assert_eq!(outcome, EditOutcome::NoOp("No checkbox here"));
        assert_eq!(d.text(), "- plain item\n");
    }

    #[test]
    fn toggle_on_a_non_item_line_is_a_noop() {
        let mut d = doc("just prose\n");
        let outcome = toggle_checkbox(&mut d, 0, Format::Org);
        assert_eq!(outcome, EditOutcome::NoOp("No checkbox here"));
        assert_eq!(d.text(), "just prose\n");
    }

    // ---- update_cookies (exercised through toggle_checkbox) ------------------------

    #[test]
    fn toggling_a_child_updates_the_parent_fraction_cookie() {
        let mut d = doc("- top [0/2]\n  - [ ] a\n  - [ ] b\n");
        toggle_checkbox(&mut d, 1, Format::Org); // toggle "a"
        assert_eq!(d.text(), "- top [1/2]\n  - [X] a\n  - [ ] b\n");
    }

    #[test]
    fn grandchildren_do_not_count_toward_the_grandparent_cookie() {
        let mut d =
            doc("- [ ] grandparent [0/1]\n  - [ ] parent [0/1]\n    - [ ] child\n");
        toggle_checkbox(&mut d, 2, Format::Org); // toggle "child"
        assert_eq!(
            d.text(),
            "- [ ] grandparent [0/1]\n  - [ ] parent [1/1]\n    - [X] child\n"
        );
    }

    #[test]
    fn percent_cookie_updates_by_truncation() {
        let mut d = doc("- top [0%]\n  - [X] a\n  - [ ] b\n  - [ ] c\n");
        toggle_checkbox(&mut d, 3, Format::Org); // toggle "c": 1 -> 2 of 3 checked
        assert_eq!(d.text(), "- top [66%]\n  - [X] a\n  - [ ] b\n  - [X] c\n");
    }

    #[test]
    fn unfilled_cookies_get_filled_in() {
        let mut d = doc("- top [/]\n  - [X] a\n  - [ ] b\n");
        toggle_checkbox(&mut d, 2, Format::Org); // toggle "b"
        assert_eq!(d.text(), "- top [2/2]\n  - [X] a\n  - [X] b\n");

        let mut d = doc("- top [%]\n  - [X] a\n  - [ ] b\n");
        toggle_checkbox(&mut d, 2, Format::Org);
        assert_eq!(d.text(), "- top [100%]\n  - [X] a\n  - [X] b\n");
    }

    #[test]
    fn heading_cookie_counts_only_top_level_checkbox_items() {
        let mut d = doc(
            "* Groceries [0/3]\n- [ ] milk\n  - [ ] cream\n- [ ] eggs\n- [ ] bread\n",
        );
        toggle_checkbox(&mut d, 1, Format::Org); // toggle "milk"
        assert_eq!(
            d.text(),
            "* Groceries [1/3]\n- [X] milk\n  - [ ] cream\n- [ ] eggs\n- [ ] bread\n"
        );
    }

    #[test]
    fn a_heading_cookie_in_a_different_section_is_untouched() {
        let mut d = doc("* A [0/1]\n- [ ] a1\n* B [0/1]\n- [ ] b1\n");
        toggle_checkbox(&mut d, 1, Format::Org); // toggle a1, under heading A
        assert_eq!(d.text(), "* A [1/1]\n- [X] a1\n* B [0/1]\n- [ ] b1\n");
    }

    #[test]
    fn multiple_cookies_on_one_line_all_update() {
        let mut d = doc("- top [0/2] again [0/2]\n  - [ ] a\n  - [ ] b\n");
        toggle_checkbox(&mut d, 1, Format::Org); // toggle "a"
        assert_eq!(d.text(), "- top [1/2] again [1/2]\n  - [X] a\n  - [ ] b\n");
    }

    #[test]
    fn a_parent_with_no_cookie_gains_nothing() {
        let mut d = doc("- top no cookie\n  - [ ] a\n  - [ ] b\n");
        toggle_checkbox(&mut d, 1, Format::Org); // toggle "a"
        assert_eq!(d.text(), "- top no cookie\n  - [X] a\n  - [ ] b\n");
    }

    #[test]
    fn cookie_rewrite_preserves_surrounding_text_on_the_line() {
        let mut d = doc("- Shopping [0/2] (due Friday)\n  - [ ] a\n  - [ ] b\n");
        toggle_checkbox(&mut d, 1, Format::Org); // toggle "a"
        assert_eq!(d.text(), "- Shopping [1/2] (due Friday)\n  - [X] a\n  - [ ] b\n");
    }

    #[test]
    fn heading_cookie_excludes_a_child_headings_own_section() {
        // P's own section ends right before C (the next heading at ANY level), even though
        // C is nested inside P's subtree — so P's cookie counts only p1, not c1/c2.
        let mut d = doc("* P [/]\n- [ ] p1\n** C\n- [ ] c1\n- [ ] c2\n");
        toggle_checkbox(&mut d, 1, Format::Org); // toggle p1
        assert_eq!(d.text(), "* P [1/1]\n- [X] p1\n** C\n- [ ] c1\n- [ ] c2\n");

        // A change inside the child heading's own section doesn't touch P's cookie.
        toggle_checkbox(&mut d, 3, Format::Org); // toggle c1
        assert_eq!(d.text(), "* P [1/1]\n- [X] p1\n** C\n- [X] c1\n- [ ] c2\n");
    }

    #[test]
    fn heading_percent_cookie_with_no_countable_boxes_recomputes_to_zero_percent() {
        let mut d = doc("* H [100%]\nplain text, no items\n");
        update_cookies(&mut d, 0, Format::Org);
        assert_eq!(d.text(), "* H [0%]\nplain text, no items\n");
    }

    #[test]
    fn heading_fraction_cookie_with_no_countable_boxes_recomputes_to_zero_over_zero() {
        let mut d = doc("* H [3/5]\nplain text, no items\n");
        update_cookies(&mut d, 0, Format::Org);
        assert_eq!(d.text(), "* H [0/0]\nplain text, no items\n");
    }
}
