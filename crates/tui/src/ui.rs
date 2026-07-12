//! Rendering — a pure view of [`App`] into a ratatui `Frame`. No crossterm, no I/O: this
//! tier is driver-agnostic, so a future GUI could drive the same state through a different
//! backend. Everything here derives from `&App`; it never mutates.

use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Mode};

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
        if app.is_folded_heading(doc_line) {
            text.push_str(" …"); // a collapsed subtree
        }
        if doc_line == cursor_line {
            cursor = Some((app.view().cursor_column() as u16, lines.len() as u16));
        }
        let style = if is_heading(app, doc_line) {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::styled(text, style));
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
        // No cursor while picking from the buffer list or answering a confirmation.
        Mode::BufferList { .. } | Mode::ConfirmClose | Mode::ConfirmQuit => {}
    }
}

fn is_heading(app: &App, line: usize) -> bool {
    app.outline().headings.iter().any(|h| h.line == line)
}

/// The status-line text: the Save-As prompt, or `[i/n] name[*] — line:col` plus any transient
/// message (the `[i/n]` buffer position appears only when more than one file is open).
fn status_text(app: &App) -> String {
    match app.mode() {
        Mode::SaveAs { input } => return format!("Save as: {input}"),
        Mode::OpenFile { input } => return format!("Open: {input}"),
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
    use textr_org_core::document::Document;

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
