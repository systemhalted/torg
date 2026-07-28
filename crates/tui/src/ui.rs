//! Rendering — a pure view of [`App`] into a ratatui `Frame`. No crossterm, no I/O: this
//! tier is driver-agnostic, so a future GUI could drive the same state through a different
//! backend. Everything here derives from `&App`; it never mutates.

use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use torg_core::timestamp::find_timestamps;

use crate::app::{App, DatePurpose, Mode};
use crate::commands::{commands_in, Category};

/// The status-line prefix for a date prompt.
fn date_prompt_label(purpose: DatePurpose) -> &'static str {
    match purpose {
        DatePurpose::Scheduled => "Scheduled: ",
        DatePurpose::Deadline => "Deadline: ",
        DatePurpose::InsertActive | DatePurpose::InsertInactive => "Timestamp: ",
    }
}

/// Draw the whole editor: the (fold-aware) text body, then the status line, then place the
/// real hardware cursor.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let status = Rect::new(area.x, area.y + body.height, area.width, 1);

    let cursor = if let Mode::BufferList { selected } = app.mode() {
        draw_buffer_list(frame, app, body, *selected);
        None
    } else if let Mode::HelpMenu { category, selected } = app.mode() {
        draw_help_menu(frame, body, *category, *selected);
        None
    } else {
        draw_body(frame, app, body)
    };
    draw_status(frame, app, status);
    place_cursor(frame, app, body, status, cursor);
}

/// The buffer-list picker: one row per open file, the selected one reversed. Replaces the
/// document body while [`Mode::BufferList`] is active — the app's rendering stays a single
/// paragraph, no overlay machinery.
fn draw_buffer_list(frame: &mut Frame, app: &App, body: Rect, selected: usize) {
    let lines: Vec<Line> = app
        .buffer_labels()
        .into_iter()
        .enumerate()
        .map(|(i, (name, dirty))| {
            let text = format!(" {} {}{} ", i + 1, name, if dirty { "*" } else { "" });
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::styled(text, style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
}

/// The help menu's lines: a tab row (one span per category, the active one reversed), a blank
/// line, then one row per command in the active category (the selected row reversed). Pure so
/// tests can call it without a terminal.
fn help_menu_lines(category: usize, selected: usize, _width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut tabs: Vec<Span> = Vec::new();
    for (i, cat) in Category::ALL.iter().enumerate() {
        let style = if i == category {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        tabs.push(Span::styled(format!(" {} ", cat.title()), style));
        tabs.push(Span::raw(" "));
    }
    lines.push(Line::from(tabs));
    lines.push(Line::raw(""));
    for (i, cmd) in commands_in(Category::ALL[category]).iter().enumerate() {
        let text = format!(" {:<14} {:<26} {}", cmd.keys, cmd.name, cmd.description);
        let style = if i == selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::styled(text, style));
    }
    lines
}

/// The help menu: category tabs, then the active category's commands, the selected one
/// reversed. Replaces the document body while [`Mode::HelpMenu`] is active — the same
/// body-replacement pattern as [`draw_buffer_list`].
fn draw_help_menu(frame: &mut Frame, body: Rect, category: usize, selected: usize) {
    let lines = help_menu_lines(category, selected, body.width as usize);
    frame.render_widget(Paragraph::new(lines), body);
}

/// How many columns a tab advances: to the next multiple of 4.
const TAB_WIDTH: usize = 4;

/// Expand tabs to spaces at [`TAB_WIDTH`]-column stops. The terminal widget renders a raw
/// `\t` as a zero-width cell, garbling the row — every line must pass through here.
fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut col = 0;
    for c in text.chars() {
        if c == '\t' {
            let next_stop = (col / TAB_WIDTH + 1) * TAB_WIDTH;
            out.extend(std::iter::repeat_n(' ', next_stop - col));
            col = next_stop;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// The display column of character index `char_col` in `text` — the same tab-stop rule as
/// [`expand_tabs`], so the hardware cursor lands where the character was drawn.
fn display_col(text: &str, char_col: usize) -> usize {
    let mut col = 0;
    for c in text.chars().take(char_col) {
        col = if c == '\t' {
            (col / TAB_WIDTH + 1) * TAB_WIDTH
        } else {
            col + 1
        };
    }
    col
}

/// Render the visible document lines, skipping any hidden inside a fold. Returns the cursor's
/// on-screen `(column, row)` within `body`, if the cursor line is visible.
fn draw_body(frame: &mut Frame, app: &App, body: Rect) -> Option<(u16, u16)> {
    let doc = app.document();
    let height = body.height as usize;
    let cursor_line = app.view().cursor_line();

    let mut lines: Vec<Line> = Vec::with_capacity(height);
    let mut cursor: Option<(u16, u16)> = None;
    let mut doc_line = app.scroll_top();
    let search_hl = app.search_hl();

    while lines.len() < height && doc_line < doc.line_count() {
        if app.is_hidden(doc_line) {
            doc_line += 1;
            continue;
        }
        let mut text = doc.line_text(doc_line);
        while text.ends_with('\n') || text.ends_with('\r') {
            text.pop();
        }
        if doc_line == cursor_line {
            let col = display_col(&text, app.view().cursor_column());
            cursor = Some((col as u16, lines.len() as u16));
        }
        let mut text = expand_tabs(&text);
        if app.is_folded_heading(doc_line) {
            text.push_str(" …"); // a collapsed subtree
        }
        let base = if is_heading(app, doc_line) {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let line = match search_hl {
            Some((query, (cur_line, cur_col))) => {
                search_line(&text, query, (doc_line == cur_line).then_some(cur_col), base)
            }
            None => highlight_line(&text, base),
        };
        lines.push(line);
        doc_line += 1;
    }

    frame.render_widget(Paragraph::new(lines), body);
    cursor
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let widget =
        Paragraph::new(status_text(app)).style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_widget(widget, area);
}

fn place_cursor(frame: &mut Frame, app: &App, body: Rect, status: Rect, cursor: Option<(u16, u16)>) {
    match app.mode() {
        Mode::Edit => {
            if let Some((col, row)) = cursor {
                frame.set_cursor_position(Position::new(body.x + col, body.y + row));
            }
        }
        Mode::SaveAs { input } => {
            let col = "Save as: ".len() + input.chars().count();
            frame.set_cursor_position(Position::new(status.x + col as u16, status.y));
        }
        Mode::OpenFile { input } => {
            let col = "Open: ".len() + input.chars().count();
            frame.set_cursor_position(Position::new(status.x + col as u16, status.y));
        }
        Mode::EditTags { input } => {
            let col = "Tags: ".len() + input.chars().count();
            frame.set_cursor_position(Position::new(status.x + col as u16, status.y));
        }
        Mode::DatePrompt { input, purpose } => {
            let col = date_prompt_label(*purpose).len() + input.chars().count();
            frame.set_cursor_position(Position::new(status.x + col as u16, status.y));
        }
        Mode::Search { input, .. } => {
            let col = "Find: ".len() + input.chars().count();
            frame.set_cursor_position(Position::new(status.x + col as u16, status.y));
        }
        // No cursor while picking from the buffer list, browsing the help menu, or
        // answering a confirmation.
        Mode::BufferList { .. } | Mode::HelpMenu { .. } | Mode::ConfirmClose | Mode::ConfirmQuit => {}
    }
}

fn is_heading(app: &App, line: usize) -> bool {
    app.outline().headings.iter().any(|h| h.line == line)
}

/// Build a display line from already-tab-expanded `text`, styling any timestamps, the
/// `SCHEDULED:`/`DEADLINE:` planning keywords, checkboxes, and statistics cookies over the
/// `base` style. Timestamps carry no tabs, so the byte ranges from `find_timestamps` line up
/// with the expanded text.
///
/// Checkbox/cookie recognition here is a **cheap lexical scan**, not a call into
/// `torg_core::list::item_at` — that parser re-derives Markdown fence membership from the
/// start of the document on every call, which is fine for the occasional structural edit but
/// far too costly to run per visible line on every frame. The scan below can't tell a real
/// list item from a heading whose title happens to contain `[ ]`-shaped text (no `Format` is
/// threaded through, and fence membership isn't checked at all), so an oddball line could
/// pick up cosmetic styling it doesn't structurally deserve. That's an accepted tradeoff:
/// worst case is a wrongly-colored token, never a wrong edit.
fn highlight_line(text: &str, base: Style) -> Line<'static> {
    let ts_style = base.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED);
    let kw_style = base.fg(Color::Yellow);
    // Reused for both checked checkboxes and complete cookies (mirrors DONE), and for
    // unfinished cookies (mirrors TODO). Unchecked/partial checkboxes just dim the base style.
    let done_style = base.fg(Color::Green);
    let todo_style = base.fg(Color::Red);
    let dim_style = base.add_modifier(Modifier::DIM);

    // Every styled byte range gets collected first, then sorted, so ranges discovered by
    // different passes (keyword scan, timestamp scan, checkbox scan, cookie scan) still come
    // out in left-to-right order for the single emission pass below. When two ranges overlap,
    // the earlier-sorted one wins and the later one is dropped (mirrors the old
    // keyword-then-timestamp precedence, generalized to more passes).
    let mut ranges: Vec<(usize, usize, Style)> = Vec::new();

    // Planning keywords (they precede their timestamps on the line).
    for kw in ["SCHEDULED:", "DEADLINE:"] {
        if let Some(i) = text.find(kw) {
            ranges.push((i, i + kw.len(), kw_style));
        }
    }
    for (s, e) in find_timestamps(text) {
        ranges.push((s, e, ts_style));
    }
    if let Some((byte_range, checked)) = find_checkbox_token(text) {
        ranges.push((byte_range.start, byte_range.end, if checked { done_style } else { dim_style }));
    }
    if line_has_list_or_heading_prefix(text) {
        for (byte_range, complete) in find_cookie_tokens(text) {
            ranges.push((byte_range.start, byte_range.end, if complete { done_style } else { todo_style }));
        }
    }
    ranges.sort_by_key(|&(start, ..)| start);

    let mut spans: Vec<Span> = Vec::new();
    let mut cut = 0;
    for (s, e, style) in ranges {
        if s < cut {
            continue; // overlaps a range already emitted
        }
        push_plain(&mut spans, &text[cut..s], base);
        spans.push(Span::styled(text[s..e].to_string(), style));
        cut = e;
    }
    push_plain(&mut spans, &text[cut..], base);
    Line::from(spans)
}

/// Scan for a leading bullet-plus-checkbox at the very start of `text` (after indentation):
/// `-`/`+`/`*` or an ordered `1.`/`1)` bullet, one space, then `[ ]`/`[X]`/`[x]`/`[-]`. A
/// checkbox only counts when its closing bracket is followed by a space or the end of the
/// line — `- [ ]x` is glued-on content, not a checkbox (same rule `list::parse_checkbox`
/// enforces on the parsing side). Returns the byte range of the bracketed token and whether
/// it reads as checked (`X`/`x`) — unchecked and partial (`-`) both render dimmed.
fn find_checkbox_token(text: &str) -> Option<(std::ops::Range<usize>, bool)> {
    let indent = text.chars().take_while(|&c| c == ' ' || c == '\t').count();
    let after = &text[indent..];
    let mut chars = after.chars();
    let first = chars.next()?;
    let bullet_len = if first == '-' || first == '+' || first == '*' {
        1
    } else if first.is_ascii_digit() {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        match after[digits.len()..].chars().next() {
            Some('.') | Some(')') => digits.len() + 1,
            _ => return None,
        }
    } else {
        return None;
    };
    if !after[bullet_len..].starts_with(' ') {
        return None;
    }
    let content_start = indent + bullet_len + 1;
    let rest = &text[content_start..];
    let mut rc = rest.chars();
    if rc.next() != Some('[') {
        return None;
    }
    let checked = match rc.next()? {
        ' ' | '-' => false,
        'X' | 'x' => true,
        _ => return None,
    };
    if rc.next() != Some(']') {
        return None;
    }
    match rest[3..].chars().next() {
        None | Some(' ') => Some((content_start..content_start + 3, checked)),
        Some(_) => None,
    }
}

/// Cheap check for whether `text` is eligible for cookie styling at all: a line that begins
/// (after indentation) with a Markdown/Org heading marker (a run of `#` or `*` followed by a
/// space) or a list bullet (`-`, `+`, `*`, or an ordered `1.`/`1)` marker, each followed by a
/// space). This keeps cookie styling off arbitrary prose that happens to contain `[2/3]`.
fn line_has_list_or_heading_prefix(text: &str) -> bool {
    let trimmed = text.trim_start_matches([' ', '\t']);
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(lead @ ('#' | '*')) => {
            let run = trimmed.chars().take_while(|&c| c == lead).count();
            trimmed[run..].starts_with(' ')
        }
        Some('-') | Some('+') => trimmed[1..].starts_with(' '),
        Some(c) if c.is_ascii_digit() => {
            let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
            let rest = &trimmed[digits.len()..];
            matches!(rest.chars().next(), Some('.') | Some(')')) && rest[1..].starts_with(' ')
        }
        _ => false,
    }
}

/// Every `[n/m]` / `[p%]` statistics-cookie token in `text` (filled forms only — `[/]`/`[%]`
/// are the empty forms torg never renders specially), as `(byte_range, complete)`.
/// `n/m` is complete when `m > 0` and `n == m`; a percentage is complete at exactly `100`.
fn find_cookie_tokens(text: &str) -> Vec<(std::ops::Range<usize>, bool)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some((len, complete)) = parse_cookie_token(&text[i..]) {
                out.push((i..i + len, complete));
                i += len;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Parse a single cookie token starting at `s[0] == '['`. Returns its byte length (including
/// both brackets) and whether it reads as complete.
fn parse_cookie_token(s: &str) -> Option<(usize, bool)> {
    let digits_from = |from: usize| -> String { s[from..].chars().take_while(char::is_ascii_digit).collect() };
    let n_str = digits_from(1);
    if n_str.is_empty() {
        return None;
    }
    let n: u64 = n_str.parse().ok()?;
    let after_n = 1 + n_str.len();
    match s[after_n..].chars().next() {
        Some('%') => {
            let close = after_n + 1;
            s[close..].starts_with(']').then_some((close + 1, n == 100))
        }
        Some('/') => {
            let m_start = after_n + 1;
            let m_str = digits_from(m_start);
            if m_str.is_empty() {
                return None;
            }
            let m: u64 = m_str.parse().ok()?;
            let close = m_start + m_str.len();
            s[close..].starts_with(']').then_some((close + 1, m > 0 && n == m))
        }
        _ => None,
    }
}

/// Render one line during search: every `query` occurrence styled, the one starting at
/// `current` (char col) in the current-match style. Falls back to a single plain span.
fn search_line(text: &str, query: &str, current: Option<usize>, base: Style) -> Line<'static> {
    let cols = torg_core::search::matches_in_line(text, query);
    if cols.is_empty() {
        return Line::from(Span::styled(text.to_string(), base));
    }
    let match_style = base.add_modifier(Modifier::REVERSED);
    let current_style = base.add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let chars: Vec<char> = text.chars().collect();
    let qlen = query.chars().count();
    let mut spans = Vec::new();
    let mut at = 0usize;
    for col in cols {
        if col < at {
            continue; // overlapping match already covered
        }
        if col > at {
            spans.push(Span::styled(chars[at..col].iter().collect::<String>(), base));
        }
        let style = if current == Some(col) { current_style } else { match_style };
        spans.push(Span::styled(chars[col..col + qlen].iter().collect::<String>(), style));
        at = col + qlen;
    }
    if at < chars.len() {
        spans.push(Span::styled(chars[at..].iter().collect::<String>(), base));
    }
    Line::from(spans)
}

fn push_plain(spans: &mut Vec<Span<'static>>, text: &str, base: Style) {
    if !text.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
    }
}

/// The status-line text: the Save-As prompt, or `[i/n] name[*] — line:col` plus any transient
/// message (the `[i/n]` buffer position appears only when more than one file is open).
fn status_text(app: &App) -> String {
    // A completion hint / error appended to a path prompt, when present.
    let hint = if app.status().is_empty() {
        String::new()
    } else {
        format!("   {}", app.status())
    };
    match app.mode() {
        Mode::SaveAs { input } => return format!("Save as: {input}{hint}"),
        Mode::OpenFile { input } => return format!("Open: {input}{hint}"),
        Mode::EditTags { input } => return format!("Tags: {input}"),
        Mode::DatePrompt { input, purpose } => {
            return format!("{}{input}", date_prompt_label(*purpose))
        }
        Mode::Search { input, .. } => return format!("Find: {input}{hint}"),
        Mode::BufferList { .. } => {
            return " Buffers — ↑/↓ or 1-9 select · Enter switch · Esc cancel ".to_string()
        }
        Mode::HelpMenu { .. } => {
            return " ←/→ category · ↑/↓ select · Enter run · Esc close ".to_string()
        }
        Mode::ConfirmClose => {
            let (name, _) = app.buffer_labels().swap_remove(app.active_index());
            return format!(" Discard unsaved changes to {name}? (y/n) ");
        }
        Mode::ConfirmQuit => {
            let dirty = app.buffer_labels().iter().filter(|(_, d)| *d).count();
            return format!(" {dirty} buffer(s) have unsaved changes — quit anyway? (y/n) ");
        }
        Mode::Edit => {}
    }
    let position = if app.buffer_count() > 1 {
        format!("[{}/{}] ", app.active_index() + 1, app.buffer_count())
    } else {
        String::new()
    };
    let (name, is_dirty) = app.buffer_labels().swap_remove(app.active_index());
    let dirty = if is_dirty { "*" } else { "" };
    let line = app.view().cursor_line() + 1;
    let col = app.view().cursor_column() + 1;
    let mut text = format!(" {position}{name}{dirty} — {line}:{col} ");
    if !app.status().is_empty() {
        text.push_str("  ");
        text.push_str(app.status());
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use torg_core::document::Document;

    #[test]
    fn help_rows_show_tabs_rows_and_selection() {
        let lines = help_menu_lines(1, 0, 80); // category 1 = Navigate, row 0 selected
        let tabs = &lines[0];
        let tab_text: String = tabs.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(tab_text.contains("File") && tab_text.contains("Navigate"));
        // Navigate's active tab styled differently from File's.
        assert_ne!(tabs.spans[0].style, tabs.spans[2].style);
        let first_row: String = lines[2].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_row.contains("Ctrl+N") && first_row.contains("Next heading"));
    }

    #[test]
    fn expand_tabs_advances_to_the_next_tab_stop() {
        assert_eq!(expand_tabs("\tx"), "    x"); // tab at col 0 → stop at 4
        assert_eq!(expand_tabs("ab\tx"), "ab  x"); // tab at col 2 → stop at 4
        assert_eq!(expand_tabs("abcd\tx"), "abcd    x"); // tab at a stop → full width
        assert_eq!(expand_tabs("a\tb\tc"), "a   b   c"); // multiple tabs
        assert_eq!(expand_tabs("no tabs"), "no tabs");
    }

    #[test]
    fn display_col_maps_char_columns_through_tabs() {
        // "\tx": char 0 is the tab (display 0), char 1 is 'x' at display 4.
        assert_eq!(display_col("\tx", 0), 0);
        assert_eq!(display_col("\tx", 1), 4);
        assert_eq!(display_col("\tx", 2), 5); // end of line
        // "ab\tx": 'x' is char 3, display 4.
        assert_eq!(display_col("ab\tx", 3), 4);
        // No tabs: identity.
        assert_eq!(display_col("plain", 3), 3);
    }

    #[test]
    fn highlight_line_splits_out_timestamps_and_keeps_the_full_text() {
        let line = highlight_line("SCHEDULED: <2024-01-15 Mon>", Style::default());
        // The reassembled spans equal the original text (nothing dropped or duplicated).
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "SCHEDULED: <2024-01-15 Mon>");
        // At least three spans: the keyword, the gap, and the timestamp.
        assert!(line.spans.len() >= 3);
    }

    #[test]
    fn highlight_line_leaves_plain_text_as_one_span() {
        let line = highlight_line("just prose", Style::default());
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "just prose");
    }

    #[test]
    fn highlight_line_styles_a_checked_checkbox_like_done_and_reassembles() {
        let line = highlight_line("- [X] buy milk", Style::default());
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "- [X] buy milk");
        let checkbox_span = line.spans.iter().find(|s| s.content.as_ref() == "[X]").unwrap();
        assert_ne!(checkbox_span.style, Style::default());
    }

    #[test]
    fn highlight_line_styles_a_lowercase_checked_checkbox_too() {
        let line = highlight_line("- [x] buy milk", Style::default());
        let checkbox_span = line.spans.iter().find(|s| s.content.as_ref() == "[x]").unwrap();
        assert_ne!(checkbox_span.style, Style::default());
    }

    #[test]
    fn highlight_line_dims_an_unchecked_or_partial_checkbox() {
        for text in ["- [ ] buy milk", "- [-] buy milk"] {
            let line = highlight_line(text, Style::default());
            let token = &text[2..5];
            let span = line.spans.iter().find(|s| s.content.as_ref() == token).unwrap();
            assert!(span.style.add_modifier.contains(Modifier::DIM), "{text}");
        }
    }

    #[test]
    fn highlight_line_does_not_style_a_checkbox_with_glued_on_content() {
        // "- [ ]x" has no space (or EOL) after the closing bracket, so it's not a checkbox —
        // same rule `list::parse_checkbox` enforces when parsing for real edits.
        let line = highlight_line("- [ ]x", Style::default());
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].style, Style::default());
    }

    #[test]
    fn highlight_line_styles_a_complete_cookie_on_a_heading_like_done() {
        let line = highlight_line("* Groceries [3/3]", Style::default());
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "* Groceries [3/3]");
        let cookie = line.spans.iter().find(|s| s.content.as_ref() == "[3/3]").unwrap();
        assert_ne!(cookie.style, Style::default());
    }

    #[test]
    fn highlight_line_styles_an_incomplete_cookie_differently_from_a_complete_one() {
        let incomplete = highlight_line("* Groceries [1/3]", Style::default());
        let complete = highlight_line("* Groceries [3/3]", Style::default());
        let incomplete_style = incomplete.spans.iter().find(|s| s.content.as_ref() == "[1/3]").unwrap().style;
        let complete_style = complete.spans.iter().find(|s| s.content.as_ref() == "[3/3]").unwrap().style;
        assert_ne!(incomplete_style, complete_style);
    }

    #[test]
    fn highlight_line_treats_a_percent_cookie_of_100_as_complete() {
        let line = highlight_line("- item [100%]", Style::default());
        let cookie = line.spans.iter().find(|s| s.content.as_ref() == "[100%]").unwrap();
        let other = highlight_line("- item [50%]", Style::default());
        let other_cookie = other.spans.iter().find(|s| s.content.as_ref() == "[50%]").unwrap();
        assert_ne!(cookie.style, other_cookie.style);
    }

    #[test]
    fn highlight_line_treats_zero_of_zero_as_incomplete() {
        // update_cookies writes [0/0] for a countable-but-empty list; per the styling rule
        // (m > 0 required for "complete"), that renders in the incomplete style, not done.
        let zero = highlight_line("* Groceries [0/0]", Style::default());
        let complete = highlight_line("* Groceries [3/3]", Style::default());
        let zero_style = zero.spans.iter().find(|s| s.content.as_ref() == "[0/0]").unwrap().style;
        let complete_style = complete.spans.iter().find(|s| s.content.as_ref() == "[3/3]").unwrap().style;
        assert_ne!(zero_style, complete_style);
    }

    #[test]
    fn highlight_line_does_not_style_a_cookie_shaped_token_in_plain_prose() {
        let line = highlight_line("see section [2/3] of the report", Style::default());
        assert!(line.spans.iter().all(|s| s.style == Style::default()));
    }

    #[test]
    fn highlight_line_styles_cookies_on_markdown_gfm_items_and_headings() {
        let heading = highlight_line("# Groceries [0/2]", Style::default());
        assert!(heading.spans.iter().any(|s| s.content.as_ref() == "[0/2]" && s.style != Style::default()));
        let item = highlight_line("- [ ] milk", Style::default());
        let dim = item.spans.iter().find(|s| s.content.as_ref() == "[ ]").unwrap();
        assert!(dim.style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn search_line_styles_every_occurrence_and_the_current_one_distinctly() {
        let line = search_line("foo bar foo", "foo", Some(8), Style::default());
        let spans: Vec<&str> = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(spans, vec!["foo", " bar ", "foo"]);
        assert_ne!(line.spans[0].style, line.spans[1].style);
        assert_ne!(line.spans[0].style, line.spans[2].style); // current ≠ other matches
    }

    #[test]
    fn search_line_without_matches_is_one_plain_span() {
        let line = search_line("nothing here", "zzz", None, Style::default());
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn status_line_shows_the_find_prompt_hint_while_searching() {
        let mut app = App::new(vec![Buffer::new(Document::from_text("plain\n"), None)]);
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        for c in "zzz".chars() {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        let text = status_text(&app);
        assert!(text.contains("Find: zzz"), "{text}");
        assert!(text.contains("Not found"), "{text}");
    }

    #[test]
    fn status_shows_the_buffer_position_only_with_multiple_buffers() {
        let one = App::new(vec![Buffer::new(Document::from_text("x"), None)]);
        assert!(!status_text(&one).contains("[1/1]"));

        let two = App::new(vec![
            Buffer::new(Document::from_text("x"), None),
            Buffer::new(Document::from_text("y"), None),
        ]);
        assert!(status_text(&two).starts_with(" [1/2]"));
    }
}
