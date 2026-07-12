//! `torg` — the terminal frontend binary.
//!
//! Thin lifecycle glue: parse the file argument into an initial [`Document`], then hand off to
//! the terminal driver. All editor behaviour lives in the (tested) [`app`] state tier and in
//! `textr-org-core`; this file only wires them to a real terminal.

mod action;
mod app;
mod terminal;
mod ui;
mod viewport;

use std::path::PathBuf;
use std::process::ExitCode;

use textr_org_core::document::Document;

use crate::app::App;

fn main() -> ExitCode {
    let (doc, stash_path) = match load() {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("torg: {msg}");
            return ExitCode::FAILURE;
        }
    };
    let mut app = App::new(doc, stash_path);

    terminal::install_panic_hook();
    let mut term = match terminal::init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("torg: failed to start terminal: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = terminal::run(&mut term, &mut app);
    let _ = terminal::restore(); // always restore, even if the loop errored

    if let Err(e) = result {
        eprintln!("torg: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Resolve the command-line argument into a document and, for a not-yet-existing path, the
/// path to stash for the first save.
///
/// - a path that exists → open it;
/// - a path that does not exist → an empty buffer that will save there;
/// - no argument → an untitled buffer.
fn load() -> Result<(Document, Option<PathBuf>), String> {
    match std::env::args().nth(1) {
        Some(arg) => {
            let path = PathBuf::from(arg);
            if path.exists() {
                let doc = Document::open(&path)
                    .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
                Ok((doc, None))
            } else {
                Ok((Document::new(), Some(path)))
            }
        }
        None => Ok((Document::new(), None)),
    }
}
