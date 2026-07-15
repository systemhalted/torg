//! UniFFI surface exposing `torg-core` to Swift (and other UniFFI languages).
//!
//! This is the thin bridge the iOS/iPad frontend calls across the FFI boundary. It is
//! deliberately small — enough to prove the pipeline end to end: parse a document into an
//! outline (both formats), and mutate it (cycle a heading's TODO) so an edit round-trips.
//! Everything here delegates to `torg-core`; no editing logic lives in this crate.

use torg_core::document::Document;
use torg_core::structure::{Format, StructureProvider, TodoState};

uniffi::setup_scaffolding!();

/// One heading in a parsed document, flattened for the FFI (usize → u32).
#[derive(uniffi::Record)]
pub struct HeadingInfo {
    /// Nesting depth, 1-based (`*`/`#` = 1, `**`/`##` = 2, …).
    pub level: u32,
    /// The 0-based line the heading sits on.
    pub line: u32,
    /// The heading text, with markers, keyword, priority cookie, and tags stripped.
    pub title: String,
    /// The last line of this heading's subtree — the frontend folds `line + 1 ..= last_line`.
    pub last_line: u32,
    /// `"TODO"` / `"DONE"` if the heading carries a keyword, else `None`.
    pub todo: Option<String>,
    /// `"A"`/`"B"`/`"C"` priority cookie, if present.
    pub priority: Option<String>,
    /// The heading's `:tags:`, colons stripped.
    pub tags: Vec<String>,
}

fn format(markdown: bool) -> Format {
    if markdown {
        Format::Markdown
    } else {
        Format::Org
    }
}

/// Parse `text` into its outline. `markdown` selects the Markdown provider; otherwise Org.
#[uniffi::export]
pub fn outline(text: String, markdown: bool) -> Vec<HeadingInfo> {
    let doc = Document::from_text(&text);
    format(markdown)
        .parse(&doc)
        .headings
        .into_iter()
        .map(|h| HeadingInfo {
            level: h.level as u32,
            line: h.line as u32,
            title: h.title,
            last_line: h.last_line as u32,
            todo: h.todo.map(|t| match t {
                TodoState::Todo => "TODO".to_string(),
                TodoState::Done => "DONE".to_string(),
            }),
            priority: h.priority.map(|c| c.to_string()),
            tags: h.tags,
        })
        .collect()
}

/// Cycle the TODO keyword on the heading at `line` (none → TODO → DONE → none) and return the
/// updated document text. A no-op if `line` is not a heading.
#[uniffi::export]
pub fn cycle_todo(text: String, line: u32, markdown: bool) -> String {
    let mut doc = Document::from_text(&text);
    format(markdown).cycle_todo(&mut doc, line as usize);
    doc.text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_reports_headings_for_org() {
        let o = outline("* A\nbody\n** B\n".to_string(), false);
        assert_eq!(o.len(), 2);
        assert_eq!((o[0].level, o[0].title.as_str(), o[0].last_line), (1, "A", 2));
        assert_eq!((o[1].level, o[1].title.as_str(), o[1].line), (2, "B", 2));
    }

    #[test]
    fn outline_reports_metadata() {
        let o = outline("* TODO [#A] ship :work:\n".to_string(), false);
        assert_eq!(o[0].todo.as_deref(), Some("TODO"));
        assert_eq!(o[0].priority.as_deref(), Some("A"));
        assert_eq!(o[0].tags, vec!["work"]);
        assert_eq!(o[0].title, "ship");
    }

    #[test]
    fn outline_uses_the_markdown_provider_when_asked() {
        assert!(outline("* org\n".to_string(), true).is_empty()); // no `#` heading
        assert_eq!(outline("# md\n".to_string(), true)[0].title, "md");
    }

    #[test]
    fn cycle_todo_round_trips_through_the_ffi() {
        let t = cycle_todo("* task\n".to_string(), 0, false);
        assert_eq!(t, "* TODO task\n");
        let t = cycle_todo(t, 0, false);
        assert_eq!(t, "* DONE task\n");
        // Markdown works the same.
        assert_eq!(cycle_todo("# task\n".to_string(), 0, true), "# TODO task\n");
    }
}
