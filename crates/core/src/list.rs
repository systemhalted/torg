//! Plain lists: bullets, checkboxes, statistics cookies, and their edits.
//! Free functions over `Document` + `Format`, like `timestamp` and `search` — list syntax
//! differs per format by only the bullet rules, so there is no new trait.

use crate::document::Document;
use crate::structure::{line_in_fence, Format};

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
pub fn item_at(doc: &Document, line: usize, format: Format) -> Option<ListItem> {
    if line >= doc.line_count() {
        return None;
    }
    if format == Format::Markdown && line_in_fence(doc, line) {
        return None;
    }
    let raw = doc.line_text(line);
    let text = raw.strip_suffix('\n').unwrap_or(&raw);
    parse_item(text, line, format)
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
/// if present) consumed. `None` if `rest` doesn't open with a recognized checkbox token.
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
    // "[ ]" is 3 chars; one more if a space follows (content resumes after it).
    let trailing_space = rest[3..].starts_with(' ');
    Some((state, if trailing_space { 4 } else { 3 }))
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
}
