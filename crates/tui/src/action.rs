//! The editor's vocabulary of intents, and the pure mapping from a key press to one.
//!
//! `key_to_action` is deliberately a free function of `KeyEvent → Option<Action>` with no
//! terminal I/O, so it lives in the tested state tier: swapping the terminal driver (e.g. for
//! a GUI) changes only how `KeyEvent`s are produced, never this mapping.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// A single editor intent, decoded from a key press and applied to the `App`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // movement
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveHome,
    MoveEnd,
    PageUp,
    PageDown,
    // editing
    InsertChar(char),
    Newline,
    Backspace,
    Delete,
    // file
    Save,
    Quit,
    // structure (Org)
    ToggleFold,
    NextHeading,
    PrevHeading,
    CycleTodo,
    // buffers
    OpenFile,
    NextBuffer,
    PrevBuffer,
    ListBuffers,
    CloseBuffer,
}

/// Map a key press to an [`Action`], or `None` if the key is unbound.
///
/// Key *releases* and *repeats* are ignored (only `Press` maps), which also avoids the
/// Windows double-fire where a press and a release both arrive. The structure keymap is
/// provisional — real Org chords (`C-c C-t`, …) are a later concern.
pub fn key_to_action(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Left => Some(Action::MoveLeft),
        KeyCode::Right => Some(Action::MoveRight),
        KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Home => Some(Action::MoveHome),
        KeyCode::End => Some(Action::MoveEnd),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::Enter => Some(Action::Newline),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Delete => Some(Action::Delete),
        KeyCode::Tab => Some(Action::ToggleFold),
        // Ctrl chords: file + structure commands.
        KeyCode::Char(c) if ctrl => match c {
            's' => Some(Action::Save),
            'q' => Some(Action::Quit),
            't' => Some(Action::CycleTodo),
            'n' => Some(Action::NextHeading),
            'p' => Some(Action::PrevHeading),
            'o' => Some(Action::OpenFile),
            'b' => Some(Action::ListBuffers),
            'w' => Some(Action::CloseBuffer),
            _ => None,
        },
        // Alt chords: buffer commands (echoing Ctrl+N/P's heading navigation).
        KeyCode::Char(c) if alt => match c {
            'n' => Some(Action::NextBuffer),
            'p' => Some(Action::PrevBuffer),
            _ => None,
        },
        // Any other printable char (incl. Shift for capitals) is inserted.
        KeyCode::Char(c) => Some(Action::InsertChar(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
    }

    #[test]
    fn arrows_and_paging_map_to_moves() {
        assert_eq!(key_to_action(press(KeyCode::Left)), Some(Action::MoveLeft));
        assert_eq!(key_to_action(press(KeyCode::Up)), Some(Action::MoveUp));
        assert_eq!(key_to_action(press(KeyCode::PageDown)), Some(Action::PageDown));
        assert_eq!(key_to_action(press(KeyCode::Home)), Some(Action::MoveHome));
    }

    #[test]
    fn editing_keys_map() {
        assert_eq!(key_to_action(press(KeyCode::Enter)), Some(Action::Newline));
        assert_eq!(key_to_action(press(KeyCode::Backspace)), Some(Action::Backspace));
        assert_eq!(key_to_action(press(KeyCode::Delete)), Some(Action::Delete));
        assert_eq!(
            key_to_action(press(KeyCode::Char('a'))),
            Some(Action::InsertChar('a'))
        );
    }

    #[test]
    fn ctrl_chords_map_to_commands() {
        assert_eq!(key_to_action(ctrl('s')), Some(Action::Save));
        assert_eq!(key_to_action(ctrl('q')), Some(Action::Quit));
        assert_eq!(key_to_action(ctrl('t')), Some(Action::CycleTodo));
        assert_eq!(key_to_action(ctrl('n')), Some(Action::NextHeading));
        assert_eq!(key_to_action(ctrl('p')), Some(Action::PrevHeading));
    }

    #[test]
    fn tab_is_toggle_fold() {
        assert_eq!(key_to_action(press(KeyCode::Tab)), Some(Action::ToggleFold));
    }

    #[test]
    fn unbound_ctrl_chord_is_none() {
        assert_eq!(key_to_action(ctrl('a')), None);
    }

    #[test]
    fn alt_n_and_alt_p_cycle_buffers() {
        assert_eq!(key_to_action(alt('n')), Some(Action::NextBuffer));
        assert_eq!(key_to_action(alt('p')), Some(Action::PrevBuffer));
    }

    #[test]
    fn ctrl_o_opens_a_file() {
        assert_eq!(key_to_action(ctrl('o')), Some(Action::OpenFile));
    }

    #[test]
    fn ctrl_b_lists_buffers() {
        assert_eq!(key_to_action(ctrl('b')), Some(Action::ListBuffers));
    }

    #[test]
    fn ctrl_w_closes_the_buffer() {
        assert_eq!(key_to_action(ctrl('w')), Some(Action::CloseBuffer));
    }

    #[test]
    fn an_alt_modified_char_is_not_inserted() {
        assert_eq!(key_to_action(alt('x')), None);
    }

    #[test]
    fn key_release_is_ignored() {
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        assert_eq!(key_to_action(release), None);
    }
}
