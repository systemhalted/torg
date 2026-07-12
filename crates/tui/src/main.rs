//! `torg` — the terminal frontend binary.
//!
//! Thin lifecycle glue: parse the file argument into an initial [`Document`], then hand off to
//! the terminal driver. All editor behaviour lives in the (tested) [`app`] state tier and in
//! `textr-org-core`; this file only wires them to a real terminal.

mod action;
mod app;
mod buffer;
mod terminal;
mod ui;
mod viewport;

use std::path::PathBuf;
use std::process::ExitCode;

use textr_org_core::document::Document;

use crate::app::App;
use crate::buffer::Buffer;

fn main() -> ExitCode {
    let buffers = match load() {
        Ok(buffers) => buffers,
        Err(msg) => {
            eprintln!("torg: {msg}");
            return ExitCode::FAILURE;
        }
    };
    let mut app = App::new(buffers);

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

/// Resolve the command-line arguments into the initial buffers, one per path, first active.
///
/// - a path that exists → open it;
/// - a path that does not exist → an empty buffer that will save there;
/// - a path given twice → one buffer;
/// - no arguments → a single untitled buffer (`App::new` supplies it).
fn load() -> Result<Vec<Buffer>, String> {
    let mut buffers: Vec<Buffer> = Vec::new();
    for arg in std::env::args().skip(1) {
        let path = PathBuf::from(arg);
        if buffers.iter().any(|b| b.matches_path(&path)) {
            continue;
        }
        if path.exists() {
            let doc = Document::open(&path)
                .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
            buffers.push(Buffer::new(doc, None));
        } else {
            buffers.push(Buffer::new(Document::new(), Some(path)));
        }
    }
    Ok(buffers)
}
