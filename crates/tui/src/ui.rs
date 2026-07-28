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

/// Build a display line from already-tab-expanded `text`, styling any timestamps and the
/// `SCHEDULED:`/`DEADLINE:` planning keywords over the `base` style. Timestamps carry no
/// tabs, so the byte ranges from `find_timestamps` line up with the expanded text.
fn highlight_line(text: &str, base: Style) -> Line<'static> {
    let ts_style = base.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED);
    let kw_style = base.fg(Color::Yellow);
    let mut spans: Vec<Span> = Vec::new();
    let mut cut = 0;
    // Planning keywords first (they precede their timestamps on the line).
    for kw in ["SCHEDULED:", "DEADLINE:"] {
        if let Some(i) = text.find(kw) {
            push_plain(&mut spans, &text[cut..i.max(cut)], base);
            if i >= cut {
                spans.push(Span::styled(kw.to_string(), kw_style));
                cut = i + kw.len();
            }
        }
    }
    for (s, e) in find_timestamps(text) {
        if s < cut {
            continue; // already inside an emitted span
        }
        push_plain(&mut spans, &text[cut..s], base);
        spans.push(Span::styled(text[s..e].to_string(), ts_style));
        cut = e;
    }
    push_plain(&mut spans, &text[cut..], base);
    Line::from(spans)
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
