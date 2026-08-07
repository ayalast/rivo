# Research: Cursor — Modos, Side Chat y Ventanas (fidelidad 1:1 para `rivo`)

> Investigación profunda 2026-08-07 (Exa + Brave + fetch de docs oficiales).  
> Referencia última: **Cursor Agents GUI**. Todas las citas vienen de fuentes oficiales citadas.

---

## 0. Mapa de modos (fuente de la verdad)

**Fuente primaria:** `https://cursor.com/help/ai-features/agent` (tabla oficial)

| Mode | Best for | Can edit files? |
|------|----------|-----------------|
| **Agent** (= Normal en rivo) | Building features, refactoring, fixing bugs | **Yes** |
| **Ask** | Understanding code, exploring architecture | **No (read-only)** |
| **Plan** | Complex features where you want to review approach first | **Yes (after you approve plan)** |
| **Debug** | Tricky bugs that need runtime evidence | **Yes** |

> “Use Agent for most tasks. Switch to Ask when you want answers without changes. Use Plan for multi-file features where you want to review approach. Use Debug for bugs that are hard to reproduce or understand.”

- `https://cursor.com/docs/agent/overview`, `https://cursor.com/docs/agent/plan-mode` (2025-10-07), `https://cursor.com/docs/agent/debug-mode` (2025-12-10, Cursor 2.2), `https://cursor.com/help/ai-features/side-chats` (2026-07-10, 3.11), `https://cursor.com/docs/subagents`, `https://cursor.com/docs/agent/agents-window`, `https://cursor.com/changelog/3-1` (tiled layout).

**Nota de evolución:** “Composer” ya no es un modo sino un *modelo* (Composer 2.5). No se replica.

---

## 1. Ask Mode — read-only

> “Ask mode is a read-only mode for understanding your codebase. Agent answers questions and explores code without making any edits.” — `cursor.com/help/ai-features/ask-mode.md`

- **Tools:** solo lectura — `readFile`, `readDir`, `grep/glob`, `fetchRules`, `askQuestions`. **Sin** `editFile`/`writeFile`/`applyPatch` ni ejecución de shell (vía sandbox; hoy aún hay limitación conocida donde `git add/push` puede pasar si sandbox inactivo — staff thread 166758 del 2026-07-24).
- **Invocación:** `Shift+Tab` o picker, `/ask`, `--mode=ask`.
- **Reglas:** `AGENTS.md`/project/user/team rules siguen aplicando en todos los modos.
- **Principio de rivo:** *toolset gatting*, no cambio de modelo. Todos los modos corren sobre el mismo modelo; solo cambia qué tools ve. (`www.learncursor.dev` — “Switching mode swaps the toolset the agent can reach for; the underlying model stays put.”)

**Para replicar en rivo:** `AgentMode::Ask` → allowlist `read_file`, `list_dir`, `grep`, `web_search/fetch`, `memory_search`, `ask_user_question`; bloquea `search_replace`/`write`/`run_terminal_command` (salvo lectura `git status/diff` si se desea paridad futura). Placeholder `Ask Rivo...`, borde azul.Respeta Yolo: Ask sigue sin editar aunque Yolo ON.

---

## 2. Plan Mode — plan antes de tocar código

> “Plan Mode creates detailed implementation plans before writing any code. Agent researches your codebase, asks clarifying questions, and generates a reviewable plan you can edit before building.” — `cursor.com/docs/agent/plan-mode`

**Flujo oficial (5 pasos):**
1. Agent pregunta para aclarar requisitos.
2. Investiga codebase (solo lectura).
3. Crea plan Markdown con paths + refs.
4. Usuario revisa/edita el plan (chat o editor Markdown, add/remove to-dos).
5. Click **Build** para ejecutar.

- Planes por defecto en `~` (no ensucian diff); “Save to workspace” → `.cursor/plans/*.md`.
- Hasta **Build**, todo `editFile` fuera de `*.md` devuelve “You must exit plan mode…”.
- Auto-sugerido en tareas complejas; para cambios triviales se salta.

**Para rivo:** **reutilizar Grok Build Plan Mode existente** (`crates/.../19-plan-mode.md`) tal cual — ya es read-only + genera `~/.agent/plans/*.md` + requiere aprobación. Solo añadirlo al ciclo `Shift+Tab` + flag `--plan`/`--mode=plan`. No reescribir.

---

## 3. Debug Mode — hipótesis + instrumentación + repro loop

> “Instead of immediately writing code, the agent generates hypotheses, adds log statements, and uses runtime information to pinpoint the exact issue before making a targeted fix.” — `cursor.com/docs/agent/debug-mode`

**Lanzamiento:** 2025-12-10 con Cursor 2.2 (`cursor.com/blog/debug-mode`, Albert Slepak).

**Loop exacto (docs + blog):**
1. **Explore & hypothesize** — genera **4–5 hipótesis** (docs dicen “multiple”; Learn Cursor precisa 4–5).
2. **Add instrumentation** — inserta `POST JSON` a **local debug server** (extensión Cursor) o a `.cursor/debug.log` (JSON con `id`, `timestamp`, `location`, `message`, `data`, `sessionId`, `hypothesisId`, …).
3. **Reproduce the bug** — Debug pide reproducir con pasos concretos (“keeps you in the loop”).
4. **Analyze logs** — revisa variable states / execution paths / timing.
5. **Make targeted fix** — “often just a two or three line modification”.
6. **Verify & clean up** — pide reproducir de nuevo con el fix; si **fixed**, borra toda la instrumentación; si no, **re-instrumenta y repite**.

**Opciones al final de cada turno (para rivo):** `(A) Revisar logs que generé` / `(B) Ya está solucionado` — rivo expone exactamente esos dos botones (Cursor blog los frasea como *fixed vs not-fixed*).

**Para rivo (`AgentMode::Debug`):** state machine `hypothesize → instrument ([rivo-debug] marks) → askRepro → analyze → fix → askReproAgain → cleanup|reloop`. Helper `insert_debug_logs` wrap de `search_replace` con marca para borrado limpio.

---

## 4. Multitask — el orquestador paralelo

Dos piezas combinadas:

### 4.1 `/multitask` (el patrón “main como orquestador”)

> “With `/multitask`, Cursor will run async subagents to parallelize your requests instead of adding them to the queue. It will also break down larger tasks into smaller chunks for a fleet of async subagents to tackle simultaneously.” — `cursor.com/changelog/04-24-26`

> “Type `/multitask` to have Cursor run async subagents in parallel instead of queuing your requests. From a plan, click Build in Parallel and Cursor runs independent steps at once while keeping dependent steps in order.” — `cursor.com/help/ai-features/multi-agent`

- Main descompone plan en **DAG** (independientes en paralelo, dependientes en orden).
- Paralelos aislados en **worktrees/branches** (`04-24-26`).
- Cloud handoff: local puede delegar a cloud VM+branch.

### 4.2 Subagents (implementación)

> “Subagents are specialized AI assistants that Cursor's agent can delegate tasks to. Each subagent operates in its own context window … Use subagents to break down complex tasks, do work in parallel, and preserve context in the main conversation.”

- Built-ins: **Explore** (búsqueda rápida), **Bash**, **Browser** (MCP).
- Paralelismo: “Agent sends multiple Task tool calls in a single message, so subagents run simultaneously.” `is_background: true/false`.
- Custom: `.cursor/agents/*.md` (también `.claude/agents/`, `.codex/agents/`), nesting hasta depth 1 desde 2.5.

**Para rivo (`AgentMode::Multitask`):** Main **nunca edita directo**; cada `todo_write` → `spawn_subagent` con prompt aislado; nuevas tareas mid-flight lanzan más subagentes; agregación en main. Indicador `Multitask · N subagents running`.

---

## 5. Yolo / Auto-run / Run Everything — aprobación global

**Timeline:** YOLO checkbox → Auto-Run + allowlist → **Run Mode dropdown** (Cursor 3.6, 2026-05-29) en `Settings → Cursor Settings → Agents → Approvals & Execution`: `Ask`, `Auto-review` (default nuevo), `Allowlist`, `Allowlist (with Sandbox)`, `Run Everything` (= ex-YOLO).

| Run Mode | Comportamiento |
|----------|----------------|
| **Ask** | Todo espera aprobación |
| **Auto-review** (default) | 1) Allowlist → 2) Sandbox → 3) Classifier LLM subagent (bloquea ~4%, no determinista, “not a security boundary”) |
| **Allowlist (+Sandbox)** | Solo allowlist auto; resto pide |
| **Run Everything** | **Sin gate — cero prompts** (“what Cursor used to call YOLO mode — and the prompts disappear”) |

- `permissions.json` (`~/.cursor/permissions.json` + `.cursor/permissions.json`) con `terminalAllowlist`, `mcpAllowlist`, `autoRun.allow_instructions/block_instructions`.
- **No existe `Ctrl+O` para Yolo en Cursor** (búsqueda exhaustiva sin hits). Es probable confusión con Claude Code. Curso usa `/run-everything on|off|status` (`/auto-run` alias).

**Para rivo:** **Yolo global ortogonal a AgentMode.** Se activa con `Ctrl+O` (toggle) y `--yolo` (CLI) — nombres familiares de Grok Build — y es verdaderamente global: suprime toda aprobación en main/side/subagentes/ventanas. Badge único `· YOLO`, sin otros carteles. Implementación en `src/permissions.rs` antes de cualquier `ask_user_question` de aprobación. Combinable: `Rivo · Debug · YOLO`.

---

## 6. Shift+Tab — orden del ciclo

> “Press Shift + Tab to cycle through modes” — `cursor.com/help/ai-features/agent`

**Orden real documentado:**

| Fuente | Orden |
|--------|-------|
| Forum staff `stop-changing-the-shift-tab-ordering` | `Plan, Agent, Debug, Ask` (ejemplo no normativo) |
| eastondev recheck 2026-06-08 | `Agent / Ask / Plan / Debug` |
| hktitan quick-ref | `Agent → Ask → Plan → Debug` |
| `cursor.com/docs/cli/using` | `Agent, Plan, Ask` (pre-Debug) |
| Bug report 159322 (lap real 4-way) | `Ask → Plan → Debug → Ask → Agent` ⇒ **4-way `Agent → Ask → Plan → Debug`** |

**Veredicto para rivo:**
- **Ciclo canónico:** `Agent (=Normal) → Ask → Plan → Debug → back to Agent`. **4 modos.** Documentación que lista `Agent, Plan, Ask` es lag pre-Debug.
- **Multitask NO va en Shift+Tab** — es `/multitask` / `Build in Parallel`. rivo lo expone como modo extra del ciclo para conveniencia (Normal→Plan→Ask→Debug→**Multitask**) pero documenta que Cursor no lo cicla.
- **Run Everything no va en el ciclo** — es Run Mode separado.

**En rivo:** `Shift+Tab` cicla `Normal → Plan → Ask → Debug → Multitask → Normal` (5, con Multitask extra explícito). Si se quiere fidelidad estricta 4, quitar Multitask del ciclo y dejarlo solo vía `/multitask`/`--multitask`.

---

## 7. Side Chat — el `/side` durable

**Lanzamiento:** 2026-07-10, Cursor 3.11 (`cursor.com/changelog/side-chat`, `cursor.com/help/ai-features/side-chats`).

> “Side chats are durable child conversations attached to a parent agent. They use the parent thread as reference context while keeping their own visible transcript.”

> “A side chat is a full agent conversation that runs next to your main chat. The parent's conversation history is copied in as reference context for the model. That history does not appear in the side-chat transcript.”

**Entradas (3+2):**
1. `/side` en chat (alias viejo `/btw`) — abre vacío, con texto lo envía directo
2. Selección text/diff → **Ask in Side Chat**
3. `Shift+Cmd+S` / `Shift+Ctrl+S`
4. Plus button (aún no roll-out general → `/side` es el fiable)

**Contexto:** parent history entra como **hidden reference context** (model lo ve, no se renderiza en side). Solo prompt+follow-ups del side aparecen en su transcript.

**Re-inyección:** `@-mention` del side en main: `@side-chat: retry-policy apply that ...` → Cursor trae contexto del side al main.

**Ciclo de vida:**
- Conversación completa durable, follow-ups quedan en side.
- `X` **archiva**, no borra. Scoped a **parent agent + workspace**; persiste al navegar/cerrar parent; queda hasta archivar.
- **No nesting:** un side no puede crear otro side.
- **No es fork:** fork copia todo; side solo siembra contexto oculto.

**¿Ejecuta tools?** **Sí — full agent**, pero por defecto “focus on reading, searching, and answering, so the main agent keeps working uninterrupted.” (`developers digest` 2026-07-11: “Each side chat is a full agent session — it can read files, run commands, and make edits — but it stays separate from your primary thread.”)

**Para rivo:** `SideChat { parentId, hiddenContext: parentHistory, transcript, durable: true, archived: bool }`, slash `/side`/`/btw`, selección pinned, `@side-chat:` resolver, archivado no borrado. Side puede `read_file`/`grep`/`run_terminal_command` pero por defecto se usa para preguntar sin interrumpir main. Local-only (como Cursor).

---

## 8. Agents GUI — ventanas/paneles múltiples, resizable, navegables

**Estructura Window Manager en Cursor:**

### 8.1 Dos ventanas top-level
- **Agents Window** (agent-first workspace) vs **Editor Window** (VS Code-style IDE). Pueden estar **abiertas a la vez** (bug 3.3.27 que cerraba editor revertido). `Cmd+Shift+P → Open Agents Window / Open Editor Window`.

### 8.2 Dentro de Agents Window — 3 regiones

- Central **prompt window**
- Top-left **agent pane** — múltiples workspaces/repos en una instancia, co-located indexing; sidebar lista **todos los agentes** (local, cloud, worktrees, SSH, mobile/web/Slack/Linear)
- Right-side **inspection panel:** File browser, **Sandbox terminal (per-agent, aislado)**, Review pane (diffs/PRs)

### 8.3 Tiled layout (3.1, pane splitting)

> “Split your current view into panes to run and manage several agents in parallel. The tiled layout makes it easier to multi-task and compare outputs across agents without jumping between tabs. Expand panes to focus on a conversation, drag agents into tiles, and use keybindings for quick navigation and organization. Your setup also persists across sessions.” — `cursor.com/changelog/3-1`

- Resize por **drag del divisor** + expandir pane para focus + keybindings.
- **Your setup also persists across sessions** (layout JSON).
- Resizability confirmada en forum `multiple-agent-windows` (2026-07-15) — side-by-side split panes.

### 8.4 Navegación / múltiples side chats

- Sidebar lista **todos**; pin chats frecuentes para que queden arriba.
- Cloud agents con demos/screenshots.
- `Cmd+K` (Agents Window) búsqueda global sobre miles de transcripts; `Cmd+F` en chat; side chats indexados.

### 8.5 Limitación actual Cursor

> “Right now, the Agents Window works as a single window. If you try to open a second one, it just switches focus to the one that's already open.” — forum staff 2026-07-15

Workaround dual-monitor: múltiples proyectos en un Agents Window + `File > New Window` (Editor) o `Open IDE`.

### 8.6 Para rivo (réplica en TUI ratatui)

- **Una sola ventana de proceso** (como Cursor Agents Window), con **sidebar list de agents/side-chats** + prompt central + **tiled pane manager**.
- Tiling con `ratatui::layout::Layout` (`Direction::Horizontal`/`Vertical`, `Constraint::Ratio`), divisores `│`/`┃` draggeables (MouseEvent::Drag) + `Ctrl+←/→` (2 cols) / `Ctrl+Shift+←/→` (10 cols) + `/window resize`.
- Sidebar pin, búsqueda `Cmd+K` futura.
- Per-agent terminal aislado (cada Window/SideChat su `cwd`/shell session).
- Persistencia: `~/.rivo/windows.json` (ratios + orden) + `~/.rivo/side-chats.json` + `~/.rivo/sessions/side-*.jsonl`; rehidratación al reiniciar.
- Cap visible 4 paneles (resto en lista, switch vía `Ctrl+Tab`/`/side switch`).

---

## 9. Fuentes clave (trazabilidad)

- `cursor.com/help/ai-features/agent` — tabla oficial de modos
- `cursor.com/help/ai-features/ask-mode.md`, `cursor.com/docs/agent/plan-mode`, `cursor.com/blog/plan-mode` (2025-10-07), `cursor.com/docs/agent/debug-mode`, `cursor.com/blog/debug-mode` (2025-12-10), `www.learncursor.dev/learn/cursor-agents/debug-mode` (loop 6 pasos)
- `cursor.com/changelog/04-24-26` (`/multitask`), `cursor.com/docs/subagents`, `cursor.com/changelog/3-1` (tiled), `cursor.com/docs/agent/agents-window`, `cursor.com/changelog/3-0` (Agents Window)
- `cursor.com/help/ai-features/side-chats` + `cursor.com/changelog/side-chat` (3.11, 2026-07-10)
- `cursor.com/changelog/auto-review` + `cursor.com/blog/agent-autonomy-auto-review` (Run Mode / Auto-review)
- Forum: `stop-changing-the-shift-tab-ordering`, `agent-mode-skipped-when-switching-with-hotkey/159322`, `multiple-agent-windows/165827`, `ask-mode-agent-unaware-of-shell-restriction/166758`
- Grok Build 1.0: `x.ai/build/changelog` v1.0.0 2026-08-07 (mode indicator sync fix, workflows, etc.)

> Todo lo anterior está listo para guiar la implementación de `src/mode.rs` (AgentMode + ApprovalMode), `src/tui/windows.rs`, `src/session/side.rs`, `src/commands/side.rs` y el ciclo `Shift+Tab` en rivo.
