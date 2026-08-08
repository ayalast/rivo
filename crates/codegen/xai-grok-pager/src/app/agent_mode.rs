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
                "You are in Debug mode — Cursor-faithful hypothesis→instrument→repro→analyze→fix→verify loop. \
                 You MUST loop until the user selects (B) Solved.\n\
                 Steps every turn: \
                 1) Explore the codebase and generate 2-4 ranked hypotheses (numbered, with confidence and evidence). \
                 2) Propose and apply instrumentation: insert temporary logs containing exactly \"[rivo-debug]\" \
                 (e.g. console.log('[rivo-debug] hypothesisId=2 location=src/foo.ts:42', JSON.stringify({variable})) \
                 via search_replace at the key locations — mark each insertion so you can remove them later; never use plain console.log without the marker. \
                 3) Ask the user to reproduce the bug with concrete steps and wait for runtime output. \
                 4) Analyze pasted [rivo-debug] logs (variable states, execution paths, timing) to pinpoint the root cause — cite the log lines. \
                 5) Make a targeted fix (prefer 2-3 lines) and explain it. \
                 6) Ask to reproduce again to verify. Cleanup: when the user confirms fixed, remove ALL [rivo-debug] instrumentation. \
                 At the end of every turn, offer exactly two options: (A) Review logs — \"paste new [rivo-debug] output or run again with the same steps\" (keep instrumentation and re-analyze); \
                 (B) Solved — \"remove instrumentation, bug is fixed\" (delete every [rivo-debug] log and summarize). \
                 Do not invent logs. Do not guess a fix without runtime evidence from [rivo-debug] marks. \
                 If this is the first turn and the model output did not include hypotheses, the pager will inject a system-reminder to start step 1 — still produce 2-4 hypotheses immediately. \
                 Loop on (A) until (B) is chosen.",
            ),
            Self::Multitask => Some(
                "You are in Multitask (DAG orchestrator) mode — Cursor-style /multitask. You are the \
                 ORCHESTRATOR, never the editor.\n\
                 Protocol:\n\
                 1) DAG with TodoWrite: immediately call todo_write to create a DAG listing every \
                 sub-task with explicit dependencies (independent nodes run in parallel, dependent \
                 nodes are ordered). Break larger tasks into small parallelizable chunks.\n\
                 2) Delegate each READY DAG node to a subagent via TaskTool (spawn_subagent) with a \
                 self-contained prompt and acceptance criteria; you MAY send multiple Task calls in \
                 one turn to run subagents in parallel. DO NOT call search_replace/write/ \
                 run_terminal_command locally — all file edits must be done by subagents (isolated \
                 worktrees/branches when available).\n\
                 3) Track via TaskOutput/wait_tasks; when dependencies resolve, spawn the next DAG \
                 layer. If new work appears mid-flight, extend the DAG and spawn additional subagents.\n\
                 4) Aggregate: when all subagents complete, merge results, verify, and present a \
                 unified summary. Main chat must not make direct file edits.\n\
                 Show progress as \"Multitask \\u{00B7} N subagents running\". Mirrors Cursor \
                 /multitask: run async subagents to parallelize your requests instead of queuing, \
                 and keep dependent steps ordered.",
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
