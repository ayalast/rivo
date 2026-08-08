//! Cursor-faithful Debug loop helper — hypothesis → instrument → repro → analyze → fix → verify.
//!
//! Lightweight pager-side tracker for the Debug agent mode. The model drives the
//! loop via the system prompt (`AgentMode::Debug.system_prompt_injection`), but
//! the pager keeps a small state machine so the UI can surface progress and the
//! `[rivo-debug]` instrumentation contract is enforceable without re-parsing the
//! transcript each frame.
//!
//! Mark contract: every instrumentation insertion MUST contain the literal
//! `[rivo-debug]` (e.g. `console.log('[rivo-debug] hypothesisId=2 at src/foo.ts:42', JSON.stringify({x}))`).
//! The helper tracks hypotheses (2-4) and whether at least one such marker has
//! been emitted, and always offers the two terminal options at turn end:
//! (A) Review logs — keep instrumentation, paste new `[rivo-debug]` output
//! (B) Solved — remove all instrumentation, summarize fix.
//!
//! No rendering here — pure data. The view layer (`agent_view/render.rs`) may
//! read `DebugLoopState` to show a chip or card, paralleling `question_view` /
//! `permission_view`.

/// Marker that every instrumentation log must contain. Literal, case-sensitive.
pub const RIVO_DEBUG_MARKER: &str = "[rivo-debug]";

/// One hypothesis in the 2-4 set generated at loop start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hypothesis {
    pub id: usize,
    pub text: String,
}

/// A single `[rivo-debug]` log line captured from tool output / terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugLogEntry {
    /// Raw line that contained the marker.
    pub raw: String,
    /// Optional hypothesis id parsed from the line (e.g. `hypothesisId=2`).
    pub hypothesis_id: Option<usize>,
}

/// Phase of the Cursor Debug loop. Mirrors `cursor-research.md` 6 steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugPhase {
    #[default]
    Hypothesize,
    Instrument,
    Reproduce,
    Analyze,
    Fix,
    Verify,
    Solved,
}

impl DebugPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hypothesize => "hypothesize",
            Self::Instrument => "instrument",
            Self::Reproduce => "reproduce",
            Self::Analyze => "analyze",
            Self::Fix => "fix",
            Self::Verify => "verify",
            Self::Solved => "solved",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Hypothesize => Self::Instrument,
            Self::Instrument => Self::Reproduce,
            Self::Reproduce => Self::Analyze,
            Self::Analyze => Self::Fix,
            Self::Fix => Self::Verify,
            Self::Verify => Self::Solved,
            Self::Solved => Self::Solved,
        }
    }
}

/// Pager-side Debug loop state, stored on `AgentView` while `agent_mode == Debug`.
///
/// Created when entering Debug, updated on each turn / tool output that
/// contains `[rivo-debug]` markers, and cleared (or marked Solved) when the
/// user picks (B) Solved. The loop is `Solved` only when the user confirms it —
/// until then `phase != Solved` and the system prompt keeps offering (A)/(B).
#[derive(Debug, Clone, Default)]
pub struct DebugLoopState {
    pub hypotheses: Vec<Hypothesis>,
    pub logs: Vec<DebugLogEntry>,
    pub phase: DebugPhase,
    pub instrumented: bool,
}

impl DebugLoopState {
    pub fn new() -> Self {
        Self {
            hypotheses: Vec::new(),
            logs: Vec::new(),
            phase: DebugPhase::Hypothesize,
            instrumented: false,
        }
    }

    /// Record a hypothesis (1 of 2-4). Caller ensures 2 ≤ len ≤ 4 before
    /// advancing phase, but this helper does not enforce — the model does.
    pub fn push_hypothesis(&mut self, text: impl Into<String>) {
        let id = self.hypotheses.len() + 1;
        self.hypotheses.push(Hypothesis {
            id,
            text: text.into(),
        });
    }

    /// True if `line` contains the required `[rivo-debug]` marker.
    pub fn is_rivo_debug_log(line: &str) -> bool {
        line.contains(RIVO_DEBUG_MARKER)
    }

    /// Extract all `[rivo-debug]` lines from a blob of text (tool output,
    /// terminal stdout, or pasted logs). Returns the raw lines that contain
    /// the marker, trimmed.
    pub fn extract_marker_lines(text: &str) -> Vec<String> {
        text.lines()
            .filter(|l| Self::is_rivo_debug_log(l))
            .map(|l| l.trim().to_string())
            .collect()
    }

    /// Record a log line if it contains the marker. Returns true if recorded.
    pub fn record_log(&mut self, raw: impl Into<String>) -> bool {
        let raw = raw.into();
        if !Self::is_rivo_debug_log(&raw) {
            return false;
        }
        let hypothesis_id = parse_hypothesis_id(&raw);
        self.logs.push(DebugLogEntry {
            raw,
            hypothesis_id,
        });
        self.instrumented = true;
        true
    }

    /// Record all marker lines from a multi-line text blob. Returns count.
    pub fn record_logs_from_text(&mut self, text: &str) -> usize {
        let mut n = 0;
        for line in Self::extract_marker_lines(text) {
            self.record_log(line);
            n += 1;
        }
        n
    }

    /// Whether at least one `[rivo-debug]` log has been captured.
    pub fn has_logs(&self) -> bool {
        !self.logs.is_empty()
    }

    /// Whether hypotheses have been populated (2-4 expected).
    pub fn has_hypotheses(&self) -> bool {
        !self.hypotheses.is_empty()
    }

    /// Advance phase if preconditions are met. Pure helper for UI hints.
    pub fn advance_if_ready(&mut self) {
        match self.phase {
            DebugPhase::Hypothesize if self.hypotheses.len() >= 2 => {
                self.phase = DebugPhase::Instrument;
            }
            DebugPhase::Instrument if self.instrumented => {
                self.phase = DebugPhase::Reproduce;
            }
            DebugPhase::Reproduce if self.has_logs() => {
                self.phase = DebugPhase::Analyze;
            }
            _ => {}
        }
    }

    /// Mark as solved — user picked (B). Caller should also remove
    /// instrumentation (search for `[rivo-debug]` and delete those lines).
    pub fn mark_solved(&mut self) {
        self.phase = DebugPhase::Solved;
    }

    /// System reminder injected at turn start when in Debug but model has
    /// not yet produced hypotheses/ instrumentation. The pager calls this
    /// when `agent_mode == Debug` and `hypotheses.is_empty()` after a turn.
    pub fn first_turn_reminder() -> &'static str {
        "System reminder: you are in Debug mode. If you have not yet done so, \
         generate 2-4 ranked hypotheses, propose instrumentation with exactly \
         \"[rivo-debug]\" marks, ask the user to reproduce, then analyze the \
         [rivo-debug] logs. End every turn with (A) Review logs / (B) Solved. \
         Do not invent logs; fix only with runtime evidence."
    }

    /// The two terminal options offered every turn, per Cursor spec.
    pub fn ab_options() -> [&'static str; 2] {
        [
            "(A) Review logs — paste new [rivo-debug] output or re-run with the same steps",
            "(B) Solved — remove all [rivo-debug] instrumentation and summarize the fix",
        ]
    }

    /// Quick check for any `[rivo-debug]` marker in `text`.
    pub fn contains_marker(text: &str) -> bool {
        text.contains(RIVO_DEBUG_MARKER)
    }
}

fn parse_hypothesis_id(line: &str) -> Option<usize> {
    // Very small parser: look for `hypothesisId=NUM` or `hypothesis=NUM`
    for token in line.split(|c: char| !c.is_ascii_alphanumeric() && c != '=' && c != '_') {
        if let Some(rest) = token.strip_prefix("hypothesisId=") {
            if let Ok(n) = rest.parse() {
                return Some(n);
            }
        }
        if let Some(rest) = token.strip_prefix("hypothesis=") {
            if let Ok(n) = rest.parse() {
                return Some(n);
            }
        }
    }
    // Fallback: scan for `id=NUM` near marker
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_detection() {
        assert!(DebugLoopState::is_rivo_debug_log("[rivo-debug] foo"));
        assert!(DebugLoopState::is_rivo_debug_log(
            "console.log('[rivo-debug] hi', x)"
        ));
        assert!(!DebugLoopState::is_rivo_debug_log("console.log('hi', x)"));
        assert!(!DebugLoopState::is_rivo_debug_log("[rivo - debug]"));
    }

    #[test]
    fn extract_marker_lines_filters() {
        let text = "line1\n[rivo-debug] a=1\nline2\n[rivo-debug] b=2\n";
        let out = DebugLoopState::extract_marker_lines(text);
        assert_eq!(out.len(), 2);
        assert!(out[0].contains("[rivo-debug]"));
    }

    #[test]
    fn loop_advances_phases() {
        let mut s = DebugLoopState::new();
        assert_eq!(s.phase, DebugPhase::Hypothesize);
        s.push_hypothesis("h1");
        s.push_hypothesis("h2");
        s.advance_if_ready();
        assert_eq!(s.phase, DebugPhase::Instrument);
        s.record_log("[rivo-debug] test");
        s.advance_if_ready();
        assert_eq!(s.phase, DebugPhase::Reproduce);
    }

    #[test]
    fn first_turn_reminder_mentions_marker_and_ab() {
        let r = DebugLoopState::first_turn_reminder();
        assert!(r.contains("[rivo-debug]"));
        assert!(r.contains("(A)"));
        assert!(r.contains("(B)"));
    }

    #[test]
    fn contains_marker_helper() {
        assert!(DebugLoopState::contains_marker("[rivo-debug]"));
        assert!(!DebugLoopState::contains_marker("nope"));
    }
}
