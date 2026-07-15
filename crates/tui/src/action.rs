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
    // structure
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
    // structural editing
    PromoteHeading,
    DemoteHeading,
    PromoteSubtree,
    DemoteSubtree,
    MoveSubtreeUp,
    MoveSubtreeDown,
    InsertSibling,
    InsertTodoSibling,
    PriorityUp,
    PriorityDown,
    EditTags,
    // dates
    SetScheduled,
    SetDeadline,
    InsertActiveTs,
    InsertInactiveTs,
    // documentation
    Help,
    Guide,
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
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        // Structural-editing chords come before the plain arms they modify.
        KeyCode::Left if alt && shift => Some(Action::PromoteSubtree),
        KeyCode::Right if alt && shift => Some(Action::DemoteSubtree),
        KeyCode::Left if alt => Some(Action::PromoteHeading),
        KeyCode::Right if alt => Some(Action::DemoteHeading),
        KeyCode::Up if alt => Some(Action::MoveSubtreeUp),
        KeyCode::Down if alt => Some(Action::MoveSubtreeDown),
        KeyCode::Up if shift => Some(Action::PriorityUp),
        KeyCode::Down if shift => Some(Action::PriorityDown),
        KeyCode::Enter if alt && shift => Some(Action::InsertTodoSibling),
        KeyCode::Enter if alt => Some(Action::InsertSibling),
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
            'g' => Some(Action::EditTags),
            // Help: `h` is the mnemonic (works where the terminal distinguishes Ctrl+H from
            // Backspace); `k` is the always-reliable fallback.
            'h' | 'k' => Some(Action::Help),
            'u' => Some(Action::Guide),
            _ => None,
        },
        // Alt chords: buffer commands (echoing Ctrl+N/P's heading navigation), plus the
        // TODO-sibling alias — Shift+Enter has no legacy escape sequence, so Alt+Shift+Enter
        // never reaches the app in many terminals.
        KeyCode::Char(c) if alt => match c {
            'n' => Some(Action::NextBuffer),
            'p' => Some(Action::PrevBuffer),
            't' => Some(Action::InsertTodoSibling),
            's' => Some(Action::SetScheduled),
            'd' => Some(Action::SetDeadline),
            '.' => Some(Action::InsertActiveTs),
            'i' => Some(Action::InsertInactiveTs),
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
    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }
    fn alt_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }
    fn alt_shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT)
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
    fn timestamp_command_chords_map() {
        assert_eq!(key_to_action(alt('s')), Some(Action::SetScheduled));
        assert_eq!(key_to_action(alt('d')), Some(Action::SetDeadline));
        assert_eq!(key_to_action(alt('.')), Some(Action::InsertActiveTs));
        assert_eq!(key_to_action(alt('i')), Some(Action::InsertInactiveTs));
    }

    #[test]
    fn shift_arrows_still_map_to_priority() {
        // Date-shift vs priority is decided in the app layer by cursor context, so the pure
        // keymap still yields the priority actions.
        assert_eq!(key_to_action(shift(KeyCode::Up)), Some(Action::PriorityUp));
        assert_eq!(key_to_action(shift(KeyCode::Down)), Some(Action::PriorityDown));
    }

    #[test]
    fn alt_t_is_the_reachable_todo_sibling_alias() {
        // Shift+Enter has no legacy escape sequence, so Alt+Shift+Enter never arrives in
        // many terminals; Alt+T must always work.
        assert_eq!(key_to_action(alt('t')), Some(Action::InsertTodoSibling));
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
    fn ctrl_h_and_ctrl_k_open_help_and_ctrl_u_the_guide() {
        assert_eq!(key_to_action(ctrl('h')), Some(Action::Help)); // primary, mnemonic
        assert_eq!(key_to_action(ctrl('k')), Some(Action::Help)); // portable fallback
        assert_eq!(key_to_action(ctrl('u')), Some(Action::Guide));
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
    fn structural_editing_chords_map() {
        assert_eq!(key_to_action(alt_key(KeyCode::Left)), Some(Action::PromoteHeading));
        assert_eq!(key_to_action(alt_key(KeyCode::Right)), Some(Action::DemoteHeading));
        assert_eq!(key_to_action(alt_shift(KeyCode::Left)), Some(Action::PromoteSubtree));
        assert_eq!(key_to_action(alt_shift(KeyCode::Right)), Some(Action::DemoteSubtree));
        assert_eq!(key_to_action(alt_key(KeyCode::Up)), Some(Action::MoveSubtreeUp));
        assert_eq!(key_to_action(alt_key(KeyCode::Down)), Some(Action::MoveSubtreeDown));
        assert_eq!(key_to_action(alt_key(KeyCode::Enter)), Some(Action::InsertSibling));
        assert_eq!(key_to_action(alt_shift(KeyCode::Enter)), Some(Action::InsertTodoSibling));
        assert_eq!(key_to_action(shift(KeyCode::Up)), Some(Action::PriorityUp));
        assert_eq!(key_to_action(shift(KeyCode::Down)), Some(Action::PriorityDown));
        assert_eq!(key_to_action(ctrl('g')), Some(Action::EditTags));
    }

    #[test]
    fn plain_arrows_and_enter_still_map_to_movement_and_newline() {
        assert_eq!(key_to_action(press(KeyCode::Left)), Some(Action::MoveLeft)); // regression
        assert_eq!(key_to_action(press(KeyCode::Enter)), Some(Action::Newline));
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
