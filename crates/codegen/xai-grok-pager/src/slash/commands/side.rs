//! `/side` — durable side chats (Cursor-faithful).
//!
//! `/side` creates a `SideChat` via `Action::CreateSideChat` (parent_id +
//! prompt) and `Action::ListSideChats` / switch / close + transcript
//! follow-ups. Wired to `WindowManager` when `tiling_enabled` (docked right
//! 65|35) or as overlay when not. Also honors `btw` alias later.

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
        "Open a durable side chat — /side list/switch/close"
    }

    fn usage(&self) -> &str {
        "/side [question]|switch <id>|close <id>|list"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("[question]|[switch|close] <id>")
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
        // Subcommands within `/side ...`
        let lower = trimmed.to_ascii_lowercase();
        if lower == "list" || lower == "ls" {
            return CommandResult::Action(Action::ListSideChats);
        }
        if lower.strip_prefix("switch ").is_some() {
            let id = trimmed[7..].trim();
            if id.is_empty() {
                return CommandResult::Message("Usage: /side switch <id>".to_string());
            }
            return CommandResult::Action(Action::SwitchSideChat { id: id.to_string() });
        }
        if lower.strip_prefix("close ").is_some() {
            let id = trimmed[6..].trim();
            if id.is_empty() {
                return CommandResult::Message("Usage: /side close <id>".to_string());
            }
            return CommandResult::Action(Action::CloseSideChat { id: id.to_string() });
        }
        // Otherwise treat as initial question / follow-up if active side chat exists is handled in dispatch.
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
