//! `torg` — the terminal frontend binary.
//!
//! Thin lifecycle glue: parse the file argument into an initial [`Document`], then hand off to
//! the terminal driver. All editor behaviour lives in the (tested) [`app`] state tier and in
//! `torg-core`; this file only wires them to a real terminal.

mod action;
mod app;
mod buffer;
mod terminal;
mod ui;
mod viewport;

use std::path::PathBuf;
use std::process::ExitCode;

use torg_core::document::Document;

use crate::app::App;
use crate::buffer::Buffer;

/// `torg --help` text.
const HELP: &str = "\
torg — an Org-mode–flavored terminal text editor

USAGE:
    torg [OPTIONS] [FILE]...

ARGS:
    <FILE>...    Files to open. An existing file is loaded; a path that does
                 not exist yet is created on first save. With no file, torg
                 starts with an empty, untitled buffer. Use `--` before a name
                 that begins with `-`.

OPTIONS:
    -h, --help       Print this help and exit
    -V, --version    Print version and exit

IN THE EDITOR:
    Ctrl+Q  quit        Ctrl+O  open a file      Ctrl+S  save
    Ctrl+H  key reference (quick help; Ctrl+K if your terminal eats Ctrl+H)
    Ctrl+U  full guide

Files ending in .md or .markdown are treated as Markdown; everything else as Org.
Full guide: https://github.com/systemhalted/torg/blob/main/docs/guide.md
";

/// The outcome of parsing the command line.
#[derive(Debug)]
enum Cli {
    Help,
    Version,
    Files(Vec<String>),
    Unknown(String),
}

/// Split arguments (without the program name) into an action. `-h`/`--help` and
/// `-V`/`--version` win, first seen; `--` ends option parsing; any other `-`-leading token
/// before `--` is an unknown option; everything else is a file path.
fn parse_cli(raw: Vec<String>) -> Cli {
    let mut files = Vec::new();
    let mut end_of_opts = false;
    for arg in raw {
        if end_of_opts {
            files.push(arg);
            continue;
        }
        match arg.as_str() {
            "--" => end_of_opts = true,
            "-h" | "--help" => return Cli::Help,
            "-V" | "--version" => return Cli::Version,
            a if a.len() > 1 && a.starts_with('-') => return Cli::Unknown(arg),
            _ => files.push(arg),
        }
    }
    Cli::Files(files)
}

fn main() -> ExitCode {
    let files = match parse_cli(std::env::args().skip(1).collect()) {
        Cli::Help => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Cli::Version => {
            println!("torg {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Cli::Unknown(opt) => {
            eprintln!("torg: unknown option '{opt}'\nTry 'torg --help' for usage.");
            return ExitCode::from(2);
        }
        Cli::Files(files) => files,
    };

    let buffers = match load(&files) {
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
fn load(files: &[String]) -> Result<Vec<Buffer>, String> {
    let mut buffers: Vec<Buffer> = Vec::new();
    for arg in files {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        parse_cli(args.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn help_and_version_flags_win_over_files() {
        assert!(matches!(cli(&["--help"]), Cli::Help));
        assert!(matches!(cli(&["-h"]), Cli::Help));
        assert!(matches!(cli(&["--version"]), Cli::Version));
        assert!(matches!(cli(&["-V"]), Cli::Version));
        // A flag anywhere in the list still triggers.
        assert!(matches!(cli(&["a.org", "--help"]), Cli::Help));
        // First flag seen wins.
        assert!(matches!(cli(&["--version", "--help"]), Cli::Version));
    }

    #[test]
    fn bare_args_become_files() {
        match cli(&[]) {
            Cli::Files(f) => assert!(f.is_empty()),
            other => panic!("{other:?}"),
        }
        match cli(&["a.org", "b.md"]) {
            Cli::Files(f) => assert_eq!(f, ["a.org", "b.md"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_option_is_reported() {
        match cli(&["--bogus"]) {
            Cli::Unknown(o) => assert_eq!(o, "--bogus"),
            other => panic!("{other:?}"),
        }
        match cli(&["-x"]) {
            Cli::Unknown(o) => assert_eq!(o, "-x"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn double_dash_ends_option_parsing() {
        // A file literally named like a flag is reachable after `--`.
        match cli(&["--", "-weird.org"]) {
            Cli::Files(f) => assert_eq!(f, ["-weird.org"]),
            other => panic!("{other:?}"),
        }
        // `--help` after `--` is a filename, not the flag.
        match cli(&["--", "--help"]) {
            Cli::Files(f) => assert_eq!(f, ["--help"]),
            other => panic!("{other:?}"),
        }
    }
}
