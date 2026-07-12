//! One open file and everything the editor remembers about it — content, cursor, folds,
//! outline cache, scroll position, and where a not-yet-existing file should first save.
//!
//! This is presentation-plus-document state, so it lives in the TUI state tier (see
//! `docs/architecture.md`): the core stays a single-`Document` model, and the multi-file
//! machinery is a `Vec<Buffer>` on the [`App`](crate::app::App).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use textr_org_core::document::Document;
use textr_org_core::structure::{Outline, OrgProvider, StructureProvider};
use textr_org_core::view::View;

/// A single open file: the document plus all of its per-buffer editor state. Switching
/// buffers swaps the whole `Buffer`, so cursor, folds, and scroll survive a round trip.
pub struct Buffer {
    pub(crate) doc: Document,
    pub(crate) view: View,
    /// Heading start-lines that are currently collapsed.
    pub(crate) folded: HashSet<usize>,
    /// Outline cache, re-derived after every edit so fold ranges stay correct.
    pub(crate) outline: Outline,
    /// Top document line of the viewport (updated before each render).
    pub(crate) scroll_top: usize,
    /// For a buffer opened on a not-yet-existing path: where the first save should go.
    pub(crate) stash_path: Option<PathBuf>,
}

impl Buffer {
    /// Build a buffer over `doc`. `stash_path` is `Some` when the file did not exist yet —
    /// the first save writes there without prompting.
    pub fn new(doc: Document, stash_path: Option<PathBuf>) -> Self {
        let outline = OrgProvider.parse(&doc);
        Self {
            doc,
            view: View::new(),
            folded: HashSet::new(),
            outline,
            scroll_top: 0,
            stash_path,
        }
    }

    /// An untitled, empty buffer.
    pub fn untitled() -> Self {
        Self::new(Document::new(), None)
    }

    /// The name shown in the status line and buffer list: the file name of the document's
    /// path, else of the stashed first-save path, else `[No Name]`.
    pub fn display_name(&self) -> String {
        self.doc
            .path()
            .or(self.stash_path.as_deref())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "[No Name]".to_string())
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.is_modified()
    }

    /// Whether this buffer is the one identified by `path` — its document path or its
    /// stashed first-save path.
    pub fn matches_path(&self, path: &Path) -> bool {
        self.doc
            .path()
            .map(|p| same_path(p, path))
            .unwrap_or(false)
            || self
                .stash_path
                .as_deref()
                .map(|p| same_path(p, path))
                .unwrap_or(false)
    }
}

/// Whether two paths name the same file: canonicalized comparison when both resolve (so
/// `a.org` and `./a.org` match), raw comparison otherwise (the only option for paths that
/// don't exist yet, e.g. stashed first-save targets).
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_prefers_the_document_path() {
        let path = std::env::temp_dir().join(format!("textr_buf_name_{}.org", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        let buf = Buffer::new(Document::open(&path).unwrap(), None);
        assert_eq!(buf.display_name(), path.file_name().unwrap().to_string_lossy());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn display_name_falls_back_to_the_stash_path_then_no_name() {
        let stash = Buffer::new(Document::new(), Some(PathBuf::from("todo/new.org")));
        assert_eq!(stash.display_name(), "new.org");
        assert_eq!(Buffer::untitled().display_name(), "[No Name]");
    }

    #[test]
    fn matches_path_sees_document_and_stash_paths() {
        let stash = Buffer::new(Document::new(), Some(PathBuf::from("missing/later.org")));
        assert!(stash.matches_path(Path::new("missing/later.org")));
        assert!(!stash.matches_path(Path::new("other.org")));
        assert!(!Buffer::untitled().matches_path(Path::new("anything.org")));
    }

    #[test]
    fn same_path_matches_different_spellings_of_an_existing_file() {
        let path = std::env::temp_dir().join(format!("textr_buf_same_{}.org", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        let dotted = std::env::temp_dir().join(".").join(path.file_name().unwrap());
        assert!(same_path(&path, &dotted));
        let _ = std::fs::remove_file(&path);
    }
}
