# Rivo Modes Implementation — Handoff Doc

> **Date:** 2026-08-07 · **Author:** modes subagent · **Status:** Research complete, no Rust source edited
> **Repo:** `C:\rivo` (fork of `xai-org/grok-build` 1.0.0) · branch `main` · binary `rivo` (`xai-grok-pager` + alias `rivo` in `crates/codegen/xai-grok-pager-bin/Cargo.toml`)
> **Context docs:** `docs/cursor-research.md` (exhaustive Cursor feature map), `docs/name-check.md` (name audit), `plan.md` in Grok session storage (full project plan)
> **Scope:** Thorough codebase exploration only — exact file paths, line numbers, current snippets, and **what must change** for the rivo modes fork. The main agent should be able to implement immediately after the background `cargo build` finishes.

---

## Executive Summary

### What exists today in grok-build (upstream `grok`)

| Concern | Current reality | Location |
|---|---|---|
| **Mode ring** | `Shift+Tab` cycles `Normal → Plan → Auto → Always-Approve → Normal` (Auto skipped when `auto_mode_gate==false`; legacy `Normal→Plan→Always-Approve` remains). Pre-session stashes `deferred_session_mode`. | `input/key.rs:309-324`, `actions/defaults.rs:519-533`, `app/dispatch/modes.rs:631-968` |
| **Plan Mode** | First-class. Agent calls `enter_plan_mode` (read-only + seeds `plan.md`); on exit `exit_plan_mode` parks `PlanApprovalViewState` (`a` approve / `s` request changes / `c` comment / `q` quit). Edit gate rejects non-`plan.md` writes even under YOLO. | `xai-grok-tools/src/implementations/grok_build/enter_plan_mode/mod.rs`, `exit_plan_mode/mod.rs`, `app/agent_view/mod.rs:1322-1337`, `app/acp_handler/session_notification.rs:1426-1460` |
| **YOLO / Always-Approve** | Per-session `agent.session.yolo_mode` + global `app.default_yolo` mirror. `Ctrl+O` toggles; `/always-approve` toggles; `PermissionModeKind::{Default,Ask,Auto,AlwaysApprove}`. `Auto` is LLM classifier (`auto_mode` flag, `yolo` wins). Persisted to `[ui].permission_mode` + ACP `x.ai/yolo_mode_changed`. | `app/actions.rs:1032-1090`, `app/dispatch/modes.rs:198-420`, `app/effects/helpers.rs:1394-1540`, `app/agent.rs:726-735`, `app/acp_handler/permissions.rs:20-70` |
| **Tool system** | ~50 tools in `xai-grok-tools`. Categorized by `ToolKind` (`Read/Edit/Execute/Plan/AskUser/...`) with `ToolMetadata::is_read_only()` + `Tool::capabilities().is_read_only`. No Ask-gate yet. | `xai-grok-tools/src/types/tool.rs:70-118`, `xai-grok-tools/src/tool_taxonomy.rs:79-260` |
| **Status bar** | Two-layer: generic `views/status_bar.rs` (left/center/right `StatusBar` widget) and per-agent `views/agent_status.rs::AgentStatusBar` (right-aligned `push(id, Line)` with `│` separators). Rendered in `app/agent_view/render.rs:1445-1520` into `layout.status_bar`. Chips: `plan`, `goal`, `mcp (n/m)`, `bg_tasks`. | `views/agent_status.rs:40-135`, `views/agent.rs:102-110`, `app/agent_view/mod.rs:150-400`, `app/agent_view/plan.rs:68-80` |
| **Side question** | `/btw` == **inline transient panel** (not durable side chat). `BtwOverlayState::{Loading,Done,Error}` rendered in `views/btw_overlay.rs` above prompt. Fires `x.ai/btw` ext method, bypasses queue. Dismissed with `Esc` → collapsed `BtwBlock` in scrollback. | `slash/commands/btw.rs`, `views/btw_overlay.rs:30-490`, `app/dispatch/notes.rs:349-395`, `scrollback/blocks/btw.rs` |
| **Layout** | **Single-column vertical stack** via `ratatui::layout::Layout::vertical`. No tiling. `AgentViewLayout::compute` produces ~15 `Rect`s stacked top-to-bottom. No `Layout::horizontal` tiling, no sidebar. | `views/agent.rs:100-430`, `app/agent_view/mod.rs:100-430` |
| **Slash registry** | `slash/registry.rs`, `slash/command.rs`, `slash/commands/*` (39 builtins + ACP-advertised skills). Gates: `hidden`, `menu_hidden`, `restricted`, `available_tools`, `mode_support`. | `slash/registry.rs:113-610`, `slash/command.rs:160-370` |

### What `rivo` must become (Cursor-faithful, from `docs/cursor-research.md`)

Cursor's model-mode split is the guiding principle (**toolset gating, not model swap** — `cursor-research.md:37`): all modes run the same model; only the visible toolset changes.

*   **`AgentMode` enum** (`Normal=Agent, Ask, Plan, Debug, Multitask`) — cycle `Normal → Plan → Ask → Debug → Multitask → Normal` (Cursor canonical `Agent→Ask→Plan→Debug` plus `Multitask` appended; `cursor-research.md:131-142`). `Multitask` is Cursor's `/multitask` DAG (not a Shift+Tab item upstream) but rivo explicitly cycles it for convenience.
*   **`ApprovalMode` orthogonal flag** (`YOLO` global) — `Ctrl+O` / `--yolo` / `/always-approve` suppresses **all** approvals (main/side/subagent/window).Badge reads `Rivo · Ask · YOLO` (`cursor-research.md:118-120`).
*   **`Ask` read-only** allowlist: `read_file`, `list_dir`, `grep/glob`, `web_search/fetch`, `memory_search`, `ask_user_question` — block `search_replace/write/run_terminal_command` (future allow `git status/diff` read-only).
*   **`Debug`** hypothesis→instrument→repro→analyze→fix→verify loop (Cursor Debug Mode `cursor-research.md:59-77`).
*   **Side Chats durable** (`/side` alias `/btw`): `SideChat { parentId, hiddenContext, transcript, durable, archived }` (`cursor-research.md:144-175`).
*   **Tiled windows** (`ratatui::layout::Layout` `Horizontal`/`Vertical` + drag + `Ctrl+←/→`) persisting to `~/.rivo/windows.json` (`cursor-research.md:206-217`).

---

## 1. Shift+Tab and Keybindings — Mode Switching

### 1.1 Where `Shift+Tab` is defined and matched

**Single source of truth:** `crates/codegen/xai-grok-pager/src/input/key.rs`

```rust
// line 309-324
/// Canonical Shift+Tab encodings: `BackTab` (most xterm-likes),
/// `BackTab+SHIFT` (some terminals), `Tab+SHIFT` (kitty protocol, some
/// Windows terminals). Single source of truth for the `CycleMode` /
/// `DashboardCycleMode` ActionDefs and [`is_shift_tab`].
pub fn shift_tab_keys() -> [KeyShortcut; 3] {
    [
        KeyShortcut::key(KeyCode::BackTab),
        KeyShortcut::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        KeyShortcut::new(KeyCode::Tab, KeyModifiers::SHIFT),
    ]
}

/// True when the event is Shift+Tab in any encoding from
/// [`shift_tab_keys`]. Release events never match.
pub fn is_shift_tab(key: &KeyEvent) -> bool {
    shift_tab_keys().iter().any(|k| k.matches(key))
}
```

Additional helper used by cheatsheet telemetry:

```rust
// key.rs:129-130  BackTab displays as "Shift+Tab" in every surface
KeyCode::BackTab => f.write_str("Shift+Tab"),
// key.rs:181-183  shift injection for BackTab without SHIFT modifier
if self.code == KeyCode::BackTab && !has_shift {
    parts.push("Shift".into());
}
```

**Pinned test:** `key.rs:461-484` — `shift_tab_all_encodings` asserts the three encodings and that bare `Tab`, `Ctrl+Tab`, `Alt+Tab` do **not** match, and that `KeyEventKind::Release` never matches. Any new mode cycle must not break this.

### 1.2 Action registrations (what Shift+Tab actually does)

**File:** `crates/codegen/xai-grok-pager/src/actions/defaults.rs`

Agent-scope `CycleMode` (lines 519-533):

```rust
ActionDef {
    id: ActionId::CycleMode,
    label: "mode",
    description: "Cycle mode (Normal / Plan / Always-approve)",
    // All Shift+Tab encodings — see `input::key::shift_tab_keys()`.
    default_key: crate::input::key::shift_tab_keys()[0],
    alt_keys: crate::input::key::shift_tab_keys()[1..].to_vec(),
    category: Category::GettingStarted,
    context: When::PromptFocused,
    hint_priority: None,
    hint_key_display: Some("Shift+Tab"),
    requires_confirmation: false,
    long_help: Some(
        "Steps the session mode: Normal -> Plan -> Always-Approve -> Normal.\nPlan keeps the agent planning first and writes no files; Always-Approve runs every tool call without asking.\nCtrl+O toggles auto-approve directly.",
    ),
},
```

Dashboard-scope mirror (lines 982-992) — same three keys but `ActionId::DashboardCycleMode` and description *"Cycles the dispatch mode for agents you launch from the dashboard: Normal, Plan, then Always-Approve."* Both share `shift_tab_keys()` so they cannot drift (pinned by `tips/plan_nudge.rs:36-39` and its test).

`ToggleYolo` (`Ctrl+O`) lines 734-736 and mirror `DashboardToggleAutoApprove` lines 1093-1110 bind `key!('o', CONTROL)` — see §3.

### 1.3 Dispatch: where mode state lives and mutates

**State fields:**

*Per-agent (in `crates/codegen/xai-grok-pager/src/app/agent_view/mod.rs` lines 1322-1337):*

```rust
/// Whether plan mode is currently active. Set when `enter_plan_mode`
/// tool completes, cleared when `exit_plan_mode` tool completes.
pub(crate) plan_mode_active: bool,
/// Optimistic plan-mode state set immediately on Shift+Tab.
/// Cleared to `None` when `detect_plan_mode_change()` confirms real state.
pub(crate) plan_mode_pending: Option<bool>,
/// Session mode to apply once this agent's ACP session exists. Set when
/// the agent is spawned from the dashboard with `/plan` active
deferred_session_mode: Option<xai_grok_tools::types::SessionMode>,
```

*Per-session permission flags (in `crates/codegen/xai-grok-pager/src/app/agent.rs` lines 726-735):*

```rust
/// Whether YOLO mode (auto-approve all permissions) is active.
pub(crate) yolo_mode: bool,
/// Whether Auto (LLM classifier) permission mode is active for this session.
pub(crate) auto_mode: bool, // mutually exclusive with yolo_mode (yolo wins)
```

*App-global mirrors (in `crates/codegen/xai-grok-pager/src/app/app_view.rs` lines 942-968):*

```rust
pub default_yolo: bool,            // seeded from --yolo / config, mirrored to new sessions
pub yolo_policy_block: Option<&'static str>,
pub auto_mode_gate: bool,           // feature gate for Auto (default ON), read before every cycle
pub plan_mode: bool,                // --plan CLI flag
pub current_ui: CurrentUi { permission_mode: Option<String> }, // "ask"/"auto"/"always-approve"/"default"
```

**The cycle engine:** `crates/codegen/xai-grok-pager/src/app/dispatch/modes.rs`

Top-level entry points:

```rust
// line 525-555 — agent chat view, plus plan-nudge acceptance bookkeeping
pub(super) fn dispatch_cycle_mode(app: &mut AppView) -> Vec<Effect> {
    let (nudge_showing, in_plan_before) = active_agent_plan_nudge_state(app);
    let mut effects = collapse_to_ask_for_nudge_jump(app).unwrap_or_default();
    effects.extend(dispatch_cycle_mode_and_sync(app));
    // if nudge was showing and we just entered Plan, log ContextualTip::Accepted
    // and clear the nudge
}

// line 602-607 — shared body used by both agent view and dashboard peek (no telemetry)
pub(super) fn dispatch_cycle_mode_and_sync(app: &mut AppView) -> Vec<Effect> {
    app.permission_mode_from_soft_default = false;
    let effects = dispatch_cycle_mode_inner(app);
    sync_active_auto_flag(app);
    effects
}
```

**The ring itself:** `dispatch_cycle_mode_inner` (lines 631-968) — the file to edit.

Simplified truth table (with `auto_gate == true`, the prod default; `auto_gate == false` collapses Auto):

| `(in_plan, in_auto, in_yolo)` | Match arm | Next mode | Effects emitted |
|---|---|---|---|
| `(false,false,false)` | `Normal → Plan` | Plan | `SetSessionMode(Plan)` + banner "Plan" |
| `(true,false,false)` | `Plan → Auto` | Auto (classifier) | `SetSessionMode(Default)` + `PersistPermissionMode("auto")` + banner "Auto" |
| `(false,true,false)` | `Auto → AlwaysApprove` | YOLO | `PersistPermissionMode("always-approve")` + banner "Always-Approve" |
| `(false,_,true)` | `AlwaysApprove → Normal` | Normal | `PersistPermissionMode("ask")` + banner "Normal" |
| `(true,true,false)` | `Plan+Auto → Auto` | Auto (explicit, keeps classifier) | `SetSessionMode(Default)` + `PersistPermissionMode("auto")` |
| `_` (weird combos, notably `Plan+yolo`) | catch-all | Normal | conditional `SetSessionMode(Default)` + `PersistPermissionMode("ask")` |

Pre-session path (no `session_id` yet, lines 650-770) mirrors the same table but mutates `deferred_session_mode` and `app.default_yolo`/`current_ui.permission_mode` locally and stashes `PersistPermissionMode` with `session_id: None` (no ACP push yet; see comment line 759-761: *"Plan is the one mode that replays, via `deferred_session_mode` in `handle_session_created`."*).

Optimism discipline: every `Normal→Plan` writes `agent.plan_mode_pending = Some(true)` immediately; `detect_plan_mode_change` (`app/acp_handler/session_notification.rs:1441-1460`) clears `pending` on the ACP `CurrentModeUpdate` echo:

```rust
pub(super) fn detect_plan_mode_change(update: &acp::SessionUpdate, agent: &mut AgentView) -> bool {
    let acp::SessionUpdate::CurrentModeUpdate(cmu) = update else { return false; };
    let mode = SessionMode::from_id(cmu.current_mode_id.0.as_ref());
    let now_active = mode.is_plan();
    agent.plan_mode_active = now_active;
    agent.plan_mode_pending = None;
    // ...
    true
}
```

Nudge jump (lines 558-595): if the 3-second `PlanMode` ephemeral tip (`tips/plan_nudge.rs`) is showing and the agent is in `Auto` or `yolo`, a Shift+Tab first collapses to `ask` (silent, no banner) before the ring advances, so one chord always lands on Plan. The dashboard peek (`dispatch_cycle_mode_and_sync`) deliberately skips this — it must not attribute a nudge acceptance for an agent the user isn't viewing.

Policy pin: `yolo_policy_block` (managed policy disallows always-approve) is captured before borrowing `agent` (line 637-639) and tested on every `→ Always-Approve` arm, falling back to Normal + toast.

Welcome-screen handling (`app/app_view.rs:3726-3728`): even without an agent, `is_shift_tab` → `ActionThenForward(NewSession)` so a Shift+Tab before the first prompt still advances the cycle via the pre-session path.

### 1.4 Input routing order (who steals the chord)

`app/agent_view/input.rs` — three-level bubbling documented in `app/agent_view/mod.rs:8-50`:

```
key press
 → overlays/modals/dropdowns steal Esc first
 → 1. pane level (PromptFocused registry → Tab => FocusScrollback)
 → 2. agent level (AgentScreen registry → Ctrl+O ToggleYolo, Ctrl+C cancel)
       From prompt pane, Ctrl+C on non-empty prompt skips AgentScreen promotion
       (falls to widget clear), so bare prompt doesn't cancel.
 → 3. Esc policy (try_handle_esc_policy) — turn/clear/rewind
 → 4. bubble to app_view global (quit, NewSession)
```

`PromptWidget` deliberately does **not** special-case Shift+Tab (`app/agent_view/prompt.rs:520`):

```rust
// Shift+Tab (cycle session mode) is not special-cased here — the
```

So the `CycleMode` `When::PromptFocused` binding is reached via the `registry.lookup` at level 1, not hard-coded. Test `prompt::shift_tab_cycle_mode_tests` (prompt.rs:1086-1120) asserts that Shift+Tab emits `Action::CycleMode` through `handle_key` and that multiline `Shift+Tab` with a non-empty draft still cycles (doesn't insert a newline).

### 1.5 Mode-switch banner (UX affordance rivo should keep)

`app/agent_view/mod.rs:458` and `1278-1281`:

```rust
pub const MODE_BANNER_FADE_TICKS: u8 = /* 2s full + 0.3s fade, driven per tick */;
pub(crate) mode_switch_banner: Option<(String, u8)>, // ("Plan"/"Auto"/"Always-Approve"/"Normal", ttl)
fn show_mode_switch_banner(&mut self, label: &str) { self.mode_switch_banner = Some((format!("Switched to mode: {label}"), MODE_BANNER_FADE_TICKS)); }
```

Rendered in `app/agent_view/render.rs` banner rect (see §7 for layout) and tested via `links.rs:2298-2306` — `show_mode_switch_banner("PlanMode")` frame contains "Switched to mode: PlanMode".

### 1.6 What needs to change for rivo

| Area | File | Change |
|---|---|---|
| **SessionMode enum** | `crates/codegen/xai-grok-tools/src/types/session_mode.rs` | Add `Ask`, `Debug`, `Multitask` variants (keep wire as `snake_case`; update `is_plan()` or add `is_ask()`, etc.; document that unknown → `Default` keeps old pagers non-bricking). |
| **Action description** | `crates/codegen/xai-grok-pager/src/actions/defaults.rs:519-533` + dashboard mirror 982+ | Update `label`/`description`/`long_help` to `"Cycle mode (Normal / Plan / Ask / Debug / Multitask)"` and long_help enumerating each. Keep `shift_tab_keys()` binding unchanged. |
| **Cycle table** | `crates/codegen/xai-grok-pager/src/app/dispatch/modes.rs:631-968` | Replace 3×3 ring with 5-state ring. Recommend `Normal → Plan → Ask → Debug → Multitask → Normal`. Each new state must mirror both the `in_plan/in_auto/in_yolo` tuple **and** the new `SessionMode`-backed ask/debug flags. Simplest: introduce `AgentMode` pager enum (Normal/Plan/Ask/Debug/Multitask) stored in `AgentView` + `AppView` like `plan_mode_active`, decoupling plan's dual-flag (`pending/active`) from permission flags (`yolo/auto`). Alternatively, repurpose `Ask` as `SessionMode::Ask` (already deserializes to `Default` today — see `session_mode.rs:47-52` — so wire change is backwards-compatible) and add a separate `debug_active` boolean. |
| **Pre-session path** | same file 650-770 | Mirror new ring in the `session_id==None` branch; stash the new modes in `deferred_session_mode` (currently `Option<SessionMode>` — widen to new enum) so the `SessionCreated` handlers replay them. At least `Ask` and `Plan` must replay; `Debug`/`Multitask` can be best-effort. |
| **Permission interaction** | same file | Make `YOLO` **orthogonal**: today YOLO is a **step in the ring** (`Auto→AlwaysApprove→Normal`). For rivo, YOLO must be a `Ctrl+O` toggle that is **not** a ring step (Cursor Run Mode orthogonal to AgentMode). Two options: (A) keep YOLO in the ring but also allow `Ctrl+O` to toggle arbitrarily — risks confusing double entry. (B) Remove YOLO from the ring entirely (`Normal→Plan→Ask→Debug→Multitask→Normal`) and keep `always-approve` only via `Ctrl+O`/`/always-approve`/`--yolo` (recommended; matches `cursor-research.md:140`). If (B), delete the `(false,_,true)` and `(false,true,false)` arms. |
| **Banner + telemetry** | `app/agent_view/mod.rs:1278`, `dispatch/modes.rs` tracing + telemetry | Add banners `"Ask"`, `"Debug"`, `"Multitask"`; wire new `log_event` if Cursor parity telemetry desired. |
| **CLI flags** | `crates/codegen/xai-grok-pager/src/app/cli.rs:277-278`, `headless.rs:50` | Add `--mode=ask|plan|debug|multitask` (or `--ask` shorthand like `--plan`) and wire to `default_yolo`/`plan_mode` seeding. `cursor-research.md:31` lists `--mode=ask`. |
| **Slash commands** | `slash/commands/` | Add `/ask`, `/debug`, `/multitask` (mirroring `/plan` in `commands/plan.rs:1-60`) or a unified `/mode <name>`; add mode indicator sync fix already noted in plan. |
| **Tests** | `app/dispatch/tests/modes.rs`, `views/agent_status.rs`, `input/key.rs` | Expand `shift_tab_cycles_*` tests to assert the 5-step ring; do not regress `is_shift_tab` encodings. |
| **Settings** | `settings/defs.rs:1476`, `settings/registry.rs:292` | Mirror mode list in settings catalog if exposed; gate descriptions. |

**Do not touch:** `input/key.rs` shift-tab encodings/hit-testing (they are correct), welcome-screen forwarding in `app_view.rs:3726`, nudge-jump eligibility predicate (fix after ring settles).

---

## 2. Plan Mode — Current Implementation

### 2.1 User-facing contract

Source of truth is the bundled user guide shipped with the binary:

*   `crates/codegen/xai-grok-pager/docs/user-guide/19-plan-mode.md` (copied verbatim into the review prompt for this doc in context).

Key quotations:

> *Plan mode is read-only except for the plan file: plan-file edits (`plan.md` in the session directory) are auto-approved, and edits to any other file are rejected outright — the tool call fails with a short message naming the plan file as the only editable path. This holds in every permission mode, including always-approve.* (line 14-16)

> *State machine: `Inactive → Pending (toggle on) → Active → ExitPending (toggle off mid-turn) → Inactive`* (`Inactive → Active` also via `enter_plan_mode` tool skipping Pending). Transient `Pending`/`ExitPending` collapse to `Inactive` on restart. (lines 104-125)

> *Approval view: `a` Approve (`approve w/ comments` when comments pending), `s` Request changes → focus prompt → Enter, `c` Comment on line/range, `y` Copy plan, `q` Quit. `Tab` switches preview/prompt. `Ctrl+P` still switches model before `a`.* (lines 71-90)

> *YOLO stays armed underneath plan mode — non-edit tools still auto-run; once approved, always-approve resumes.* (lines 134-139)

### 2.2 Tool definitions (agent side, `xai-grok-tools`)

**`EnterPlanModeTool`** — `crates/codegen/xai-grok-tools/src/implementations/grok_build/enter_plan_mode/mod.rs`:

*   `ToolMetadata`: `kind = ToolKind::EnterPlan`, `tool_namespace = GrokBuild`, `is_read_only = true`, `emitted_notifications = ["PlanModeEntered"]`, `requires_expr` mutually requires `ExitPlanModeTool` (dead-end guard, lines 61-72).
*   `Tool::run`: resolves `resolve_plan_file_path(&res)` (from `PlanFilePath` resource or `Cwd + /.grok/plan.md`), sends `PlanModeEntered { tool_call_id }` notification, seeds empty `plan.md` if missing via `probe_or_create_empty_plan_file` (never truncates; maps `IsADirectory → NotAFile`, `NotFound → Empty via write`, other read errors → `Inaccessible` without write — lines 181-214). Returns `EnterPlanModeOutput::Entered { message, plan_file_path, tool_hints, plan_file_seed }` where `tool_hints` are looked up via `TemplateRenderer::tool_for_kind` for `AskUser/ExitPlan/Task` (lines 126-141).
*   Description template (line 59): *"Use this tool when a task has ambiguity ... create an implementation plan"* — this is the model-facing instruction.

**`ExitPlanModeTool`** — `crates/codegen/xai-grok-tools/src/implementations/grok_build/exit_plan_mode/mod.rs`:

*   Dual role: reads `plan.md` from async FS (or `tokio::fs` fallback), sends `PlanModeExited { tool_call_id, plan_content, plan_file_path }` notification, returns `ExitPlanModeOutput::{PlanReady{message, plan_content, plan_file_path} | EmptyPlan{message, plan_file_path}}`. `is_read_only = true`. Mutually requires `EnterPlanModeTool`.
*   `PlanModeExited` drives `plan_approval_view.rs` on the pager; `PlanReady.message = "Your plan has been approved. You can now start coding."`.

**Registration** ( `xai-grok-tools/src/registry/mod.rs:682-706` builder `new()` registers both `grok_build::ReadFileTool`, `SearchReplaceTool`, `BashTool`, `EnterPlanModeTool`, `ExitPlanModeTool` etc.; they are `finalized` into `FinalizedToolset` per `SessionContext` which stamps `plan_mode: bool` into `SessionFlags.to_meta` → `_meta.yoloMode/autoMode/agentProfile` + `plan` profile selection `grok-build-plan` vs `grok-build-plan-no-subagents` — `app/effects/helpers.rs:270-335`).

### 2.3 Pager side — the pager is the plan gate

#### State and lifecycle

*   **`AgentView` fields** (`app/agent_view/mod.rs:1322-1337`) — see §1.3.
*   **`AppView` flag** (`app/app_view.rs:963-965`) `plan_mode: bool` seeded from `--plan` CLI.
*   **Optimistic pending** — `plan_mode_pending` set to `Some(true/false)` at toggle time (`dispatch/modes.rs:set_plan_mode`, `dispatch_cycle_mode_inner`, `dispatch_enter_plan_mode`) so rapid toggles don't double-send; cleared by `detect_plan_mode_change`.
*   **`CurrentModeUpdate` handler** — `app/acp_handler/session_notification.rs:1426-1460` (snippet in §1.3). Sourced from both user `session/set_mode` and agent `EnterPlanMode/ExitPlanMode` tool completions via the shell's notification bridge (comment line 1429-1438 warns **not** to infer mode from tool titles).

#### Approval UI

*   **Ext handler** — `app/acp_handler/interactions.rs:126-170` `handle_exit_plan_mode`: deserializes `ExitPlanModeExtRequest { session_id, tool_call_id, plan_content }` (from `views/plan_approval_view.rs:types`), parks `PlanApprovalViewState` on the owning `AgentView`, seeds `latest_inline_plan_content`, opens `show_plan_preview_if_available`.
*   **View state** — `views/plan_approval_view.rs:1-160`:

```rust
pub const EMPTY_PLAN_PLACEHOLDER: &str = "# No plan written yet\nThe agent exited plan mode without writing a plan.\n...";
pub enum PlanApprovalFocus { Preview, Prompt, Commenting }
pub enum PlanReviewSource { Inline, FileBacked }
pub struct PlanApprovalViewState {
    pub tool_call_id: String,
    pub has_plan: bool,                // plan_content.is_some() after trim
    pub plan_content: Option<String>,
    pub source: PlanReviewSource,
    pub stashed_prompt: StashedPrompt, // prompt stashed on show, restored on dismiss
    pub response_tx: Option<oneshot::Sender<AcpResult<ExtResponse>>>, // blocks agent until user acts
    pub focus: PlanApprovalFocus,
    pub comments: Vec<PlanComment>,    // {id, line_range: Range<usize>, text}
    pub next_comment_id: u64,
    pub editing_comment_id: Option<u64>,
    pub commenting_range: Option<Range<usize>>,
    pub stashed_feedback_prompt: Option<StashedPrompt>,
}
impl PlanApprovalViewState {
    fn new(request, stashed_prompt, response_tx) -> Self { /* Inline source, has_plan = content non-empty */ }
    fn format_feedback(&self, freeform: Option<&str>) -> String { /* "Proposed plan line N:" + comments + freeform */ }
    fn send_approved/cancelled/abandoned(&mut self) { /* completes response_tx with ExtResponse */ }
}
```

*   **Surfaces** ( `app/agent_view/plan.rs` ):

```rust
// line 68-80 — chip (used by status bar, §5)
pub(super) fn should_show_plan_chip(&self, appearance: &AppearanceConfig) -> bool {
    (self.plan_mode_active || appearance.show_plan_chip) && self.plan_preview_available()
}
fn plan_body_for_preview(&self) -> Option<String> {
    // prefers pav.plan_content → latest_inline_plan_content → read plan.md from sessions dir
}
fn show_plan_preview(&mut self) {
    // opens LineViewerState::open_markdown_content("plan.md", content) fullscreen
    // with plan_comments rebuilt into viewer; sets viewer.plan.feedback_active
}
fn approve_plan / abandon_plan / close_plan_review { // optimistically plan_mode_pending=Some(false), clear latest_inline, restore prompt, log PlanSubmit::Approved
}
fn handle_plan_feedback_key(&mut self, key) -> InputOutcome { // Tab toggles Preview/Prompt, Esc back, 'a' approves when empty prompt+no comments, Enter sends revision
}
/// casual commenting (outside plan approval — the line viewer preview via /view-plan)
fn enter_casual_plan_commenting / save_casual_plan_comment / send_casual_plan_comments
```

*   **Shortcuts bar while parked** — `app/agent_view/render.rs:103-145` `plan_approval_shortcut_hints` returns `Save comment / cancel` (Commenting), `request changes / plan / back` or `approve / plan / back` (Prompt, depending on whether revision text/comments exist), `copy plan / prompt` (Preview).

*   **Lifecycle note** (`app/agent_view/plan.rs:224-240`): `close_plan_review` flips `plan_mode_pending = Some(false)` optimistically because *"the shell's confirming `CurrentModeUpdate("default")` only arrives after the exit tool runs (and can be lost entirely). Resolving the review with a decision must therefore optimistically clear the effective plan mode"* — the badge would otherwise stick on "plan".

#### Plan file path

`app/agent_view/plan.rs:30-40`:

```rust
fn plan_file_path(&self) -> Option<PathBuf> {
    let session_id = self.session.session_id.as_ref()?;
    let cwd_str = self.session.cwd.to_string_lossy().into_owned();
    let encoded_cwd = urlencoding::encode(&cwd_str);
    Some(grok_home().join("sessions").join(encoded_cwd.as_ref()).join(session_id.0.as_ref()).join("plan.md"))
}
```

The shell resolver `xai_grok_tools::types::resources::resolve_plan_file_path` prefers `PlanFilePath` resource, then `Cwd + .grok/plan.md` (see `enter_plan_mode/mod.rs:350-610` tests `uses_plan_file_path_resource_when_set`, `falls_back_to_cwd`).

#### Slash commands that enter/exit plan

*   `slash/commands/plan.rs:1-60` `PlanCommand` — `/plan` → `SetPlanMode(On)`, `/plan <desc>` → `EnterPlanMode { description }`. Dashboard `offered_when_session_less = true` so `/plan` on the welcome screen stages `deferred_session_mode`.
*   `slash/commands/view_plan.rs` `ViewPlanCommand` — `/view-plan` (aliases `show-plan`, `plan-view`) → `ShowPlan` action, delegates to `show_plan_preview` / `reopen_plan_approval` (`app/dispatch/modes.rs:11-24`).
*   Settings modal `plan_mode` toggle — `app/dispatch/modes.rs:127-189` `set_plan_mode(kind)`  is the pager-owned optimistic path with `SetSessionMode` effect; idempotent no-op toasts `save_success_toast("Plan mode", ...)`.

#### Docs emitted to the user

`app/docs.rs:140` lists `"19-plan-mode.md"` among the bundled docs searchable via `/docs`.

### 2.4 What needs to change for rivo

*   **Keep it** — do not re-invent plan mode. The existing implementation already satisfies Cursor Plan Mode's 5-step flow (`cursor-research.md:43-52`) and is the correct foundation for rivo (`cursor-research.md:53-55`). The only changes are integration:
*   **Wire into the new ring** — `SessionMode::Plan` stays canonical (wire `"plan"`). In the new 5-state `AgentMode`, Plan is a genuine ring step (today it is). Keep `enter_plan_mode`/`exit_plan_mode` tools untouched.
*   **Add CLI `—mode=plan` parse alongside `--plan`** — `app/cli.rs` already has `no_plan` inversion (`event_loop.rs:832 app.plan_mode = !args.no_plan`); add `--mode plan` as an alias that sets the same flag.
*   **Ask vs Plan overlap** — Ask is read-only without a plan file; Plan is read-only except `plan.md`. Ensure the Ask tool filter (see §4) does not accidentally restrict plan-file writes when in Plan mode (Plan's allowlist is `Ask ∪ {plan.md write}`).
*   **Debug uses Plan's read-only shape temporarily** — the `hypothesize→instrument` phase of Debug is read-only-ish but intentionally writes instrumentation files (see `cursor-research.md:61-73`). Don't conflate Debug with Plan's strict plan-file-only gate.
*   **No new plan-file location** — Cursor stores plans under `~/.cursor/plans/*.md` or `~.md` with "Save to workspace → `.cursor/plans`". Grok stores under `sessions/<encoded_cwd>/<session_id>/plan.md`. Keep grok's location for now (simpler isolation + already wired). If rivo later wants a global `~/.rivo/plans`, add a second writer; don't move the primary.

---

## 3. Permission / Yolo / Always-Approve

### 3.1 What it is called where

*   **CLI:** `--always-approve` with alias `--yolo` (`app/cli.rs:277-278`):

```rust
#[arg(long = "always-approve", alias = "yolo")]
pub yolo: bool,
```

Same alias duplicated for the `new`/`resume` subcommand (`cli.rs:451-452`). Headless `HeadlessArgs` also carries `yolo: bool` (lines 50-803) and seeds `agent_config.default_yolo_mode` / `SessionFlags.yolo_mode` into `_meta.yoloMode` (acp/mod.rs:152-155, 183-184, effects/helpers.rs:278-360). Always emitted, not gated.

*   **In code:** `yolo_mode: bool` (raw), `PermissionModeKind` enum (typed). `is_yolo()` / `set_yolo_mode_for_test()`. Global `app.default_yolo`, per-session `agent.session.yolo_mode`, ephemeral display mirror `app.current_ui.permission_mode: Option<String>` (`"ask"/"auto"/"always-approve"/"default"`). `Auto` is `agent.session.auto_mode` (classified, `!yolo && auto` precedence — `dispatch/modes.rs:216-223` `effective_auto`).

```rust
// app/actions.rs:1032-1082
pub enum PermissionModeKind {
    Default,        // yolo=false, canonical "default"
    Ask,            // yolo=false, explicit "ask"
    Auto,           // yolo=false, auto_mode=true, canonical "auto" — LLM classifier
    AlwaysApprove,  // yolo=true, canonical "always-approve"
}
impl PermissionModeKind {
    fn as_canonical(&self) -> &'static str { match self { Self::Default=>"default", Self::Ask=>"ask", Self::Auto=>"auto", Self::AlwaysApprove=>"always-approve" } }
    fn is_always_approve(&self) -> bool { matches!(self, Self::AlwaysApprove) }
    fn is_auto(&self) -> bool { matches!(self, Self::Auto) }
}
```

*   **User copy / toasts:** "Always-approve" (symbol ⚠). `YOLO_ON_UNDER_PLAN_TOAST = "⚠ Always-approve ON: plan mode still blocks file edits until you exit plan mode"` (lines 494-496). `yolo_toast(true) = "⚠ Always-approve ON: all tool actions auto-run"` (500-502).

Note: there is **no** `Ctrl+O` in upstream Cursor — the doc clears that up (`cursor-research.md:117` — *"No existe Ctrl+O para Yolo en Cursor (confusión con Claude Code). Cursor usa /run-everything"*). In grok, `Ctrl+O` is the native chord (see §3.3). Rivo keeps `Ctrl+O` as the global toggle.

### 3.2 How `always_allow` / auto Approve works today

*Not a single config table — it is per-permission-request routing in `app/acp_handler/permissions.rs:20-90`.*

```rust
pub(super) fn handle_permission_request(perm: AcpArgs<RequestPermissionRequest>, app: &mut AppView) -> bool {
    // 1. route by session_id (including subagent sessions)
    let matched = find_session_match(app, &perm.request.session_id) ...
    let agent = app.agents.get_mut(&owning_agent_id) ...

    // 2. YOLO fast path — on the OWNING agent (so background turns don't block)
    if agent.session.is_yolo()
        && let Some(allow) = perm.request.options.iter().find(|o| o.kind == AllowOnce)
    {
        perm.response_tx.send(Ok(RequestPermissionResponse::new(Selected(allow.option_id.clone())))).ok();
        return false; // no redraw
    }
    // 3. notification bell on first queued permission
    // 4. enqueue_permission (build bash/mcp highlights, stash prompt, push PermissionViewState, stamp last_active_at)
}
```

Fallthrough rule: if no `AllowOnce` option exists, YOLO **does not** pick `AllowAlways` by default — it falls through to the interactive queue (`permissions.rs:43-46` comment *"won't pick AllowAlways by default"*). The same predicate routes `set_yolo_mode_inner`'s drain of the already-queued `permission_queue` ( `dispatch/modes.rs:304-330` — drains preferring `AllowOnce`, else `Cancelled`, never `AllowAlways`).

Per-tool `AllowAlways` semantics (`permissions.rs:374-422` — `is_edit_permission` sniffs `AllowAlways` name containing `"edit"`; `mcp_scope` parses the `allow-always-mcp` option's meta for `McpToolPermission { tool_name, server_prefix, selected: Tool|Server }`). Used for the "Always allow Edit/this MCP" checkboxes inside the permission cards, not the global YOLO switch.

### 3.3 `Ctrl+O` handling — every path that can steal it

`Ctrl+O` is **overloaded**; the pager resolves it by screen/promo state. Call sites in precedence order (highest listed first where a promo guard fires):

*   **`actions/defaults.rs:734-736`** — agent-screen binding:

```rust
ActionDef { id: ActionId::ToggleYolo, label: "yolo", description: "Toggle always-approve",
            default_key: key!('o', CONTROL), alt_keys: vec![],
            context: When::AgentScreen, ... }
```

*   **`actions/defaults.rs:1093-1110`** — dashboard mirror `DashboardToggleAutoApprove` (same `Ctrl+O` chord, `When::DashboardOverlay` etc., delegates to `ToggleYolo` on the selected dashboard row).

*   **`app/agent_view/input.rs:325-330`** — agent-level chord checked inside `handle_key` before bubbling to global. Preserved in docs `agent_view/mod.rs:16-26`.

*   **`app/actions.rs:437-452`** — `CycleMode` vs `ToggleYolo` are distinct actions; `Ctrl+O` never enters the plan ring — it flips YOLO directly.

Conflicts that **shadow** `Ctrl+O`:

*   In minimal mode, `Ctrl+O` opens the transcript pager (`app_view.rs:4299-4305`: *"In minimal mode Ctrl+O routes to Action::OpenTranscriptPager (unless overridden)"*). Only in that path; full TUI keeps it as YOLO. Apple Terminal special-casing: `In interject = Ctrl+O` mapping collides with minimal transcript mapping — see tests `7798-7912` (idle vs running, queue payload, Apple Terminal chord table).
*   When a **pinned non-dismissible promo CTA** is live, `Ctrl+O` opens that CTA instead of YOLO (`app_view.rs:3148-3160`, `agent_view/mod.rs:1289-1292`, guarded by `pinned_upgrade_cta_live`). Dispatch re-resolves the gate (stale-by-one-frame safe). Covering tests in `agent_view/links.rs:998-1082`: pinned promo → Ctrl+O opens CTA; dismissible promo → Ctrl+O still YOLO.
*   Interject while running (`agent_view/input.rs:338-369` minimal-only `/btw` ownership path suspends/minimal lifecycle) can also eclipse it.

So: **rivo global YOLO must be careful in two regressions** — minimal mode and pinned promo guards. Both are exercised by `links.rs` and `app_view.rs` tests — update them alongside any mode-badge change.

### 3.4 Can it be made global? — Yes, and the seams are small

Today YOLO is:

*   per-session `agent.session.yolo_mode` + global mirror `app.default_yolo` (write-only mirror — `dispatch/modes.rs:277-283` *"The global mirrors update unconditionally ... per-agent state is gated below"*).
*   Already broadcast to the shell per-session via ACP `x.ai/yolo_mode_changed` with `{ yolo_mode: bool, auto_mode: bool, permission_mode: "always-approve"|"ask"|... }` (effects/helpers.rs:1394-1430, dispatch/modes.rs:146-165).
*   The shell's `_meta.yoloMode` seed at `NewSessionRequest` reads `default_yolo` (agent_config/default_yolo_mode) so new tabs inherit the current global.

What "global" means for rivo vs today:

| Today | Rivo target (`cursor-research.md:118-120`) |
|---|---|
| Toggle touches active `AgentView` and `app.default_yolo` but **does not** drain other live agents' queues (only active `agent.permission_queue.drain` in `set_yolo_mode_inner`). Background turns rely on step-2 fast-path at permission-arrival time, so they **do** auto-approve subsequent requests, but queued-before-toggle permissions on inactive agents remain stuck. | **True global:** `Ctrl+O` drains **every** live agent's `permission_queue` (and stashes restore), broadcasts `x.ai/yolo_mode_changed` for each `session_id`, persists `permission_mode` once, badges `· YOLO` on every agent row/status bar, and suppresses every future `ask_user_question`/`AllowOnce` gate in main/side/subagent/window until toggled off. Combinable with every AgentMode badge (`Rivo · Ask · YOLO`, `Rivo · Debug · YOLO`, ...) . |
| YOLO is both a ring entry **and** a separate chord — can be entered two ways. | **Orthogonal:** YOLO is **not** a ring entry (remove from Shift+Tab; see §1.6). Only `Ctrl+O` / `--yolo` / `/always-approve` toggles it. Badge is `· YOLO` independent of plan/ask/debug. |

**Where to change (minimal diff):**

1.  `app/dispatch/modes.rs:274-340` `set_yolo_mode_inner` — hoist `agent.session.yolo_mode = new` loop over `app.agents.values_mut()` after `app.default_yolo = new`, and drain `agent.permission_queue` + `restore_permission_stashes` per agent (today `restore_permission_stashes` already handles stray stashed prompts across agents — keep it).
2.  `app/effects/helpers.rs:1394-1515` `persist_permission_mode_and_notify` + `PersistPermissionMode` handling — today it takes `session_id: Option<SessionId>` and sends one notification. Change the `BestEffort` cycle path to fan-out one notification per live `session_id` (loop over `app.agents.keys()` captured before async). Keep `WithRollback` single-session for the typed setter failure rollback.
3.  `app/acp_handler/permissions.rs:20-65` fast-path already **does** consult `agent.session.is_yolo()` on the owning agent, so a made-global YOLO (all agents' `yolo_mode = true`) is already correct there — no edit needed, just ensure the global drain completed.
4.  `app/app_view.rs:942-956` — `default_yolo` comment already calls it *"Default YOLO for new sessions"*; keep it but rename mentally to "global YOLO mirror" — no field rename required initially (avoid churn), just update doc comments and `surface_yolo_launch_block_notice` (`dispatch/ctx.rs:187-268` reanchors `app.default_yolo` on every session spawn).
5.  Badge — §5.
6.  Tests — `app/dispatch/tests/permissions.rs:259`, `app/dispatch/tests/modes.rs:1893-1945`, `app/acp_handler/tests/permissions.rs`, `actions/defaults.rs` yolo mirror description.

Managed-policy pin `app.yolo_policy_block` (`AppView:953-956`, `dispatch/modes.rs:198-212` `yolo_enable_blocked`/`refuse_if_yolo_locked`) applies on **every enabling path** — keep that guard unchanged for rivo's global path. It already short-circuits both ring and Ctrl+O.

### 3.5 Other permission surfaces to keep consistent

*   `app/effects/helpers.rs:278-360` `SessionFlags { yolo_mode, auto_mode, plan_mode }` stamps `_meta` at session create. Today `effective_auto(!yolo && auto)` precedence is sacred — every site calls that helper. Keep.
*   `headless.rs:803-805` seeds `agent_config.default_yolo_mode = options.yolo` — wire `—yolo` / `—mode=...` into that.
*   `/always-approve` slash (`slash/commands/always_approve.rs:1-40` — one-liner `ToggleAlwaysApprove -> Action::SetYoloMode(!ctx.pager_state.yolo_mode)`) stays but surface it as `/yolo` alias for discoverability if desired (add to `aliases()` in that command).
*   Settings modal `permission_mode` registry (`settings/defs.rs`, `settings/registry.rs:292` permission-mode picker hides "Auto" choice when `auto_mode_gate==false`) — keep lockstep via `downgrade_displayed_auto_if_gated`.

---

## 4. Tool Allowlist — How `AgentMode::Ask` Could Filter Tools

### 4.1 Where `read_file`, `search_replace`, `run_terminal_command` live

Monorepo `crates/codegen/xai-grok-tools`:

*   **Type-level taxonomy** — `src/types/tool.rs:70-118` (`ToolKind` enum, `ToolNamespace`, `ToolKind::is_read_only()` default, `VARIANT_COUNT` etc.) and `src/tool_taxonomy.rs:79+` (kind → description/template, `read_only` metadata).
*   **Concrete tools** — `src/implementations/` subdirs:
    *   `grok_build/read_file/mod.rs` (ToolKind::Read, `is_read_only: true`), `grok_build/search_replace/mod.rs` (Kind::Edit), `grok_build/bash/mod.rs` (Kind::Execute, `is_read_only: false` — lines 47-300, requires `BackgroundTaskAction/KillTaskAction` tool kinds), `grok_build/list_dir/mod.rs` (Kind::List), `grok_build/grep/mod.rs` (Kind::Search), `grok_build/enter_plan_mode`, `exit_plan_mode`, `ask_user_question`, `todo`, `task` (subagents), `monitor`, `scheduler/*`, `codex/*` (apply_patch/grep/read/list_dir), `opencode/*` (bash/read/edit/grep/glob/skill/todowrite), `memory/search_tool`, `search_tool`, `use_tool`, `web_search/*`, `web_fetch/*`, `skills/*`, `lsp/*`, `image_gen` etc. (registry builder `src/registry/mod.rs:450-760` `ToolRegistryBuilder::new()` registers ~30 built-ins plus `register_tool_pack` extensibility).
*   **Registry and filtering** — `src/registry/mod.rs`, `src/registry/types.rs`, `src/types/resources.rs` (`SharedResources` bag of `Terminal`, `FileSystem`, `Cwd`, `NotificationHandle`, `PlanFilePath`, `TemplateRenderer`, etc.). Filtering today is **requirement-based** (`requires_expr` `ToolRequirement::Tool{namespace,id,if_params}` — e.g., `ExitPlanMode` requires `EnterPlanMode` present, `BashTool` requires `BackgroundTaskAction` etc.) and **behavior-preset** versioning, not mode-based.
*   **`is_read_only` duality** — `ToolMetadata::is_read_only()` (kind-derived, line 59 `self.kind().is_read_only()`) vs `Tool::capabilities().is_read_only` per tool must agree (test `registry/types.rs:2523` `capabilities_is_read_only_matches_metadata` pins this — "every registered tool must give the same is_read_only from both surfaces").

Concrete kind predicate that matters for Ask gating (`src/tool_taxonomy.rs:257-262`):

```rust
fn is_read_only_classifies_kinds() {
    assert!(ToolKind::Read.is_read_only());
    assert!(ToolKind::Search.is_read_only());
    assert!(ToolKind::List.is_read_only());
    assert!(!ToolKind::Edit.is_read_only());
    assert!(!ToolKind::Execute.is_read_only());
    assert!(!ToolKind::Delete.is_read_only());
}
```

Per-tool VM list (non-exhaustive from grep):

*   Read-only (`is_read_only: true`): `ReadFileTool`, `GrepTool`, `ListDirTool`, `WebSearchTool`, `WebFetchTool` (gated on enable flag), `LspTool`, `MemorySearchImpl`/`MemoryGetImpl`, `SearchTool`, `EnterPlanModeTool`, `ExitPlanModeTool`, `AskUserQuestionTool`, `TaskOutputTool`/`GetTerminalCommandOutputTool`/`WaitTasksTool` (overridden `is_read_only: true` despite action kinds — `registry/types.rs:2558` covers this), `codex/read_file`, `codex/grep_files`, `opencode/read`, `opencode/grep/glob`, `hashline_read/grep`, etc.
*   Mutating (`is_read_only: false`): `SearchReplaceTool` (and hashline/concise variants), `BashTool`/`BashConciseTool`/`OpenCodeBashTool`/`CodexApplyPatchTool`/`OpenCodeEditTool`/`OpenCodeWriteTool`/`OpencodeTodoWriteTool`/`MonitorTool`/`SchedulerCreate`, `TodoWriteTool`, `WorkflowTool`, `ImageGen/Edit/Video`, `LSP restart` sub-cases etc.

### 4.2 How to make `AgentMode::Ask` filter tools (without forking the universe)

Today there is **no** ask-mode filter. The closest precedent is `requires_expr` gating + plan-mode's **shell-side edit guard** (which rejects non-`plan.md` writes even when `yolo` — `19-plan-mode.md:14-16`; pager also blocks via `detect_plan_mode_change` + approval view overlay, but the **authoritative** reject is shell-side to survive YOLO). Two new seams are needed for Ask:

#### Option A — Client-side tool definition filtering (recommended v1)

Drop mutating tools from the **tool list exposed to the model** when `AgentMode::Ask` is active. The agent then literally cannot see `search_replace`/`bash`/`write`, so the next turn cannot call them without a jailbreak. Precedent: `FinalizedToolset` is finalized **per `SessionContext`** ( `src/registry/mod.rs:676-960` `new() -> finalize(config, ctx)` / `finalize_with_trunc_config` ). `SessionContext` already carries `Cwd`, `SessionFlags { yolo_mode, auto_mode, plan_mode }` (`app/effects/helpers.rs:261-365`). Introduce `ask_mode: bool` along that line, and in `registry/mod.rs` or `bridge.rs` filter `tools` before building `FinalizedTool` / emitting the `tool_definitions` presented to the model. Keeps `is_read_only` duality intact (just don't advertise the tool at all). Fallback for unknown future kinds: preserve `ToolKind::Other`/`kind=None` tools (existing `kind: None` MCP preservation rule in `ToolConfig` doc 71-91 covers this).

Which tools does Ask keep? Per `cursor-research.md:33-37`:

*   **Keep (read-only + interrogative):** `read_file`/`read_file_concise`/`hashline_read`, `list_dir`/`opencode/glob`, `grep`/`hashline_grep`/`opencode/grep`, `codex/read_file` sets, `lsp` (read semantics), `memory_search`/`memory_get`/`search_tool`, `ask_user_question` (`cursor-research.md:31` lists `askQuestions`), `web_search`/`web_fetch`, `task_output` readback. Possibly keep `todo` read? Exclude `TodoWriteTool` (it writes persistence file — treat as mutating).
*   **Drop:** `search_replace` (all variants), any `write`/`edit`/`apply_patch`, `run_terminal_command`/`bash`/`opencode/bash` (all `Execute`), `monitor`/`kill_task`/`scheduler_create`/`delete`, `image_gen/edit` if they produce artifact files (or gate on dry-run flag), `opencode/skill` if it can write.

A future refinement (parity with Cursor's noted `git add/push` leak when sandbox inactive — `cursor-research.md:31`) is to allow a **narrow** read-only `bash` subset (`git status/diff/log` etc.) while Ask is on; if you do that, implement with a dispatch-time `deny_list` check inside `BashTool::run` consulting `Ask` mode from `SharedResources` rather than re-exposing the whole tool.

#### Option B — Shell-side enforcement (mirrors plan's authoritative gate)

Even if the tool definition is dropped client-side, the shell can enforce: reject any mutating `ToolCall` while `SessionMode::Ask` is active, with a short error message *"You are in Ask mode — edits/bashes are blocked; ask clarifying questions or switch mode."* Useful defense-in-depth so a stale model reference can't mutate. Minimal shell change: add `SessionMode::Ask` wire id (same file as plan's `"ask"` — today `session_mode.rs:8` maps known ids `{"default","plan"}` and defaults unknown to `Default`, preserving compat) and check `is_read_only` at dispatch entry. Pager shows identical `Ask` badge as the plan banner but with blue accent.

**Recommendation for rivo v1:** ship **both** — client-side filter (no footgun model sees the tool) + shell fast-reject (prevents jailbreak). Plan already proves this two-layer pattern.

#### What needs to be touched

| File | Lines / symbol | Role |
|---|---|---|
| `xai-grok-tools/src/types/tool.rs:70-105` | `ToolKind` | No edit needed (categories stable); `AskUser`, `Plan`, `EnterPlan`/`ExitPlan`, `Search`, `Read`, `List`, `SearchTool` already exist. If you add `Debug`/`Multitask` wire kinds, add them here. |
| `xai-grok-tools/src/tool_taxonomy.rs:79+` | `is_read_only()` | Stable — don't change (Ask gates via allowlist, not via swapping `is_read_only`). |
| `xai-grok-tools/src/types/session_mode.rs:1-60` | `SessionMode { Default, Plan, Ask }` | Add `Ask` (and `Debug`/`Multitask` if you want shell-visibility). Today `from_id` case-sensitive fallthrough to `Default` — adding `Ask` means `id.parse() -> Ask` succeeds and no longer maps to `Default`. Safe because existing persisted values aren't `ask`. Update `is_plan()` → add `is_ask()`. |
| `xai-grok-tools/src/registry/mod.rs:450-760` | `ToolRegistryBuilder::new`, `finalize` | Add an `Ask` filter branch gated on `ctx.session_flags.ask_mode` or a new `AgentMode` resource before the definition rendering loop (around `kind_to_name`/`kind_params` 970-1000). Prefer allowlist approach (explicit keep set) over blocklist so new mutating tools added later don't leak. |
| `xai-grok-tools/src/types/resources.rs` | `SharedResources` bag | Plumb `Ask` mode bit into `SessionContext` → `SharedResources` → `ToolCallContext.extensions` so a fallback dispatch-time check can reject even if filtering was skipped. |
| `xai-grok-pager/src/app/effects/helpers.rs:261-365` | `SessionFlags { plan_mode, yolo_mode, auto_mode }` | Add `ask_mode: bool` field (and `debug_mode`/`multitask_mode` when they exist), stamp into `_meta` if you want shell-side enforcement; today `to_meta` maps (`plan_mode`, `subagents`, `ask_user`) → `agentProfile` (`grok-build-plan` etc.) — add `sessionMode: "ask"` when needed. |
| `xai-grok-shell/src/session/*` (shell crate, not paged in detail) | Plan edit gate | Mirror Ask gate: if shell sees `SessionMode::Ask` active, reject tool calls whose `ToolMetadata.is_read_only == false` with a short error, just as plan rejects non-plan-file edits. |
| `xai-grok-pager/src/app/agent_view/render.rs:980-1040` | `PromptStyle { accent_color_override, border_color_override }` | For Ask, render placeholder `Ask Rivo...` and blue border (`theme.link_fg` or plan-blue variant). Keep `YOLO` badge layered on top (ask stays read-only even under YOLO — `cursor-research.md:38`). |

Ask interaction with YOLO: *"Respeta Yolo: Ask sigue sin editar aunque Yolo ON."* (`cursor-research.md:37`). Implement by checking Ask **before** the `is_yolo` fast-path in `app/acp_handler/permissions.rs:20-65` — i.e., if `ask_mode`, don't fast-path through YOLO; let the edit gate reject instead.

### 4.3 Tool surface inventory to consult when building the allowlist

Fast way to rebuild the allowlist without re-grepping: `src/registry/mod.rs:676-705` `ToolRegistryBuilder::new()` listing is the canonical registry order (registered alphabetically pre-filter). Snapshot from this revision:

*   Grok-build: `BashTool(Execute)`, `ReadFileTool(Read)`, `SearchReplaceTool(Edit)`, `ListDirTool(ListDir/List)`, `GrepTool(Search)`, `KillTaskTool`, `TodoWriteTool(Workflow)`, `UpdateGoalTool`, `WorkflowTool`, `TaskOutputTool`, `GetTerminalCommandOutputTool`, `WaitTasksTool`, `TaskTool`, `WebSearchTool`, `WebFetchTool`, `LspTool`, `ImageGen/Video/ToVideo/Reference`, `EnterPlanMode`, `ExitPlanMode`, `AskUserQuestion(AskUser)`, `Monitor`, `Scheduler*`.
*   Codex/open-code concises overlap the above with same kinds.
*   Memory: `MemorySearchImpl`, `MemoryGetImpl`.

Gate `web_search`/`web_fetch`/`memory` are already allowlisted by default — keep them.

---

## 5. Status Bar — Where `Rivo · Ask · YOLO` Goes

### 5.1 Two status surfaces (don't confuse them)

*   **Generic widget** — `crates/codegen/xai-grok-pager/src/views/status_bar.rs:1-93` (`StatusBar<'a> { left, center, right }`) — a filler row used by non-agent screens (welcome's `top_bar.rs`, docs etc.). Not the one to edit.
*   **Per-agent status row** — `crates/codegen/xai-grok-pager/src/views/agent_status.rs:40-135` + rendering protocol in `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs:1440-1530`.

The agent's prompt header (`PromptWidget` top rule) and the bottom `shortcuts` bar are distinct surfaces — the status bar is the single `Constraint::Length(1)` row at the very top of `AgentViewLayout` (`views/agent.rs:103-105`):

```rust
pub struct AgentViewLayout { pub status_bar: Rect, pub startup_warnings: Rect, ... }
impl AgentViewLayout {
    pub fn compute(area:Rect, ...) -> Self {
        let mut constraints = vec![
            Constraint::Length(1), // StatusBar
        ];
        // + startup warnings if any, + tasks/catalog/todo gaps+panes, + gap+scrollback+...
        // scrollback = Constraint::Min(5) (the flex row), everything else fixed length
    }
}
```

Rendered per frame by `AgentView::draw` (top of `app/agent_view/render.rs:870+`) — pseudocode of the real site (lines 1445-1518):

```rust
use crate::views::agent_status::AgentStatusBar;
use crate::views::context_bar;
let mut status = AgentStatusBar::new(&theme);
// 1. hover link url (if hovering a link on the bottom row)
if let Some(url) = self.highlighted_link_url() { status.push("link_url", Line::from(Span::styled(display, link_style))); }
// 2. bg-tasks spinner + count (frame = dot_spinner_frames()[(tick/4)%len])
if running_count > 0 { status.push("bg_tasks", Line::from(Span::styled(format!("{frame} {running_count}"), running_style))) }
// 3. plan chip — see §2.3
if self.should_show_plan_chip(&appearance) {
    let mut plan_style = if self.hit_plan_button.hovered { bold } else { fg: accent_plan };
    status.push("plan", Line::from(Span::styled("plan", plan_style)));
}
// 4. goal status chip (Goal: Verifying/Planning/Executing/Paused/Budget/Done + tokens/elapsed)
if let Some(goal) = self.goal_state { status.push("goal", goal_status_line(...)) }
// 5. MCP status chip (MCP (1/4)) — None while total==0 else braille spinner + count
if let Some(mcp_line) = self.mcp_init_progress.as_ref().and_then(|p| mcp_status_line(...)) { status.push("mcp", mcp_line) }
// 6. local-workspace badge, credit bar (gated features)
...
let areas = status.render(buf, layout.status_bar); // layout.status_bar from AgentViewLayout::compute
// areas returned as HashMap<&'static str, Rect> keyed by the ids above ("plan","goal","mcp",...) for hover/click hit testing
```

`AgentStatusBar::render` (lines 80-135) lays items **right-aligned** with ` │ ` (`SEPARATOR` from `views/context_bar.rs`) only *between* items (never leading/trailing). Two separators max for three items. `push` order is display order left→right. Background filled `theme.bg_base`, per-span `bg_base`. Single-item rows have no separators (pinned by tests 898-908).

Model name (`render.rs:962`) and turn-state badges come from other rows (turn status line below `btw_height`, not the status bar). Don't put the Rivo badge there.

The older single-field model-name chip is deprecated — `app/agent.rs:786-788` dims when `model_switch_pending`.

### 5.2 Where the `Rivo · Ask · YOLO` badge goes

**Recommendation:** add it as **leftmost item** in the `AgentStatusBar` push list so it reads left-to-right under the prompt header grouping, or as a distinct **first** right-aligned item with plan precedence. Cursor's Render uses a single badge like `Muse Spark ... always-approve`; for rivo the spec wants `Rivo · <Mode> · YOLO` aggregation.

Example sketch keeping today's `AgentStatusBar` invariants:

```rust
// before the existing push("plan") block in app/agent_view/render.rs:1478
let mode_label = match agent_mode { // new enum from §1.6
    AgentMode::Normal => None, // hide when Normal (or show "Normal" if you prefer)
    AgentMode::Ask => Some("Ask"),
    AgentMode::Plan => Some("Plan"), // already has chip, consolidate
    AgentMode::Debug => Some("Debug"),
    AgentMode::Multitask => Some("Multitask"),
};
let is_yolo = self.session.is_yolo();
let is_auto = self.session.is_auto(); // keep if you keep Auto outside the ring
let badge = match (mode_label, is_yolo, is_auto) {
    (None, false, false) => None,
    (None, true, _) => Some("Rivo · YOLO"),
    (Some(m), false, _) => Some(format!("Rivo · {m}")),
    (Some(m), true, _) => Some(format!("Rivo · {m} · YOLO")),
    // if keeping Auto as a distinct badge
    (_, _, true) if !is_yolo => Some(format!("Rivo · {} · Auto", mode_label.unwrap_or(""))).trim_end_matches(" · ").to_owned().into(),
};
if let Some(text) = badge {
    let badge_style = Style::default().fg(theme.accent_plan).bg(theme.bg_base); // or dedicated rivo accent
    status.push("rivo_mode", Line::from(vec![
        Span::styled(text, badge_style),
    ]));
}
```

If you keep `plan`'s dedicated `push("plan", "plan")` (line 1480-1487), either (A) retire it when the new badge subsumes it (so "Plan" doesn't appear twice), or (B) let the new badge render `"Rivo · Plan · YOLO"` and skip the separate `should_show_plan_chip` push when a mode badge is present (retain `should_show_plan_chip` path for non-Rivo modes if you ship both).

Hover/click behavior: register `hit_rivo_mode` like `hit_plan_button`/`hit_goal_status` ( `app/agent_view/mod.rs` has those hit areas; wire mouse pick tests in `app/agent_view/links.rs` ) if you want the badge clickable (e.g., cycles mode or opens `/mode`).

**Theme:** use `theme.accent_plan` for Ask/Plan (blue family; already what plan chips use — `views/agent_status.rs:259-270`). For YOLO within the badge, prefer `theme.accent_error` or `theme.warning` segment only if you render YOLO as a second span: `vec![Span::styled("Rivo · Ask · ", plan_style), Span::styled("YOLO", error_style)]`. Keep `bg_base` so it composites onto the same status bar row.

**Prompt placeholder and border parity:** mirror §4.2 — distinguish Ask at the editor level too: `placeholder_when_focused: Some("Ask Rivo...")` and blue prompt border (same line `render.rs:960-995` already branches on `effective_plan || casual_commenting` — widen to `effective_plan || is_ask || casual_commenting`, and add `placeholder_override` "Ask Rivo..." there). That's the `cursor-research.md:38` instruction verbatim.

### 5.3 Files touching the status bar that tests gate

*   `views/agent_status.rs:325+` tokens/elapsed/goal/mcp tests (not status-layout).
*   `views/agent.rs` layout compute tests — adding a status chip does not change `AgentViewLayout::compute`'s constraints (it only changes content pushed into the already-allocated `status_bar` rect), so no layout test breakage.
*   `app/agent_view/render.rs` draws everything; `cargo test --lib` covers many rendering predicates — add a small `status_badge_contains_rivo_and_mode` assertion after stabilizing.

---

## 6. Side Chat Architecture — From `/btw` Panel to Cursor-faithful Side Chats

### 6.1 What exists today — `/btw` is a **transient inline panel**, not a side chat

*   **Slash command** — `slash/commands/btw.rs:7-44`:

```rust
pub struct BtwCommand;
impl SlashCommand for BtwCommand {
    fn name(&self) -> &str { "btw" } // no alias today
    fn description(&self) -> &str { "Ask a side question without interrupting" }
    fn session_scoped(&self) -> bool { true }
    fn usage(&self) -> &str { "/btw <question>" }
    fn takes_args(&self) -> bool { true }
    fn args_required(&self) -> bool { true }
    fn run(&self, _ctx:&mut CommandExecCtx, args:&str) -> CommandResult {
        CommandResult::Action(Action::SendBtw(args.trim().to_string()))
    }
}
```

Listing: `slash/commands/mod.rs:80,125` `builtin_commands()` pushes `btw::BtwCommand` alongside the other ~39 commands. Spy: `slash/registry.rs:113-160` gates via `hidden`/`menu_hidden`/`restricted`/`available_tools` — `/btw` is in neither hidden set, so visible everywhere. Offered when sessionful; dashboard's dispatch (`app/dispatch/notes.rs:349-395`) handles it even there.

*   **State + panel** — `views/btw_overlay.rs:1-1050`:

```rust
pub enum BtwOverlayState {
    Loading { question: String },
    Done { question: String, content: Box<MarkdownContent>, scroll_offset: usize },
    Error { question: String, error: String },
}
// height helpers
pub fn btw_panel_height(state: Option<&BtwOverlayState>, panel_width: u16) -> u16 {
    // Loading=3, Error=2+wrapped_error_lines, Done=2+clamped(wrapped MarkdownContent lines, DONE_MAX_BODY_LINES=12)
}
// render_btw_panel renders a rounded bordered box with "/btw <question>" title + "[Esc]" right-aligned hint,
// + scroll position "pos-end/total  ↑↓  [Esc]" when focused & overflow, spinner while Loading, MarkdownContent body when Done
```

*   **Layout slot** — `views/agent.rs:112-430` `AgentViewLayout { btw: Rect, ... }` is a **vertical stack** slot between `scrollback` and `queue` (constraints `if btw_height>0 { push Length(1) + Length(btw_height) }` lines 226-230; rendering order 300-320). `DONE_MAX_BODY_LINES = 12` caps height.

*   **Effect + ACP** — `app/effects/mod.rs:3570-3585` fires `x.ai/btw` ext method (via `Effect::SendBtw { agent_id, session_id, question, minimal_request_id }`), `app/dispatch/notes.rs:349-395` `dispatch_send_btw` stashes prompt text, sets `btw_state = Loading`, fires `Effect::SendBtw`, minimal variant seeds `minimal_api::start_minimal_btw`. Response lands at `app/dispatch/notes.rs:530-560` `handle_btw_response` which either `finish_minimal_btw` or sets `btw_state = Done{question, response}`/`Error` and `btw_focused = true/false`. Dismiss path (`app/agent_view/input.rs:338-620`) handles `Esc` / close hit target / keyboard/mouse scrolling with clamped `scroll_offset = offset.min(total − max_body)` (also `views/btw_overlay.rs:238-240` focusActive guard). Scrollback block `scrollback/blocks/btw.rs` (and `views/btw_overlay.rs:1-8` doc) — dismiss converts the panel into a `BtwBlock` collapsed block in scrollback history.

*   **Minimal mode** — `minimal/api.rs:54-130` `minimal_btw_size_is_paintable`, `minimal_btw_visible_height`, `start_minimal_btw`, `finish_minimal_btw`, handlers for `Suspend/Restore` etc.; `app/agent_view/input.rs:338-430` `handle_minimal_btw_input` owns Esc before the shared router, with `btw_scroll_max` computed exactly like desktop.

*   **Selection + hyperlink parity** — `views/btw_overlay.rs:101-130` `full_selection_model`/`line_plain_text`, `370-465` `map_hyperlinks_to_overlay`/`scan_lines_for_url_overlays` parity with scrollback. Tests `490-1050` cover Markdown-not-raw-source, link-to-overlay mapping, narrow quoi title still registers `[Esc]` hit area.

*   **What it definitely is not** (`cursor-research.md:163-173` tells why Cursor's is different): not durable, not scoped to parent, not carrying hidden parent history, not archivable (deleted on Esc), not focusable via `@-mention`, cannot create nesting, `X` is dismiss not archive.

### 6.2 Slash architecture — what needs to be extended for Side Chats

Existing slash completeness gates (`slash/command.rs:200-250` `takes_args`/`args_required`/`takes_args_now`/`suggest_args`, etc.) and the registry's `key_to_index` + `CommandProvenance` badge ( `CommandProvenance::Builtin|Shell|Skill{source}`) stay.

New commands to add (besides keeping `/btw` as an alias):

*   `/side` (alias `/btw` — Cursor parity, `cursor-research.md:149-156`: "`/side` en chat (alias viejo `/btw`) — abre vacío, con texto lo envía directo") — opens a **durable** side-chat overlay/session, not the transient panel. Dispatcher must distinguish: `/btw <question>` today = transient panel; `/side [question]` (or `/btw`) with durable behavior = new. Options: (A) add `/side` as new `SideCommand` (`session_scoped`) and deprecate transient `BtwCommand` to always run `/side` path after flag day, or (B) keep transient panel for quick "ask without interrupting" (marked Side behavior in minimal) and offer `/side` as the durable sibling (both coexist, minimal uses whichever is applicable). Cursor's history is `/btw → /side` alias consolidation, so (A) with `/btw` as alias of `/side` is clean.
*   Future selection-applied commands: selection text → Ask in Side Chat (requires editor selection plumbing, possibly defer), `Shift+Cmd+S` keybinding (maps to a new `ActionId::OpenSideChat`), `@side-chat:` mention resolver (parser for `@` mention in prompt that hydrates parent context).
*   `registry/blocked_acp_names` guard still covers pager `help/hooks` shadowing; no new ACP-advertised side-chat command expected (client-owned).

### 6.3 What a Cursor-faithful Side Chat requires (spec to build)

From `cursor-research.md:145-175`:

*   **Datatype** (`cursor-research.md:169`):

```rust
// conceptual; exact location pending — own module e.g. `app/side_chat.rs` or `session/side.rs` per cursor-research.md:231
struct SideChat {
    id: SideChatId,           // unique within parent session (e.g. SideChatId(String))
    parent_id: AgentId,       // attached parent agent
    hidden_context: Vec<acp::ContentBlock>, // parent conversation history copied as reference context (model sees, transcript hides)
    transcript: Vec<ScrollbackEntry>,       // only prompt+follow-ups (+ tool results) — not parent history
    durable: bool,            // Cursor: true (persists navigate/close parent)
    archived: bool,            // X archives, not deletes; scoped to parent+workspace
    can_create_nested: false, // enforced — a side cannot create another side
    // extra Cursor notes: scoped to parent agent + workspace; duplicate of hidden_context in model call but not in ScrollbackState
}
```

Storage suggested by `cursor-research.md:211-214`: `~/.rivo/side-chats.json` (index) + `~/.rivo/sessions/side-*.jsonl` (per-chat transcripts), hydrated on restart like plan-mode's `Transient → Inactive` collapse.

*   **Entries (all must be supported):** `/side` (empty opens composer), `/side <question>` sends directly; selection → Ask in Side Chat; `Shift+Cmd+S` (`Shift+Ctrl+S` on non-Mac); plus button (future, leave `/side` as the reliable path).
*   **Context injection:** On each SideChat turn, the pager's dispatch concatenates `hidden_context` (parent history) as **model-only** context (not stored in `scrollback/state`), analogous to how the shell's gateway inject plan reminders. The `scrollback/state` report for the side surface must not contain those blocks — only its own transcript is rendered. Documented as *hidden reference context* (`cursor-research.md:156`: "parent history entra como hidden reference context (model lo ve, no se renderiza en side). Solo prompt+follow-ups del side aparecen en su transcript").
*   **Lifecycle:** follow-ups stay in side (it's a full conversation); `X` archives (keeps in `~/.rivo/sessions/side-*.jsonl` with `archived:true`), not deletes. List shows all side chats (and cloud variants when present) for the parent; pinned ones hoisted (like Agents Window sidebar). Non-nestable — reject nested `SideChatId` spawn.
*   **@-mention re-injection:** typing `@side-chat: retry-policy apply that ...` in main resolves via a prefix-search over the parent's archived+active side chats and pulls `transcript` context (or a summary thereof) back into main's queue entry. Cursor parity requires the resolver ( `cursor-research.md:162-165` ).
*   **Full agent, but defaults to reading/searching:** spec *"Each side chat is a full agent session — it can read files, run commands, and make edits — but it stays separate."* (`cursor-research.md:169`) but defaults to search/read/answer without edits so main stays uninterrupted. Implement by defaulting side sessions to Ask-ish allowlist initially, escalating only on explicit user prompt.
*   **Local-only** (no cloud handoff) initially — `cursor-research.md:169` "Local-only (como Cursor)."

### 6.4 Files to create / change to get there

*   **New module:** `crates/codegen/xai-grok-pager/src/session/side.rs` (or `app/side_chat.rs`) + `crates/codegen/xai-grok-pager/src/slash/commands/side.rs` + renderer `views/side_overlay.rs` or `views/side_chat_pane.rs` (if side chat is an inline overlay sibling to `btw_overlay` initially, graduating to a tile pane when tiling lands).
*   **Modify:** `app/agent_view/mod.rs` — add `side_chats: Vec<SideChat>`, `active_side_chat: Option<SideChatId>` alongside `btw_state` (keep `btw_state` until migration is proven; then remove or keep as `Ask` transient for minimal parity).
*   **Modify:** `slash/registry.rs:113-210` — register `SideCommand` (alias `btw` if you choose single-command migration) and keep `available_tools` gating for tool-dependent side actions if any.
*   **Modify:** `app/effects/mod.rs` — new `Effect::SendSide { side_id, text }` and `Effect::CreateSideChat { parent_id, prompt }` (parallels `Effect::SendBtw`).
*   **Modify:** `views/agent.rs` layout — allocate a second pane for side chat transcript when `active_side_chat.is_some()`. For v1, render it like `btw` (fixed-height stack slot); for v2, in the tiling engine it becomes a real tile with its own scrollback+prompt sub-layout.
*   **Persistence:** `xai-grok-shell` ws path unchanged; this is pager-local storage only (like `plan_approval_view`). Use the same `urlencoding::encode(Cwd)` session path trick as plan files.

---

## 7. Window / Panel Architecture — ratatui Layout Today vs Tiled Tomorrow

### 7.1 Current `ratatui` wiring — single-column, no tiling

Every frame path goes through `AgentView::draw` → `views/agent.rs::AgentViewLayout::compute` → paint via `views/agent_status.rs` / `scrollback` / `views/prompt_widget` / `views/shortcuts_bar` etc.

**`AgentViewLayout::compute` signature** (`views/agent.rs:138-410`):

```rust
pub fn compute(
    area: Rect,
    layout_cfg: &LayoutConfig,      // eff_outer_vpad, eff_hpad_{left,right}, block_pad_*
    scrollbar_cfg: &ScrollbarConfig, // enabled, gap_left/gap_right, scrollbar_bg/fg
    timeline_width: u16,             // 0 = hidden, else scrollbar replaced by rail
    prompt_height: u16,
    tasks_height: u16,                // maybe 0
    catalog_height: u16,
    todo_height: u16,
    queue_height: u16,
    btw_height: u16,                  // btw_panel_height(state, area.width)
    turn_status_height: u16,
    banner_height: u16,               // mode-switch banner + ephemeral tip
    cta_height: u16,                  // plugin CTA row
    follow_ups_height: u16,           // follow-up chips row
    startup_warning_height: u16,
    prompt_gap: u16,                  // 0 or 1 (gap between turn_status and prompt)
    voice_recording_height: u16,      // 0 or 1
    shortcuts_height: u16,
    compact: bool,                    // auto-compact when area.height <= 20
) -> Self
```

**Constraint assembly** (`views/agent.rs:203-360`) is strictly `Layout::vertical`:

```rust
let outer_block = Block::default().padding(Padding::new(eff_hpad_left, eff_hpad_right, outer_vpad, outer_vpad));
let inner_area = outer_block.inner(area);
let mut constraints = vec![ Length(1) /*StatusBar*/ ];
if startup_warning_height > 0 { constraints.push(Length(startup_warning_height)); }
if tasks_height > 0 { constraints.push(Length(pane_gap)); constraints.push(Length(tasks_height)); }
if catalog_height > 0 { constraints.push(Length(pane_gap)); constraints.push(Length(catalog_height)); }
if todo_height > 0 { constraints.push(Length(pane_gap)); constraints.push(Length(todo_height)); }
constraints.push(Length(status_gap));
constraints.push(Min(5)); // scrollback — the one flex row
// then fixed slots in strict order: btw, queue, turn_status, banner, cta, follow_ups, prompt_gap, voice, prompt, shortcuts_gap, shortcuts
let chunks = Layout::vertical(constraints).split(inner_area);
// then sequential destructure into fields in identical order
Self { status_bar, startup_warnings, tasks, catalog, scrollback, todo, queue, btw, turn_status, banner, plugin_cta, follow_ups, voice_recording, prompt, shortcuts, ... }
```

Notes:

*   Every pane is a **vertical stripe**; `todo/catalog/tasks` are *above* scrollback, `btw/queue/turn_status/banner/cta/follow_ups` are *below* scrollback / above prompt. Short terminals (`area.height <= SHORT_TERMINAL_ROWS=16`) force `cta_height=follow_ups=0` and `outer_vpad/bottom_vpad=0` to preserve prompt/scrollbackBudget. Auto-compact (`<=20` rows) is derivation (`effective_compact`) never persisted.
*   Scrollbar/timeline are not pane rects but gutter columns derived *after* the vertical split (`scrollbar_x = area.right() - gap_right - 1`, `timeline_x = scrollbar_x +1 - timeline_width`, `scrollback_content` narrowed by `gap_left`).
*   No `Layout::horizontal`, no `Constraint::Ratio` tiling, no divisor drag. The only horizontal work is `render_follow_ups`' chip row and scrollbar's 1-col track.

**Pane identities** (`views/agent.rs:30-80`):

```rust
pub enum ActivePane { #[default] Scrollback, Todo, Queue, Prompt, Tasks, Catalog }
pub struct PaneAreas { pub scrollback: Rect, pub todo: Rect, pub queue: Rect, pub prompt: Rect, pub tasks: Rect, pub catalog: Rect }
impl PaneAreas { fn hit_test(&self, col,row)->Option<ActivePane> { tasks>catalog>todo>queue>scrollback>prompt precedence } }
```

Each pane's chrome is painted via `render_todo_chrome`/`render_todo_chrome_with_close_label` in `views/agent.rs:680-780` (selection border + optional `[close]`). Scrollback selection chrome (`render_entry_hover`, `render_hook_hover_popup`, `render_scrollbar`, `render_todo_badge_spans`) is all vertical-rail work. There is no horizontal pane splitter, no window manager.

**Dashboard and welcome are modal overlays**, not tiles — `app/app_view.rs` plus `views/dashboard/*` and `views/welcome/*` operate as full-screen modes (like agent fullscreen), not panes you can tile beside an agent. The only non-single-column geometry is `views/todo_pane.rs` filtered-list internal wiring, which stays columnar.

**Inline-pane bridge** (`minimal/api.rs` via `xai-grok-pager-minimal`) adds an inline scrollback-native render via `xai-ratatui-inline` (triple buffering, `ScratchBuffer`, `PostFlush` overlay) but keeps the single-column `AgentViewLayout`. Minimal's `minimal_btw_surface_available`/`minimal_btw_geometry_is_paintable` heuristics just decide whether the existing `btw` slot can be painted in the live-region rows, not whether there are multiple columns.

### 7.2 What a tiled rivo wants (from `cursor-research.md:179-217`)

*   **One OS window** like Cursor's Agents Window (`cursor-research.md:186: "Agents Window vs Editor Window"`). Inside it: top-level regions — central prompt window + top-left agent pane (multiple workspaces) + right-side inspection panel (file browser / per-agent sandbox terminal / diff review). For rivo in `ratatui`, translate to: **sidebar** (agent + side-chat list) | **tiled center** (N `AgentView`s arranged) | **inspection drawer** (optional right panel).
*   **Tiled layout spec**: `ratatui::layout::Layout` `Direction::Horizontal`/`Vertical` + `Constraint::Ratio` rows/cols, divisors rendered `│`/`┃` that are `MouseEvent::Drag`-grabbable, keyboard `Ctrl+←/→` (2 cols) / `Ctrl+Shift+←/→` (10 cols) + `/window resize` command, **cap 4 visible panels** then overflow into sidebar switch via `Ctrl+Tab`/`/side switch`, focus expansion to full width. Persistence `~/.rivo/windows.json` (ratios + order) + `~/.rivo/side-chats.json` + per-session `~/.rivo/sessions/side-*.jsonl` rehydrated on restart. Minimum 80×24 gate, `clear();` redraw for column artifact safety.
*   Elements to port: per-agent terminal isolation (reuse `xai-grok-workspace`/`xai-grok-shell` spawn env per Window/SideChat), sidebar pin (star near future search `Cmd+K`), indexing via `xai-codebase-graph`.

### 7.3 What to build — pane manager seams

| Layer | Today | Rivo window manager |
|---|---|---|
| **Single agent render entry** | `AgentView::draw(area, buf, registry, scratch, ...)` takes one `Rect` and renders directly into it via `AgentViewLayout::compute(area, ...)` . | Keep unchanged — render one tiled pane by handing it its allocated tile `Rect` instead of the full screen. The tiled manager loops `for tile in tiles { agent_view_for(tile.agent_id).draw(tile.rect, ...) }`. |
| **Layout math** | `AgentViewLayout::compute` is vertical-only. | Introduce `views/window_manager.rs` (new) that owns `Vec<WindowTile { agent_id: AgentId, rect: Rect, focused: bool }>` and `compute_tiled_layout(area, layout_cfg, tile_count, ratios: Vec<Constraint>) -> Vec<Rect>` using `Layout::horizontal`/`Layout::vertical` nesting. Persistence `~/.rivo/windows.json` (`{ ratios: Vec<f32>, order: Vec<AgentId>, active: AgentId }`). |
| **Input routing** | `AppView` routes keys by `active_view: ActiveView::Agent(id)` + `AgentView::key_owner()` probe that asks `views/prompt_widget` / `views/file_search` / overlay gates. | Extend `AppView` with `WindowManager` owning `active_view` inside tiled set + `Ctrl+Tab` cycling. `handle_key` dispatch becomes `if tiling_active && is_ctrl_tab(key) => manager.cycle_focus(); else if tiling_active && tiling_gestures(key) => manager.resize/move; else agent_view_for(manager.active_agent_id).handle_key(key)`. |
| **Mouse** | `PaneAreas::hit_test` over 6 panes inside one `AgentView`. | Promote to global `WindowHitTest` that maps `(col,row)` → `Option<WindowTileId>` via the current `tiles` rect set after layout; divisor hits produce resize-drag `DragState` (`HitArea` pattern already used for plan/btw/goal hooks — `app/agent_view/mod.rs:1240+` hits). |
| **Resizing** | Only `scrollback` height flex via `Min(5)`. | `Constraint::Ratio` per tile; drag adjusts `ratios` (`ratio = (x - left) / total_width`) snapped to 2-col / 10-col increments matching Cursor parity. `/window resize <delta>` command mirrors prompt's typed numeric control. |
| **App-level rendering** | `AppView::draw` (not seen here, but sibling to `AgentView::draw`) orchestrates welcome/dashboard/agent. | Wrap agent branch: `if tiling_active { window_manager.draw(...) } else { agent.draw(area, ...) }`. Minimal mode stays untiled (single live-region) — `cursor-research.md` notes rivo TUI only, minimal unchanged. |
| **Sidebar** | Today `AgentStatusBar` + welcome menu — no agent list. | New `views/sidebar.rs` widget: vertical `List` of agents + side chats grouped under parent, agent state icons (dot spinner / check / X mapped from `GoalDisplayStatus`, see `views/agent_status.rs:160-210`), hover to pin (★), click to focus tile. Backed by `views/dashboard/state.rs` agent row models that already group status, unread count, elapsed, provenance. |

### 7.4 Minimal surface area to get tiled v1 without bricking today

1.  Leave `views/agent.rs:AgentViewLayout` **unchanged** — its constraint list stays vertical and is the unit of tiling (a tile's inner vertical layout).
2.  New file `src/tui/windows.rs` + `src/session/side.rs` per `cursor-research.md:231` scaffold (`src/mode.rs` / `src/tui/windows.rs` / `src/session/side.rs` / `src/commands/side.rs`). Start with `tui/` or `views/window_manager.rs` if you want to keep tests under crate's `views` tree.
3.  Guard behind `--tiled` or `config.toml [ui.tiled]` flag day so default stays single-column until manual QA passes. The dashboard already does feature-flagged dispatch (`is_tiled` or `is_minimal`) in this codebase (`app/agent_view/input.rs` + `minimal/api.rs` style).
4.  Document explicitly that new window manager is **local-only** (`cursor-research.md:169`) — leader/remote shell binding remains one backend per `SessionId`; tiling doesn't multiplex one shell VT.

---

## 8. File Map — Where Everything Is

### Crates layout (workspace root `C:\rivo`)

*   `Cargo.toml` workspace, `crates/codegen/` (the product), `crates/common/` (shared), `third_party/` (vendored `dagre`, `mermaid-to-svg`, etc.), `prod/mc/` (chat proxy types), `bin/protoc`, `docs/`.

### Exact paths referenced (with scope notes)

All paths below are relative to `C:\rivo` unless marked absolute.

**Shift+Tab + Cycle engine:**

*   `crates/codegen/xai-grok-pager/src/input/key.rs:309-324,360-380` — `shift_tab_keys`, `is_shift_tab`, `RowWalk`.
*   `crates/codegen/xai-grok-pager/src/actions/defaults.rs:519-533` — `CycleMode` ActionDef (Shift+Tab) + `982-993` `DashboardCycleMode` + `734-736` `ToggleYolo` `Ctrl+O`.
*   `crates/codegen/xai-grok-pager/src/app/actions.rs:437-452,697-830,1000-1150` — `PlanModeKind`, `PermissionModeKind`, `CycleMode`/`ToggleYolo` action enums + canonical serializers.
*   `crates/codegen/xai-grok-pager/src/app/dispatch/modes.rs:1-968` — **the ring** (`dispatch_cycle_mode`, `dispatch_cycle_mode_inner`, `collapse_to_ask_for_nudge_jump`, `set_yolo_mode_inner`, `set_permission_mode`, `downgrade_displayed_auto_if_gated`, `active_agent_plan_nudge_state`). Tests `src/app/dispatch/tests/modes.rs`.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/mod.rs:458,1278-1350` — `MODE_BANNER_FADE_TICKS`, `mode_switch_banner`, `plan_mode_active/pending`, `deferred_session_mode`, `side-chats` placeholder (no side chats yet), hit areas.
*   `crates/codegen/xai-grok-pager/src/app/acp_handler/session_notification.rs:1400-1460` — `detect_plan_mode_change` (the only `pending`→`None` clearance).
*   `crates/codegen/xai-grok-pager/src/app/app_view.rs:942-968,1500+,3726-3735` — `default_yolo`, `auto_mode_gate`, `yolo_policy_block`, welcome Shift+Tab forwarding `is_shift_tab → NewSession`.

**Plan Mode:**

*   `crates/codegen/xai-grok-pager/docs/user-guide/19-plan-mode.md:1-170` — user contract (state machine, approval view, lifecycle, compaction, appropriateness).
*   `crates/codegen/xai-grok-tools/src/implementations/grok_build/enter_plan_mode/mod.rs:1-220` — `EnterPlanModeTool` (seeding, `PlanFileSeedStatus`).
*   `crates/codegen/xai-grok-tools/src/implementations/grok_build/exit_plan_mode/mod.rs:1-440` — `ExitPlanModeTool` (notify `PlanModeExited`, return `PlanReady|EmptyPlan`).
*   `crates/codegen/xai-grok-tools/src/types/session_mode.rs:1-60` — wire enum `{Default,Plan,Ask}` (`strum` `snake_case`, fallback to `Default`).
*   `crates/codegen/xai-grok-pager/src/views/plan_approval_view.rs:1-170` — `PlanApprovalViewState`, `PlanComment`, `PlanApprovalFocus`, `EMPTY_PLAN_PLACEHOLDER`, `format_feedback`.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/plan.rs:1-710` — `should_show_plan_chip`, preview/approval/casual-comment surfaces, tests `plan_chip_tests`, `plan_approval_enter_tests`.
*   `crates/codegen/xai-grok-pager/src/slash/commands/plan.rs:1-60,200-380` — `/plan[desc]` → `SetPlanMode`/`EnterPlanMode`, tests.
*   `crates/codegen/xai-grok-pager/src/app/acp_handler/interactions.rs:126-170` — `handle_exit_plan_mode`.
*   `crates/codegen/xai-grok-pager/src/app/effects/helpers.rs:261-370` — `SessionFlags { plan_mode: bool, yolo_mode, auto_mode, agent_override }` + `agent_profile()` mapping (`grok-build-plan` / `grok-build-plan-no-subagents`) + `to_meta` emission (`_meta.yoloMode/autoMode/agentProfile/plan` etc.).

**Permissions / Yolo:**

*   `crates/codegen/xai-grok-pager/src/app/actions.rs:1032-1095` — `PermissionModeKind`.
*   `crates/codegen/xai-grok-pager/src/app/dispatch/modes.rs:198-560` — `yolo_enable_blocked`, `refuse_if_yolo_locked`, `effective_auto`, `set_yolo_mode_inner`, `set_yolo_mode`, `set_permission_mode`.
*   `crates/codegen/xai-grok-pager/src/app/acp_handler/permissions.rs:20-90` — `handle_permission_request` (Yolo AllowOnce fast path, enqueue).
*   `crates/codegen/xai-grok-pager/src/app/effects/helpers.rs:1394-1540` — `persist_permission_mode_and_notify`, `should_send_yolo_acp_notification`, `route_permission_mode_result`.
*   `crates/codegen/xai-grok-pager/src/app/effects/mod.rs:1476-1540,1990-2040` — `Effect::{SetSessionMode,SetModeThenPrompt,PersistPermissionMode}` arms + `TogglePlanMode`, `SendBtw`.
*   `crates/codegen/xai-grok-pager/src/app/agent.rs:726-735,860-880` — `yolo_mode`/`auto_mode` fields + `is_yolo()/is_auto()`.
*   `crates/codegen/xai-grok-pager/src/app/cli.rs:277-278,451-452` and `crates/codegen/xai-grok-pager/src/headless.rs:50,802-805` — `--always-approve/--yolo` flag wiring.
*   `crates/codegen/xai-grok-pager/src/slash/commands/always_approve.rs:1-100` — `/always-approve` toggle; `slash/commands/auto.rs` — `/auto` (auto classifier).
*   `crates/codegen/xai-grok-pager/src/settings/defs.rs:1470+` + `settings/registry.rs:290-460` — `permission_mode` choices gated by `auto_mode_gate`.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs:960,1480+` + `views/agent_status.rs` — plan chip, badge rendering.

**Tool allowlist:**

*   `crates/codegen/xai-grok-tools/src/types/tool.rs:70-118` — `ToolKind` + `ToolNamespace`.
*   `crates/codegen/xai-grok-tools/src/tool_taxonomy.rs:79-260` — `ToolKind::is_read_only()`.
*   `crates/codegen/xai-grok-tools/src/types/tool_metadata.rs:1-205` — `ToolMetadata { kind, tool_namespace, description_template, is_read_only(), requires_expr(), versioned_definition }` + `shared_resources/helpers`.
*   `crates/codegen/xai-grok-tools/src/registry/mod.rs:1-700+` — `ToolRegistryBuilder { register<T>, finalize(...) }` + `FinalizedToolset`, `ToolServerConfig`, `known_tool_kinds`.
*   `crates/codegen/xai-grok-tools/src/implementations/**` — per-tool dirs (see §4.1 list). In particular `grok_build/read_file`, `search_replace`, `bash`.

**Status bar:**

*   `crates/codegen/xai-grok-pager/src/views/status_bar.rs:1-93` — generic row widget.
*   `crates/codegen/xai-grok-pager/src/views/agent_status.rs:1-360` — `AgentStatusBar { push("plan"/"goal"/"mcp"/"bg_tasks"), render right-aligned with │ }`, `goal_status_line`, `mcp_status_line`.
*   `crates/codegen/xai-grok-pager/src/views/agent.rs:100-430` — `AgentViewLayout { status_bar, scrollback, todo, queue, btw, turn_status, banner, plugin_cta, follow_ups, ... }` + `Layout::vertical` constraints.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs:870-1530` — `AgentView::draw` + entry `draw_subagent_fullscreen` + `build_hints`.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/plan.rs:68-135` — `should_show_plan_chip`, `plan_body_for_preview`, chip-hover styling.

**Slash / Side chat:**

*   `crates/codegen/xai-grok-pager/src/slash/mod.rs:registry re-exports` + `slash/command.rs:160-370` — `SlashCommand` trait + `CommandResult` (`Handled`, `Action`, `QueueCommand`, `InjectSkill`, `PassThrough`) + `ModeSupport` + `ArgItem`.
*   `crates/codegen/xai-grok-pager/src/slash/registry.rs:113-610` — `CommandRegistry { commands, hidden, menu_hidden, restricted, available_tools, rebuild_triggers }` (gates for `voice`, `recap`, `auto`, `share`, etc.; `BLOCKED_ACP_NAMES` safety).
*   `crates/codegen/xai-grok-pager/src/slash/commands/mod.rs:75-155` — `builtin_commands()` order (39+).
*   `crates/codegen/xai-grok-pager/src/slash/commands/btw.rs:1-44` — `/btw <question>` transient panel command.
*   `crates/codegen/xai-grok-pager/src/views/btw_overlay.rs:1-1050` — `BtwOverlayState {Loading|Done|Error}` + `btw_panel_height`, `render_btw_panel` (markdown + selection model + overlay links), `DONE_MAX_BODY_LINES=12`.
*   `crates/codegen/xai-grok-pager/src/app/dispatch/notes.rs:349-560` — `dispatch_send_btw`, `handle_btw_response`.
*   `crates/codegen/xai-grok-pager/src/minimal/api.rs:54-200` — `minimal_btw_*` lifecycle helpers.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/input.rs:338-620` — `handle_minimal_btw_input`, mouse/scroll routing for `btw_state`.
*   `crates/codegen/xai-grok-pager/src/slash/matcher.rs`, `mode_support.rs`, `mru.rs` — slash completion + MRU.

**Layout / Panel:**

*   `crates/codegen/xai-grok-pager/src/views/agent.rs:100-430` — the only `Layout::vertical` layout factory.
*   `crates/codegen/xai-grok-pager/src/app/agent_view/mod.rs:1-1400` — `AgentView` struct (screen Mode banner, `plan_mode_pending`, hit areas, `side-chats` shame: not yet existing), `BannerSlotParams`, `HitArea`.
*   `crates/codegen/xai-grok-pager/src/views/*` — widget tree: `agent/`, `prompt_widget/`, `shortcuts_bar.rs`, `todo_pane.rs`, `progress_bar.rs`, `question_view.rs`, `btw_overlay.rs`, `permission_view.rs`, `plan_approval_view.rs`, `welcome/{hero_box,logo,menu,top_bar}` etc. Everything consumed by `AgentView::draw`.
*   `crates/codegen/xai-grok-pager/src/app/app_view.rs:5390+` — App-level draw dispatch (minimal vs fullscreen fork).

**Binary alias (already rivo-ready):**

*   `crates/codegen/xai-grok-pager-bin/Cargo.toml:17-21` + `build.rs` + `src/main.rs` — dual `[[bin]] name="xai-grok-pager"` and `[[bin]] name="rivo"` both pointing to `src/main.rs` (identical artifact, `cargo build -p xai-grok-pager-bin` emits `rivo.exe` + `xai-grok-pager.exe` on Windows). `views/announcements.rs:is_promo → false` fix already on `main` per context.

---

## 9. What to Do Next — Ordered Checklist for the Main Agent

After the background `cargo build` succeeds (so `cargo test -p xai-grok-pager` + `cargo test -p xai-grok-tools` are green on unmodified tree):

### Phase 0 — Seed the mode type (1 file, no UI break)

1.  Add `crates/codegen/xai-grok-tools/src/types/session_mode.rs: Ask = "ask"` (and if opting for shell-visible Debug, `Debug = "debug"`; `Multitask` can stay pager-only). Confirm `cargo test -p xai-grok-tools` still passes (compat fallback to `Default` keeps old pagers non-bricking).
2.  Add `crates/codegen/xai-grok-pager/src/app/agent_view/modes.rs` (new) or re-use `SessionMode` import alias — declare `pub enum AgentMode { Normal, Plan, Ask, Debug, Multitask }` with `fn as_session_mode(&self)->Option<SessionMode>` and `fn badge_label(&self)->Option<&'static str>` for status. Store `agent_mode: AgentMode` in `AgentView` (+ `mode_pending: Option<AgentMode>` if you keep pending discipline), `app/app_view.rs` mirror `default_agent_mode`.

### Phase 1 — Replace the ring (1 core file, gated)

3.  Fork `app/dispatch/modes.rs:631-968` cycle. Implement `AgentMode` ring behind a short-lived constant `const RIVO_MODE_ORDER: &[AgentMode] = &[Normal, Plan, Ask, Debug, Multitask]` and render `AgentMode::next(cur)-> (next, effects)`. For now keep `Auto`/`AlwaysApprove` out of ring (YOLO stays `Ctrl+O`). Verify `cargo test -p xai-grok-pager --lib app::dispatch::tests::modes` (update fixtures to 5-step ring; keep the nudge-jump collapse for `Ask` vs `Plan`? In rivo, the tip promises one Shift+Tab → Plan, Plan stays first).
4.  Update `actions/defaults.rs:519-533` long_help + description to enumerate the new cycle; `hint_key_display: Some("Shift+Tab")` unchanged.
5.  Add banners/tests: `Plan`→`"Plan"`, `Ask`→`"Ask"` blue, `Debug`→`"Debug"` amber, `Multitask`→`"Multitask"` spinner.

### Phase 2 — Status badge (1 widget file)

6.  Edit `app/agent_view/render.rs:1445-1518` gap before `push("plan")`. Implement `Rivo · <Mode> · YOLO` Line described in §5.2, retiring standalone `push("plan")` when mode badge is active. Wire hover `hit_rivo_mode` in `app/agent_view/mod.rs` + click opens `/mode` picker (later).
7.  Prompt chrome in same file (960-1040): blue accent + `Ask Rivo...` placeholder when `AgentMode::Ask`, green/amber when Debug, purple-ish when Multitask/`workflow`. Keep plan's border rule.

### Phase 3 — Ask gate (2 crates)

8.  `xai-grok-tools/src/registry/mod.rs` Ask filter branch before `renderer.render` loop; snapshot keep-set in §4. Keep `CheckAllowlist` test `types.rs:2523` green by exempting filter path from the "wire vs doom-loop must agree" check when Ask is active (they already diverge intentionally for the filter).
9.  `xai-grok-shell/src/session/**` shell-side Ask reject on non-`is_read_only` tools while `SessionMode::Ask` is active. Gate message matches prompt gate so telemetry doesn't show allowed vs denied drift confusing model.
10. `app/effects/helpers.rs:SessionFlags` `ask_mode: bool` stamp into `_meta` (or `x.ai/mode` key) so shell sees it.

### Phase 4 — SideChats durable (new modules, biggest surface)

11. Scaffold `src/app/side_chat.rs` + `src/slash/commands/side.rs` (`/side` alias `/btw`) + `src/views/side_chat_pane.rs` (start scoped clone of `btw_overlay`). Wire storage `~/.rivo/side-chats.json` (index) + `~/.rivo/sessions/side-*.jsonl` (per chat `hidden_context` + transcript).
12. Replace transient dispatch (`app/dispatch/notes.rs:349`) dual path: keep transient `BtwOverlay` for fast path (optional), but wire canonical `/side` to `Effect::CreateSideChat` → `x.ai/side_chat/create` or pager-local spawn (local-only, per Cursor parity). Respect Cursor's "no nesting" and archived-not-deleted semantics when implementing the sidebar list.

### Phase 5 — Tiling (new crate tree, optional behind flag)

13. `src/views/window_manager.rs` (+ `views/sidebar.rs`) implementing `WindowTile::compute` horizontal/vertical splits, divisor drag, `Ctrl+←/→` resize, `Ctrl+Tab` tile focus, `~/.rivo/windows.json` persistence. Gate behind `config.toml [ui.tiling]=false` initially; default single-column must stay green under `cargo test` and PTY `pty_e2e_*` runs.
14. Dashboard agent row reuse for sidebar (already in `views/dashboard/*`), per-agent sandbox terminal isolation (re-use each Window's `workspace/fsnotify` scope).

### Phase 6 — CLI/flags + docs

15. `app/cli.rs:277` add `--mode=ask|plan|debug|multitask` and wire into `headless.rs` seeds; document in `crates/codegen/.../docs/user-guide/20-rivo-modes.md` + `/docs` alias table.

### Tests to keep green at every step

*   `cargo test -p xai-grok-pager --lib input::key` (shift_tab encodings invariant).
*   `cargo test -p xai-grok-pager --lib app::dispatch::tests::modes` (ring order).
*   `cargo test -p xai-grok-pager --lib app::agent_view::plan` (chip idempotency, `close_plan_review` pending clear).
*   `cargo test -p xai-grok-pager --lib views::btw_overlay` plus any new `side_chat*` tests (long question truncates but `[Esc]` stays).
*   `cargo test -p xai-grok-tools --lib` including `capabilities_is_read_only_matches_metadata` (update to allow Ask filter path).
*   `cargo test -p xai-grok-pager --test pty_e2e_smoke` smoke PTY — Shift+Tab must still produce banner and ring advancement in PTY harness (see `tests/pty_auto_mode.rs` precedent for `Shift+Tab → Auto banner via PtyHarness`).

---

## 10. Risks, Traps, and Pre-emptive Fixes

*   **BackTab encodings drift** — do not re-implement Shift+Tab matching outside `input/key.rs:is_shift_tab`. Every consumer must call `shift_tab_keys()` (already pinned by `tips/plan_nudge.rs:36-39` and many `links.rs` echo tests).
*   **Pending/Active duality** — `plan_mode_pending: Option<bool>` is what the ring reads via `unwrap_or(plan_mode_active)`. Any new `AgentMode` needs the same steal (pending vs active) to prevent double-sends under rapid Shift+Tab. `detect_plan_mode_change` must clear the pending counterpart for every mode added — not only plan.
*   **Welcome-screen pre-session path** — don't delete the `session_id == None` branch's `deferred_session_mode` staging; that is how `/plan` on welcome screen becomes the new agent's plan mode (consumed in `SessionCreated` handlers). Carry the same for Ask/Debug.
*   **Minimal mode** — `Ctrl+O` there opens the transcript, not YOLO. Rivo must decide whether `--minimal + Ctrl+O` keeps transcript semantics or moves them elsewhere (e.g., `Ctrl+Shift+T`). For v1, keep upstream behavior and only add the global drain on full-TUI `Ctrl+O`.
*   **Promo CTA stealing Ctrl+O** — pinned promos still light `Ctrl+O` override (see `agent_view/links.rs:1057`) — if you globally drain on every `Ctrl+O`, you'll dismiss customer's CTA before they saw it. Keep the `pinned_upgrade_cta_live` gate ahead of YOLO drain and document in the badge: when pinned CTA live, badge YOLO flip needs a second Ctrl+O.
*   **Ask vs YOLO layering** — don't short-circuit `ask_mode` with YOLO `AllowOnce` fast path. `app/acp_handler/permissions.rs:48-65` checks `is_yolo` *before* `ask` — patch to `if is_ask { // block even under yolo; fall through to edit-gate reject } else if is_yolo { fast-approve }`.
*   **Plan edit gate Scope** — plan's file check allows only `plan.md` (full path prefix, not substring). Ask must not inadvertently enlist `plan.md` semantics — its gate rejects **all** edits (no plan-file escape hatch).
*   **Build-time tools harness cycle** — `xai-grok-pager-bin` owns `rivo` binary; do not add `xai-grok-pager-minimal` dep back to `xai-grok-pager` (see `Cargo.toml:24` comment about cycle with `xai-grok-pager-minimal` needing fn-pointer seam). New `rivo` features must go in `xai-grok-pager` lib, wired into the bin via the existing `main.rs` hook.

---

## 11. Queried Areas — Direct Answers

1.  **Shift+Tab and keybindings.** Single source: `input/key.rs:309-324` + `actions/defaults.rs:519-533`. Currently toggles `Normal → Plan → Always-Approve → Normal` (4-state with `Auto` as gated extra). Bound as `When::PromptFocused` so it bubbles only from the prompt. Dashboard mirror uses same keys. Immediately writes `plan_mode_pending` optimistically, then ACP echo `CurrentModeUpdate` clears it. See full ring table in §1.3, routing diagram in §1.4, banner TTL in §1.5.
2.  **Plan Mode current implementation.** Bundled doc `crates/.../19-plan-mode.md:1-170` is authoritative. Agent tool path `enter_plan_mode` (seed empty plan, notify `PlanModeEntered`) → `exit_plan_mode` (read plan file, notify `PlanModeExited`) → pager parks `PlanApprovalViewState` modal (534 lines, §2.3). Edit gate blocks non-plan writes even under YOLO. Lifecycle 4-state machine + `deferred_session_mode` replay. Surfaces: chip, preview viewer, inline comment overlay, persisted plan file in `sessions/<encCwd>/<sid>/plan.md`. No rewrite needed — reuse as-is in the new ring.
3.  **Permission/Yolo/Always-approve.** `Ctrl+O = key!('o', CONTROL)` bound at `actions/defaults.rs:734` & dashboard mirror; `app/dispatch/modes.rs:set_yolo_mode_inner` drains `permission_queue` preferring `AllowOnce` (Never synthesizes `AllowAlways` for YOLO). ACP broadcast `x.ai/yolo_mode_changed { yolo_mode, auto_mode, permission_mode }` persisted to `[ui].permission_mode` via `persist_permission_mode_and_notify` (helpers.rs:1394). `PermissionModeKind` four-way with `effective_auto(!yolo && auto)`. Policy pin `yolo_policy_block` disallows enabling anywhere. Can be made global by draining **all** live agents' queues + fanning out ACP notifications (three sites to patch §3.4). Badge insertion site `render.rs:1478` before plan chip.
4.  **Tool allowlist.** Tools categorized by `types/tool.rs:70-105 ToolKind` (+ `tool_taxonomy.rs:79 is_read_only()` gate). Registry `registry/mod.rs:676-760` builds `FinalizedToolset` per `SessionContext`. `AgentMode::Ask` in Cursor is `readFile/readDir/grep/glob/fetchRules/askQuestions` only, blocking `editFile/writeFile/bash` (`cursor-research.md:31`). Grok mapping in §4.1-4.2; allowlist is kept set sketched there. Recommended seam: client-side filter at `finalize` (drop mutating tools from `tool_definitions` visible to model) + shell-side fast-reject for jailbreak. Add `SessionMode::Ask` wire id (currently fallback to `Default`) and plumb `ask_mode: bool` through `SessionFlags` → `SharedResources`.
5.  **Status bar.** `views/agent_status.rs:40-135 AgentStatusBar::render` (right-aligned with ` │ `), `views/agent.rs:102` `AgentViewLayout.status_bar: Rect` as first `Length(1)` constraint, `app/agent_view/render.rs:1445-1530` push pipeline. Today chips: `plan`, `goal`, `mcp`, `bg_tasks`, workspace badge. Insertion point for `Rivo · Ask · YOLO` is a new `push("rivo_mode", ...)` at §5.2 before plan chip, collapsing plan chip when badge active. Prompt chrome (`Placeholder`, accent, border blend) lives same file lines 960-1040 and already branches on `effective_plan`.
6.  **Side chat architecture.** Only `/btw` exists — `slash/commands/btw.rs` (`/btw <question>` → `Action::SendBtw`), transient `BtwOverlayState` (`views/btw_overlay.rs`), ACP `x.ai/btw` queue-bypassing ext method (`dispatch/notes.rs:349`), layout slot `views/agent.rs:112-320` vertical stack `btw_height` above prompt. Not durable, not archived, not @-mentionable. Cursor side chats are durable, parent-scoped, carry hidden parent context, archive on `X`, block nesting, re-injected via `@side-chat:` mention (`cursor-research.md:144-175`). New modules `session/side.rs` + `slash/commands/side.rs` + `views/side_chat_pane.rs` + persistence `~/.rivo/side-chats.json` described in §6.3-6.4.
7.  **Window/panel architecture.** `views/agent.rs:138-410 AgentViewLayout::compute` is **single-column `Layout::vertical` only**; panes (`todo`, `queue`, `tasks`, `catalog`) above scrollback, `btw/queue/turn_status/banner/cta/follow_ups` below, scrollback is the sole `Constraint::Min(5)` flex row. `ActivePane` has 6 vertical panes, no `Layout::horizontal`. No tiling, no divisor, no sidebar. Minimal is same stack in live-region `xai-ratatui-inline`. New tiling manager (`views/window_manager.rs` + sidebar) uses `Layout::horizontal/vertical` + `Constraint::Ratio` per §7.3, handing each `AgentView` its tile `Rect` unchanged.

---

## Appendix — Glossary

*   **YOLO** = Always-Approve (`permission_mode: "always-approve"`, `yolo_mode: true`). Chord `Ctrl+O`, slash `/always-approve` (alias `/yolo`), CLI `--always-approve --yolo`.
*   **Auto** = LLM classifier (`permission_mode: "auto"`, `auto_mode: true`). Only via `Auto` setting, cycle, or `/auto` (gated by `auto_mode_gate`). Not YOLO.
*   **Soft-default** = launch unstamped default ("follow server default") vs user-stamped `ask`/`auto`/`always-approve`; the cycle and typed setters clear `permission_mode_from_soft_default` (modes.rs:602-607).
*   **AC** = approval checkpoint (plan's approval view); `PlanApprovalFocus::{Preview,Prompt,Commenting}`.
*   **Cwd encoding** = `urlencoding::encode(Cwd.to_string_lossy())` used under `~/.grok/sessions/<encoded_cwd>/<session_id>/` for plan files and session IO (`app/agent_view/plan.rs:31-40`).

---

*Prepared by the modes subagent — no Rust source edited. Hand off this document to the main agent (or load it as `C:\rivo\docs\rivo-modes-implementation.md` in the next prompt) and implement in the phase order of §9. The build already includes the `rivo` binary alias (`crates/codegen/xai-grok-pager-bin/Cargo.toml:17-21`) and the banner fix (`is_promo→false` in `views/announcements.rs`) on branch `main`.*
