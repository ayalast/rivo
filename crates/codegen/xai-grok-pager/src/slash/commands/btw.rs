//! `/btw` -- compatibility alias for a durable `/side` question.
//!
//! Cursor's early launch copy used `/btw`; Rivo keeps it as a spelling alias,
//! not a second ephemeral conversation implementation.

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct BtwCommand;

impl SlashCommand for BtwCommand {
    fn name(&self) -> &str {
        "btw"
    }

    fn description(&self) -> &str {
        "Open a durable side chat (compatibility alias for /side)"
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn usage(&self) -> &str {
        "/btw <question>"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("<question>")
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let prompt = args.trim();
        if prompt.is_empty() {
            return CommandResult::Message("Usage: /btw <question>".to_string());
        }
        CommandResult::Action(Action::CreateSideChat {
            parent_id: String::new(),
            prompt: prompt.to_string(),
            from_selection: false,
        })
    }
}
