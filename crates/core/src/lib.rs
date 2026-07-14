//! textr-org-core — the UI-agnostic heart of the editor.
//!
//! Mirrors gedit's model layer (Document / View / Tab / Window / App + commands)
//! with no dependency on any frontend. Everything here is unit-testable headless.

pub mod document;
pub mod structure;
pub mod timestamp;
pub mod view;
