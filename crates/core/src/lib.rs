//! torg-core — the UI-agnostic heart of the editor.
//!
//! Mirrors gedit's model layer (Document / View / Tab / Window / App + commands)
//! with no dependency on any frontend. Everything here is unit-testable headless.

pub mod document;
pub mod list;
pub mod search;
pub mod structure;
pub mod timestamp;
pub mod view;

pub use list::{
    indent_item, insert_item, item_at, toggle_checkbox, update_cookies, Bullet, CheckState,
    ListItem,
};
pub use search::{find, matches_in_line, Match};
