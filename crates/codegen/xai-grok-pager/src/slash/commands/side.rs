//! `/side` — durable side chats (Cursor-faithful).
//!
//! `/side` creates a durable conversation in the right-hand side panel.

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

/// `/side [question]` — open/create a durable side chat.
///
/// When `args` is non-empty it is treated as the initial question.
/// `/side` bare opens a chat empty; `/side <question>` sends initial.
/// Also supports subcommands `switch`, `close`, `send` for TUI wiring.
pub struct SideCommand;

impl SlashCommand for SideCommand {
    fn name(&self) -> &str {
        "side"
    }

    fn aliases(&self) -> &[&str] {
        &[]
    }

    fn description(&self) -> &str {
        "Open a durable side chat — /side [question]"
    }

    fn usage(&self) -> &str {
        "/side [question]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("[question]")
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            // Bare `/side` — create empty side chat (no initial prompt).
            return CommandResult::Action(Action::CreateSideChat {
                parent_id: String::new(),
                prompt: String::new(),
            });
        }
        // The rest of the line is always the initial prompt.  Management is
        // intentionally kept in `/sides` and visible tab chrome so ordinary
        // natural-language questions such as "close a file" are not parsed as
        // accidental command subcommands.
        CommandResult::Action(Action::CreateSideChat {
            parent_id: String::new(),
            prompt: trimmed.to_string(),
        })
    }
}

/// `/sides` — list side chats (toast with count). Also ` /side list` alternative.
pub struct SidesCommand;

impl SlashCommand for SidesCommand {
    fn name(&self) -> &str {
        "sides"
    }

    fn aliases(&self) -> &[&str] {
        &[]
    }

    fn description(&self) -> &str {
        "List durable side chats"
    }

    fn usage(&self) -> &str {
        "/sides"
    }

    fn takes_args(&self) -> bool {
        false
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Action(Action::ListSideChats)
    }
}

#[cfg(test)]
mod tests {
    use super::{SideCommand, SidesCommand};
    use crate::slash::command::SlashCommand;

    #[test]
    fn side_is_session_scoped() {
        assert!(SideCommand.session_scoped());
        assert!(SidesCommand.session_scoped());
    }

    #[test]
    fn side_names() {
        assert_eq!(SideCommand.name(), "side");
        assert_eq!(SidesCommand.name(), "sides");
    }

    #[test]
    fn side_takes_optional_args() {
        assert!(SideCommand.takes_args());
        assert!(!SideCommand.args_required());
        assert!(!SidesCommand.takes_args());
    }
}
