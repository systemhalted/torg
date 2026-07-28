//! The single source of truth for user-facing commands: every chord the help menu
//! shows, grouped by category. A drift-guard test ties this table to the `Action`
//! enum so a new action cannot ship without a help entry (or an explicit exclusion).

use crate::action::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    File,
    Navigate,
    Structure,
    Dates,
    Search,
    Buffers,
    Help,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::File,
        Category::Navigate,
        Category::Structure,
        Category::Dates,
        Category::Search,
        Category::Buffers,
        Category::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::File => "File",
            Category::Navigate => "Navigate",
            Category::Structure => "Structure",
            Category::Dates => "Dates",
            Category::Search => "Search",
            Category::Buffers => "Buffers",
            Category::Help => "Help",
        }
    }
}

pub struct CommandInfo {
    pub action: Action,
    pub keys: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: Category,
}

/// Whether an [`Action`] must appear in [`COMMANDS`]. Exhaustive on purpose: adding an
/// `Action` variant fails compilation here until someone decides its help-menu fate.
///
/// Only exercised by tests below — it's a compile-time drift guard, not runtime logic, so it
/// has no caller outside `#[cfg(test)]`.
#[allow(dead_code)]
fn requires_entry(action: &Action) -> bool {
    match action {
        // Raw typing and plain cursor motion stay out of the menu by design.
        Action::MoveLeft
        | Action::MoveRight
        | Action::MoveUp
        | Action::MoveDown
        | Action::MoveHome
        | Action::MoveEnd
        | Action::PageUp
        | Action::PageDown
        | Action::InsertChar(_)
        | Action::Newline
        | Action::Backspace
        | Action::Delete => false,
        Action::Save
        | Action::Quit
        | Action::ToggleFold
        | Action::NextHeading
        | Action::PrevHeading
        | Action::CycleTodo
        | Action::OpenFile
        | Action::NextBuffer
        | Action::PrevBuffer
        | Action::ListBuffers
        | Action::CloseBuffer
        | Action::PromoteHeading
        | Action::DemoteHeading
        | Action::PromoteSubtree
        | Action::DemoteSubtree
        | Action::MoveSubtreeUp
        | Action::MoveSubtreeDown
        | Action::InsertSibling
        | Action::InsertTodoSibling
        | Action::PriorityUp
        | Action::PriorityDown
        | Action::EditTags
        | Action::ToggleCheckbox
        | Action::SetScheduled
        | Action::SetDeadline
        | Action::InsertActiveTs
        | Action::InsertInactiveTs
        | Action::Help
        | Action::Guide
        | Action::Find => true,
    }
}

pub static COMMANDS: &[CommandInfo] = &[
    // File
    CommandInfo { action: Action::Save, keys: "Ctrl+S", name: "Save", description: "Save the buffer (prompts for a path when it has none)", category: Category::File },
    CommandInfo { action: Action::OpenFile, keys: "Ctrl+O", name: "Open file", description: "Open a file in a new buffer (Tab completes the path)", category: Category::File },
    CommandInfo { action: Action::Quit, keys: "Ctrl+Q", name: "Quit", description: "Quit torg (asks about unsaved buffers)", category: Category::File },
    // Navigate
    CommandInfo { action: Action::NextHeading, keys: "Ctrl+N", name: "Next heading", description: "Jump to the next heading", category: Category::Navigate },
    CommandInfo { action: Action::PrevHeading, keys: "Ctrl+P", name: "Previous heading", description: "Jump to the previous heading", category: Category::Navigate },
    // Structure
    CommandInfo { action: Action::ToggleFold, keys: "Tab", name: "Fold / unfold", description: "Collapse or expand the current subtree", category: Category::Structure },
    CommandInfo { action: Action::CycleTodo, keys: "Ctrl+T", name: "Cycle TODO", description: "Cycle the heading's TODO state", category: Category::Structure },
    CommandInfo { action: Action::PromoteHeading, keys: "Alt+←", name: "Promote heading", description: "Raise the heading one level (children stay)", category: Category::Structure },
    CommandInfo { action: Action::DemoteHeading, keys: "Alt+→", name: "Demote heading", description: "Lower the heading one level (children stay)", category: Category::Structure },
    CommandInfo { action: Action::PromoteSubtree, keys: "Alt+Shift+←", name: "Promote subtree", description: "Raise the heading and its whole subtree", category: Category::Structure },
    CommandInfo { action: Action::DemoteSubtree, keys: "Alt+Shift+→", name: "Demote subtree", description: "Lower the heading and its whole subtree", category: Category::Structure },
    CommandInfo { action: Action::MoveSubtreeUp, keys: "Alt+↑", name: "Move subtree up", description: "Swap the subtree with the previous same-level sibling", category: Category::Structure },
    CommandInfo { action: Action::MoveSubtreeDown, keys: "Alt+↓", name: "Move subtree down", description: "Swap the subtree with the next same-level sibling", category: Category::Structure },
    CommandInfo { action: Action::InsertSibling, keys: "Alt+Enter", name: "Insert sibling", description: "Insert a sibling heading after the current subtree", category: Category::Structure },
    CommandInfo { action: Action::InsertTodoSibling, keys: "Alt+T", name: "Insert TODO sibling", description: "Insert a TODO sibling heading (also Alt+Shift+Enter)", category: Category::Structure },
    CommandInfo { action: Action::PriorityUp, keys: "Shift+↑", name: "Priority up", description: "Raise the [#A]-style priority (or shift a timestamp field)", category: Category::Structure },
    CommandInfo { action: Action::PriorityDown, keys: "Shift+↓", name: "Priority down", description: "Lower the priority (or shift a timestamp field)", category: Category::Structure },
    CommandInfo { action: Action::EditTags, keys: "Ctrl+G", name: "Edit tags", description: "Edit the heading's tags in a prompt", category: Category::Structure },
    CommandInfo { action: Action::ToggleCheckbox, keys: "Ctrl+Space", name: "Toggle checkbox", description: "Toggle the item's checkbox (Alt+X also works)", category: Category::Structure },
    // Dates
    CommandInfo { action: Action::SetScheduled, keys: "Alt+S", name: "Set SCHEDULED", description: "Set or edit the heading's SCHEDULED date (Org)", category: Category::Dates },
    CommandInfo { action: Action::SetDeadline, keys: "Alt+D", name: "Set DEADLINE", description: "Set or edit the heading's DEADLINE date (Org)", category: Category::Dates },
    CommandInfo { action: Action::InsertActiveTs, keys: "Alt+.", name: "Insert timestamp", description: "Insert an active timestamp at the cursor", category: Category::Dates },
    CommandInfo { action: Action::InsertInactiveTs, keys: "Alt+I", name: "Insert inactive timestamp", description: "Insert an inactive timestamp at the cursor", category: Category::Dates },
    // Search
    CommandInfo { action: Action::Find, keys: "Ctrl+F", name: "Find", description: "Incremental search (Ctrl+F/Ctrl+R step while open)", category: Category::Search },
    // Buffers
    CommandInfo { action: Action::ListBuffers, keys: "Ctrl+B", name: "Buffer list", description: "Pick from the open buffers", category: Category::Buffers },
    CommandInfo { action: Action::NextBuffer, keys: "Alt+N", name: "Next buffer", description: "Switch to the next buffer", category: Category::Buffers },
    CommandInfo { action: Action::PrevBuffer, keys: "Alt+P", name: "Previous buffer", description: "Switch to the previous buffer", category: Category::Buffers },
    CommandInfo { action: Action::CloseBuffer, keys: "Ctrl+W", name: "Close buffer", description: "Close the active buffer (asks when unsaved)", category: Category::Buffers },
    // Help
    CommandInfo { action: Action::Help, keys: "Ctrl+H", name: "Help menu", description: "This menu (Ctrl+K also works)", category: Category::Help },
    CommandInfo { action: Action::Guide, keys: "Ctrl+U", name: "Guide", description: "Open the full guide in a buffer", category: Category::Help },
];

/// The commands of one category, in table order.
pub fn commands_in(category: Category) -> Vec<&'static CommandInfo> {
    COMMANDS.iter().filter(|c| c.category == category).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every menu-worthy action, listed once. `requires_entry`'s exhaustive match is the
    /// compile-time guard that forces this list to be revisited when `Action` grows.
    fn menu_actions() -> Vec<Action> {
        vec![
            Action::Save,
            Action::Quit,
            Action::ToggleFold,
            Action::NextHeading,
            Action::PrevHeading,
            Action::CycleTodo,
            Action::OpenFile,
            Action::NextBuffer,
            Action::PrevBuffer,
            Action::ListBuffers,
            Action::CloseBuffer,
            Action::PromoteHeading,
            Action::DemoteHeading,
            Action::PromoteSubtree,
            Action::DemoteSubtree,
            Action::MoveSubtreeUp,
            Action::MoveSubtreeDown,
            Action::InsertSibling,
            Action::InsertTodoSibling,
            Action::PriorityUp,
            Action::PriorityDown,
            Action::EditTags,
            Action::ToggleCheckbox,
            Action::SetScheduled,
            Action::SetDeadline,
            Action::InsertActiveTs,
            Action::InsertInactiveTs,
            Action::Help,
            Action::Guide,
            Action::Find,
        ]
    }

    #[test]
    fn every_menu_action_has_exactly_one_entry() {
        for a in menu_actions() {
            assert!(requires_entry(&a), "{a:?} listed but marked excluded");
            let count = COMMANDS.iter().filter(|c| c.action == a).count();
            assert_eq!(count, 1, "{a:?} should appear exactly once, found {count}");
        }
    }

    #[test]
    fn every_entry_is_menu_worthy_and_well_formed() {
        for c in COMMANDS {
            assert!(requires_entry(&c.action), "{:?} is in the excluded set", c.action);
            assert!(!c.keys.is_empty() && !c.name.is_empty() && !c.description.is_empty());
        }
    }

    #[test]
    fn keys_are_unique_within_a_category() {
        for cat in Category::ALL {
            let mut seen = std::collections::HashSet::new();
            for c in commands_in(cat) {
                assert!(seen.insert(c.keys), "duplicate key {} in {:?}", c.keys, cat);
            }
        }
    }

    #[test]
    fn every_category_has_at_least_one_command() {
        for cat in Category::ALL {
            assert!(!commands_in(cat).is_empty(), "{cat:?} is empty");
        }
    }
}
