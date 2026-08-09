//! `/window` `/tile` `/tiling` — control the right-hand side panel.
//!
//! All three names map to the same command (aliases). Without args, toggles
//! the panel (show/hide when tabs exist). `on`/`off` show or hide explicitly.
//! `reset-size` restores the default 65/35 split. Persists the ratio via the
//! side-panel preferences (atomic write in dispatch).

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct TilingCommand;

impl SlashCommand for TilingCommand {
    fn name(&self) -> &str {
        "window"
    }

    fn aliases(&self) -> &[&str] {
        &["tile", "tiling", "windows", "panel"]
    }

    fn description(&self) -> &str {
        "Control the side panel: /window [on|off|reset-size]"
    }

    fn usage(&self) -> &str {
        "/window [on|off|reset-size]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let trimmed = args.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            return CommandResult::Action(Action::ToggleTiling);
        }
        match trimmed.as_str() {
            "on" | "enable" | "enabled" | "1" | "true" | "yes" | "show" => {
                CommandResult::Action(Action::SetTiling(true))
            }
            "off" | "disable" | "disabled" | "0" | "false" | "no" | "hide" => {
                CommandResult::Action(Action::SetTiling(false))
            }
            "reset" | "reset-size" | "reset-ratio" | "65" => {
                CommandResult::Action(Action::SetTilingReset)
            }
            _ => CommandResult::Message(
                "Usage: /window [on|off|reset-size] — control the side panel. Also: /tile, /tiling"
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::Action;
    use crate::slash::command::SlashCommand;

    fn ctx<'a>(
        models: &'a crate::acp::model_state::ModelState,
        bundle: &'a crate::app::bundle::BundleState,
    ) -> CommandExecCtx<'a> {
        CommandExecCtx {
            models,
            session_id: None,
            bundle_state: bundle,
            screen_mode: crate::app::ScreenMode::Inline,
            billing_surface_visible: true,
            usage_command_visible: true,
            pager_state: crate::settings::PagerLocalSnapshot::default(),
        }
    }

    #[test]
    fn bare_toggles() {
        let cmd = TilingCommand;
        let models = crate::acp::model_state::ModelState::default();
        let bundle = crate::app::bundle::BundleState::default();
        let mut c = ctx(&models, &bundle);
        assert!(matches!(cmd.run(&mut c, ""), CommandResult::Action(Action::ToggleTiling)));
        assert!(matches!(cmd.run(&mut c, "   "), CommandResult::Action(Action::ToggleTiling)));
    }

    #[test]
    fn on_off_sets() {
        let cmd = TilingCommand;
        let models = crate::acp::model_state::ModelState::default();
        let bundle = crate::app::bundle::BundleState::default();
        let mut c = ctx(&models, &bundle);
        assert!(matches!(cmd.run(&mut c, "on"), CommandResult::Action(Action::SetTiling(true))));
        assert!(matches!(cmd.run(&mut c, "off"), CommandResult::Action(Action::SetTiling(false))));
        assert!(matches!(cmd.run(&mut c, "enable"), CommandResult::Action(Action::SetTiling(true))));
        assert!(matches!(cmd.run(&mut c, "disable"), CommandResult::Action(Action::SetTiling(false))));
        assert!(matches!(cmd.run(&mut c, "show"), CommandResult::Action(Action::SetTiling(true))));
        assert!(matches!(cmd.run(&mut c, "hide"), CommandResult::Action(Action::SetTiling(false))));
        assert!(matches!(
            cmd.run(&mut c, "reset-size"),
            CommandResult::Action(Action::SetTilingReset)
        ));
    }

    #[test]
    fn aliases_resolve() {
        let reg = crate::slash::registry::CommandRegistry::new(vec![std::sync::Arc::new(TilingCommand)]);
        assert_eq!(reg.get("window").unwrap().name(), "window");
        assert_eq!(reg.get("tile").unwrap().name(), "window");
        assert_eq!(reg.get("tiling").unwrap().name(), "window");
        assert_eq!(reg.get("windows").unwrap().name(), "window");
        assert_eq!(reg.get("panel").unwrap().name(), "window");
    }
}
