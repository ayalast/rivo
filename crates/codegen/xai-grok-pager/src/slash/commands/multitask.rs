//! `/multitask` — enter Multitask (DAG orchestrator) mode.
//!
//! Mirrors Cursor's `/multitask`: the main chat becomes an orchestrator that
//! builds a DAG of sub-tasks, delegates each node to a subagent via TaskTool,
//! and aggregates. This command switches the active agent (or the next session
//! when session-less) to `AgentMode::Multitask` and shows the mode banner.

use crate::app::actions::Action;
use crate::app::agent_mode::AgentMode;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

/// Enter Multitask mode.
pub struct MultitaskCommand;

impl SlashCommand for MultitaskCommand {
    fn name(&self) -> &str {
        "multitask"
    }

    fn description(&self) -> &str {
        "Enter Multitask mode — DAG orchestrator with parallel subagents"
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn offered_when_session_less(&self) -> bool {
        true
    }

    fn usage(&self) -> &str {
        "/multitask"
    }

    fn takes_args(&self) -> bool {
        false
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Action(Action::SetAgentMode(AgentMode::Multitask))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::model_state::ModelState;
    use crate::app::bundle::BundleState;
    use crate::settings::PagerLocalSnapshot;

    fn make_ctx<'a>(
        models: &'a ModelState,
        bundle: &'a BundleState,
    ) -> CommandExecCtx<'a> {
        CommandExecCtx {
            models,
            session_id: None,
            bundle_state: bundle,
            screen_mode: crate::app::ScreenMode::Inline,
            billing_surface_visible: true,
            usage_command_visible: true,
            pager_state: PagerLocalSnapshot::default(),
        }
    }

    #[test]
    fn multitask_dispatches_set_agent_mode() {
        let cmd = MultitaskCommand;
        let models = ModelState::default();
        let bundle = BundleState::default();
        let mut ctx = make_ctx(&models, &bundle);
        match cmd.run(&mut ctx, "") {
            CommandResult::Action(Action::SetAgentMode(mode)) => {
                assert_eq!(mode, AgentMode::Multitask);
            }
            other => panic!("expected SetAgentMode(Multitask), got {other:?}"),
        }
    }

    #[test]
    fn multitask_is_session_scoped_but_offered_when_less() {
        assert!(MultitaskCommand.session_scoped());
        assert!(MultitaskCommand.offered_when_session_less());
    }
}
