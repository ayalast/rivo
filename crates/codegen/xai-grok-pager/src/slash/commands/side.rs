//! `/side` — open a durable side chat (Cursor-faithful scaffold).
//!
//! Scaffold behind flag: for now creates a side chat via `AppView::side_chats`
//! and shows a toast "Side chats coming soon (scaffold)". Keeps existing `/btw`
//! (`BtwCommand`) working — `/btw` still fires the transient panel. `/side` is
//! the new durable sibling; future consolidation will alias `/btw` → `/side`.

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

/// `/side [question]` — open/create a durable side chat.
///
/// When `args` is non-empty it is treated as the initial question.
/// No session yet — scaffold just toasts; real send will route via
/// `Action::CreateSideChat { parent_id, prompt }` through dispatch.
///
/// Keeps `/btw` working: `BtwCommand` still owns `/btw`.
pub struct SideCommand;

impl SlashCommand for SideCommand {
    fn name(&self) -> &str {
        "side"
    }

    fn aliases(&self) -> &[&str] {
        // Scaffold keeps `/btw` owned by `BtwCommand` to avoid alias collision.
        // Future: alias `["btw"]` once the transient panel is migrated.
        &[]
    }

    fn description(&self) -> &str {
        "Open a durable side chat (Cursor faithful; scaffold)"
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
        let prompt = args.trim();
        if prompt.is_empty() {
            // Bare `/side` — scaffold toast until dispatch creates the chat.
            CommandResult::Message("Side chats coming soon (scaffold) — try /side <question>".to_string())
        } else {
            // With text — route through Action so dispatch can create SideChat
            // via `AppView::side_chats.create_side(parent_id, prompt)` and toast.
            // For scaffold, also toast immediately if no session/active agent.
            let _ = prompt;
            CommandResult::Action(Action::CreateSideChat {
                parent_id: String::new(),
                prompt: prompt.to_string(),
            })
        }
    }
}

/// `/sides` — list side chats (scaffold: toast with count).
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
