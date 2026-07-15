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
        lines.push(highlight_line(&text, base));
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
        // No cursor while picking from the buffer list or answering a confirmation.
        Mode::BufferList { .. } | Mode::ConfirmClose | Mode::ConfirmQuit => {}
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
        Mode::BufferList { .. } => {
            return " Buffers — ↑/↓ or 1-9 select · Enter switch · Esc cancel ".to_string()
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
