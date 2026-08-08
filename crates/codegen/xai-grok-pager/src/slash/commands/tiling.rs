//! `/window` `/tile` `/tiling` — toggle tiled window layout.
//!
//! All three names map to the same command (aliases). Without args, toggles.
//! With `on`/`off`/`enable`/`disable` (and `1`/`0`/`true`/`false`), sets explicitly.
//! Persists via WindowManager persist (atomic write) in dispatch.

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct TilingCommand;

impl SlashCommand for TilingCommand {
    fn name(&self) -> &str {
        "window"
    }

    fn aliases(&self) -> &[&str] {
        &["tile", "tiling", "windows"]
    }

    fn description(&self) -> &str {
        "Toggle tiled window layout (or /window on|off)"
    }

    fn usage(&self) -> &str {
        "/window [on|off]"
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
            "on" | "enable" | "enabled" | "1" | "true" | "yes" => {
                CommandResult::Action(Action::SetTiling(true))
            }
            "off" | "disable" | "disabled" | "0" | "false" | "no" => {
                CommandResult::Action(Action::SetTiling(false))
            }
            _ => CommandResult::Message(
                "Usage: /window [on|off] — toggle tiled layout. Also: /tile, /tiling".to_string(),
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
    }

    #[test]
    fn aliases_resolve() {
        let reg = crate::slash::registry::CommandRegistry::new(vec![std::sync::Arc::new(TilingCommand)]);
        assert_eq!(reg.get("window").unwrap().name(), "window");
        assert_eq!(reg.get("tile").unwrap().name(), "window");
        assert_eq!(reg.get("tiling").unwrap().name(), "window");
        assert_eq!(reg.get("windows").unwrap().name(), "window");
    }
}
