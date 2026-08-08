//! Rivo agent mode ring — Normal → Plan → Ask → Debug → Multitask → Normal
//!
//! Cursor-faithful `AgentMode` enum for the rivo fork. This module is the
//! single source of truth for the Shift+Tab ring order and display labels.
//! YOLO / Always-Approve stays orthogonal (Ctrl+O) and is intentionally not
//! part of this ring.

/// Rivo agent modes cycled with Shift+Tab.
///
/// Order: `Normal → Plan → Ask → Debug → Multitask → Normal`.
/// `Normal` is the default (plain Agent mode). `Plan` is the existing
/// read-only-except-plan.md mode. `Ask` is read-only. `Debug` is the
/// hypothesis→instrument→repro→fix loop. `Multitask` is the DAG / parallel
/// task mode. `Ctrl+O` (YOLO) is orthogonal and not a ring step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AgentMode {
    #[default]
    Normal,
    Plan,
    Ask,
    Debug,
    Multitask,
}

impl AgentMode {
    /// Next mode in the ring: Normal → Plan → Ask → Debug → Multitask → Normal.
    pub fn next(&self) -> Self {
        match self {
            Self::Normal => Self::Plan,
            Self::Plan => Self::Ask,
            Self::Ask => Self::Debug,
            Self::Debug => Self::Multitask,
            Self::Multitask => Self::Normal,
        }
    }

    /// Human label for banners, toasts and the status bar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Plan => "Plan",
            Self::Ask => "Ask",
            Self::Debug => "Debug",
            Self::Multitask => "Multitask",
        }
    }

    /// Hint for the status-bar badge color.
    ///
    /// Returns a theme token name the renderer can map to a `Theme` color.
    /// No `Theme` import here to keep this crate-free module dependency-free;
    /// callers match on the returned `&'static str`.
    pub fn badge_color(&self) -> &'static str {
        match self {
            Self::Normal => "default",
            Self::Plan => "plan",
            Self::Ask => "ask",
            Self::Debug => "debug",
            Self::Multitask => "multitask",
        }
    }

    /// System-prompt injection for this mode (Debug/Multitask).
    /// `Normal/Plan/Ask` have no extra injection — `None`.
    /// Returned as a portable prompt fragment to be appended to
    /// `PromptContext.role_instructions` / the agent's system prompt body.
    pub fn system_prompt_injection(&self) -> Option<&'static str> {
        match self {
            Self::Debug => Some(
                "You are in Debug mode: generate 2-4 hypotheses, propose instrumented logs with \
                 [rivo-debug] marks, ask user to reproduce, analyze logs, offer (A) Review logs / \
                 (B) Solved loop.",
            ),
            Self::Multitask => Some(
                "You are orchestrator: delegate each todo to spawn_subagent, don't edit directly, \
                 aggregate.",
            ),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AgentMode;

    #[test]
    fn ring_cycles_five_steps() {
        let mut mode = AgentMode::Normal;
        let expected = [
            AgentMode::Plan,
            AgentMode::Ask,
            AgentMode::Debug,
            AgentMode::Multitask,
            AgentMode::Normal,
        ];
        for &exp in &expected {
            mode = mode.next();
            assert_eq!(mode, exp);
        }
    }

    #[test]
    fn labels_are_non_empty() {
        for mode in [
            AgentMode::Normal,
            AgentMode::Plan,
            AgentMode::Ask,
            AgentMode::Debug,
            AgentMode::Multitask,
        ] {
            assert!(!mode.label().is_empty());
        }
    }

    #[test]
    fn default_is_normal() {
        assert_eq!(AgentMode::default(), AgentMode::Normal);
    }
}
