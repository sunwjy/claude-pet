# Claude Code hook events — research for claude-pet

Ticket: sunwjy/claude-pet#3 (map: #1). Researched 2026-09-23 against the official docs, with Claude Code **v2.1.280** installed locally.

Primary sources (cited below by short tag):

- **[H]** Hooks reference — https://code.claude.com/docs/en/hooks
- **[G]** Hooks guide — https://code.claude.com/docs/en/hooks-guide
- **[S]** Settings — https://code.claude.com/docs/en/settings
- **[SL]** Status line — https://code.claude.com/docs/en/statusline

Section anchors are given as `[H#anchor]`. Markers: **Verified** means stated in the docs. **Inference** means my reading or a design suggestion that is not in the docs. **Observed (clawd)** means behaviour inferred from reading clawd-on-desk. No clawd code is reproduced here.

---

## TL;DR

1. **Delivery.** Use **HTTP hooks** (`type: "http"`) that POST to a pet server on `127.0.0.1`. The one exception is `SessionStart`, which only accepts `command`/`mcp_tool` hooks. Use a tiny `command` hook there, or skip it and create the session lazily from the first event. Every event's JSON arrives as the POST body [H#http-hook-fields].
2. **Permission.** A `PermissionRequest` hook **is blocking** and can return `allow`/`deny` (plus `updatedInput`, `updatedPermissions`, `message`, `interrupt`) [H#permissionrequest-decision-control]. The default timeout is 600 s [H#common-fields]. On timeout the output is discarded, so no decision is made [H#timeouts].
3. **Pet not running.** An HTTP connection failure is a **non-blocking error, and execution continues** [H#http-response-handling]. Claude Code then falls back to its normal permission prompt. That is safe, but it may print a `hook error` notice (see Open questions).
4. **Identity.** Every event has `session_id`. Inside a subagent, events also carry `agent_id` and `agent_type`. `SubagentStart` and `SubagentStop` bracket each subagent's lifetime [H#common-input-fields][H#subagentstart].
5. **Install.** Hooks live in `~/.claude/settings.json` → `hooks.<Event>[] = {matcher, hooks:[handler…]}`. Entries **merge across scopes** and are picked up live by the file watcher [H#hook-locations][H#disable-or-remove-hooks]. To install and uninstall safely, edit only entries you can recognise as your own (for example by the URL path or a marker in the command). Write the file atomically and keep a backup. Alternative: ship as a **plugin** with `hooks/hooks.json`, which never touches the user's settings file.

---

## 1. Event catalogue (Verified, [H#hook-lifecycle])

Cadence: per session (`SessionStart`, `SessionEnd`), per turn (`UserPromptSubmit`, `Stop`, `StopFailure`), and per tool call (`PreToolUse`, `PostToolUse`) [H#hook-lifecycle].

| Event | Fires when | Pet relevance |
|---|---|---|
| `SessionStart` | session begins/resumes (`source`: startup/resume/clear/compact/fork) | spawn/wake session. **command/mcp_tool only** |
| `Setup` | `--init-only` / `--init`/`--maintenance` in `-p` | ignore |
| `UserPromptSubmit` | user submits prompt | → *thinking* |
| `UserPromptExpansion` | slash command expands into a prompt | optional |
| `PreToolUse` | before each tool call (not `EndConversation`) | → *working* (tool name available) |
| `PermissionRequest` | a tool call needs a permission decision | → *needs permission*, **blocking decision** |
| `PermissionDenied` | auto mode denied a tool call | optional |
| `PostToolUse` | tool succeeded | → *working* |
| `PostToolUseFailure` | tool failed | → *error* flash |
| `PostToolBatch` | after a batch of parallel tool calls resolves | optional |
| `Notification` | notification (`permission_prompt`, `idle_prompt`, `elicitation_*`, `agent_*`, `quota_*`, `auth_success`) | *needs attention* / *idle* |
| `MessageDisplay` | assistant text streams (in batches) | optional *talking* signal. It is **blocking on display**, so avoid it or keep it very fast |
| `SubagentStart` / `SubagentStop` | a subagent spawns / finishes | subagent counter |
| `TaskCreated` / `TaskCompleted` | TaskCreate tasks | optional |
| `Stop` | main agent finished responding (not on user interrupt) | → *done* |
| `StopFailure` | turn ended by API error (`error`: rate_limit, overloaded, …) | → *error* |
| `TeammateIdle` | agent-team teammate about to idle | optional |
| `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `DirectoryAdded`, `FileChanged` | env/config changes | ignore |
| `WorktreeCreate` / `WorktreeRemove` | worktree lifecycle. **WorktreeCreate replaces git behaviour and must return a path** | **do not register** |
| `PreCompact` / `PostCompact` | compaction | → *sweeping* |
| `PreModelSwitch` / `PostModelSwitch` | model switch | optional |
| `Elicitation` / `ElicitationResult` | MCP server asks user for input | *needs attention* |
| `SessionEnd` | session terminates (`reason`: clear/resume/logout/prompt_input_exit/other) | → *sleep*/remove session |

Warning (Verified): `WorktreeCreate` "Replaces default git behavior". Any non-zero exit or a missing path fails worktree creation [H#hook-lifecycle][H#exit-code-2-behavior-per-event]. A passive listener must never register it.

### Hook-type support per event (Verified, [H#prompt-based-hooks])

- All five types (`command`, `http`, `mcp_tool`, `prompt`, `agent`): PreToolUse, PostToolUse, PostToolUseFailure, PostToolBatch, PermissionDenied, Stop, SubagentStop, TaskCreated, TaskCompleted, TeammateIdle, UserPromptSubmit, UserPromptExpansion.
- `PermissionRequest`: command, http, mcp_tool, prompt (no agent).
- command/http/mcp_tool only: Notification, SubagentStart, SessionEnd, StopFailure, PreCompact, PostCompact, Elicitation(+Result), MessageDisplay, ConfigChange, CwdChanged, DirectoryAdded, FileChanged, InstructionsLoaded, Pre/PostModelSwitch, Worktree*.
- **`SessionStart` and `Setup`: command and mcp_tool only. HTTP is not supported.**

---

## 2. Payloads (Verified)

### Common fields [H#common-input-fields]

| Field | Notes |
|---|---|
| `session_id` | current session id |
| `prompt_id` | UUID of the current user prompt. Absent before the first input. v2.1.196+ |
| `transcript_path` | JSONL transcript. **Written async and may lag**. Use `last_assistant_message` on Stop/SubagentStop instead |
| `cwd` | working dir at invocation |
| `scratchpad_dir` | v2.1.257+, optional |
| `permission_mode` | `default`/`plan`/`acceptEdits`/`auto`/`dontAsk`/`bypassPermissions`. Not on every event |
| `effort` | `{level}` on tool-context events |
| `hook_event_name` | event name |
| `agent_id` | **only when the hook fires inside a subagent** |
| `agent_type` | subagent type (e.g. `Explore`) or `--agent` name |

Only `SessionStart` may carry `model`. Model switches arrive via `PreModelSwitch`/`PostModelSwitch` (`from_model`, `to_model`) [H#common-input-fields].

### Event-specific fields

- **SessionStart**: `source`, optional `model`, `agent_type`, `session_title`. On resume/fork it also carries `seconds_since_last_response`, `context_tokens`, `prompt_cache_likely_expired`, `estimated_cache_write_usd` [H#sessionstart-input].
- **UserPromptSubmit**: `prompt` [H#userpromptsubmit-input].
- **PreToolUse**: `tool_name`, `tool_input`, `tool_use_id`. For MCP tools it also has `mcp_server {name, source}` (v2.1.274+). File-tool paths are absolute [H#pretooluse-input].
- **PermissionRequest**: `tool_name`, `tool_input`, optional `permission_suggestions[]`, `mcp_server` for MCP tools. **No `tool_use_id`** [H#permissionrequest-input].
- **PostToolUse**: adds `tool_response`. For the `Agent` tool it holds `status` (`completed`/`async_launched`), `agentId`, `totalDurationMs`, `totalToolUseCount`, … [H#agent].
- **PostToolBatch**: `tool_calls[]` [H#posttoolbatch-input].
- **Notification**: `message`, optional `title`, `notification_type` [H#notification-input].
- **SubagentStart**: `agent_id`, `agent_type` [H#subagentstart-input].
- **SubagentStop**: `stop_hook_active`, `agent_id`, `agent_type`, `agent_transcript_path` (…/subagents/agent-<id>.jsonl), `last_assistant_message`, `background_tasks`, `session_crons` [H#subagentstop-input].
- **Stop**: `stop_hook_active`, `last_assistant_message`, `background_tasks[]` (id, type, status, …), `session_crons[]` [H#stop-input].
- **StopFailure**: `error` (rate_limit, overloaded, authentication_failed, …, unknown), `error_details`, `last_assistant_message` (the error string) [H#stopfailure-input].
- **PreCompact**: `trigger` (manual/auto), `custom_instructions`. **PostCompact**: `trigger`, `compact_summary` [H#precompact-input].
- **SessionEnd**: `reason` [H#sessionend-input].
- **MessageDisplay**: `turn_id`, `message_id`, `index`, `final`, `delta` [H#messagedisplay-input].

---

## 3. Hook types and low-latency delivery to a long-running process

### Handler types (Verified, [H#hook-handler-fields])

`command` (stdin JSON → exit code + stdout), `http` (POST JSON → response body), `mcp_tool` (tool on an already-connected MCP server), `prompt` (single-turn LLM), `agent` (experimental). All matching hooks run **in parallel**. The same handler defined in several settings files runs once [H#hook-handler-fields].

**HTTP handler fields** [H#http-hook-fields]: `url` (required), `headers` (supports `$VAR` interpolation, only for names listed in `allowedEnvVars`), and `timeout`. The body is `Content-Type: application/json`.

**Command handler extras** [H#command-hook-fields]: `args` (exec form, no shell), `async`, `asyncRewake`, `shell`.

**`async: true`** is only available on command hooks. It runs in the background, and decision fields have no effect [H#configure-an-async-hook].

### Default timeouts (Verified, [H#common-fields])

- command/http/mcp_tool: **600 s**.
- UserPromptSubmit, Pre/PostModelSwitch: 30 s.
- MessageDisplay: 10 s.
- SessionEnd: shares a **1.5 s budget** (raisable via per-hook `timeout`, up to 60 s, or `CLAUDE_CODE_SESSIONEND_HOOKS_TIMEOUT_MS`) [H#sessionend].
- prompt: 30 s. agent: 60 s.

### HTTP response semantics (Verified, [H#http-response-handling])

- 2xx + empty body → success (like exit 0, no output).
- 2xx + JSON object → parsed as hook JSON output.
- 2xx + other body, **non-2xx, or connection failure** → **non-blocking error, execution continues**.
- Timeout → hook cancelled, output discarded.
- HTTP hooks **cannot block via status code**. To block or deny, return 2xx with decision JSON.

### Options for claude-pet

| Option | Latency | Pet not running | Notes |
|---|---|---|---|
| **A. `http` hook → `127.0.0.1:<port>`** | one local HTTP request. No process spawn | connection refused → non-blocking error, continues (Verified) | Not allowed on SessionStart/Setup. Fixed port in settings (see Open questions). Non-async, so Claude waits for the response; the server must reply fast (Inference) |
| **B. `command` hook (tiny Rust binary) → local socket/HTTP** | process spawn per event (ms) | the binary must exit 0 quickly when it cannot connect | Works on every event. Can be `async: true` for fire-and-forget state events. Can discover a dynamic port from a runtime file |
| C. Transcript tailing | file-watch latency; the transcript lags [H#common-input-fields] | n/a | no permission control. Only a fallback |
| D. statusLine command | debounced 300 ms, only on some triggers [SL#how-status-lines-work] | n/a | occupies the user's single `statusLine` slot. Not event-granular |

**Recommendation (Inference):**

- Use `http` for all state events except SessionStart, plus a blocking `http` hook for PermissionRequest.
- Register SessionStart as a short `command` hook, or skip it and create sessions lazily from any event's `session_id`.
- Keep the state-event handlers returning `204` immediately. Timeouts for state events can be small (e.g. `timeout: 2`), so a hung pet never stalls Claude.
- Only PermissionRequest keeps a long timeout.

---

## 4. Blocking decisions: PermissionRequest and PreToolUse

### PermissionRequest (Verified, [H#permissionrequest])

- Fires when Claude Code is about to ask for permission. It also fires in sessions that cannot prompt (background subagents in `-p`). There, **if no hook decides, the call is denied**.
- It fires **immediately**, unlike the `permission_prompt` Notification, which waits about 6 s of user inactivity [H#permissionrequest][H#notification].
- It does not fire for a sandboxed command's network request. Use the `permission_prompt` notification for that [H#permissionrequest].
- Output shape: `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny", "updatedInput"?, "updatedPermissions"?, "message"?, "interrupt"?}}}` [H#permissionrequest-decision-control].
- Deny and ask **rules** in settings are still evaluated. A hook's allow does not override a matching deny rule.
- **Exit code 2 is not honoured** for this event. Only the `decision` object grants or denies [H#exit-code-2-behavior-per-event].
- `updatedPermissions` entries (`addRules`, `setMode`, …) take a `destination` of `session`/`localSettings`/`projectSettings`/`userSettings`. A hook may echo one of the incoming `permission_suggestions`, which enables "Always allow" [H#permission-update-entries].
- **No decision** (empty 2xx body, connection failure, timeout) → the normal permission flow proceeds (Verified via [H#http-response-handling] and [H#timeouts]).

### PreToolUse (Verified, [H#pretooluse-decision-control])

- `hookSpecificOutput.permissionDecision`: `allow` | `deny` | `ask` | `defer`, plus `permissionDecisionReason`, `updatedInput`, `additionalContext`.
- Precedence across hooks: deny > defer > ask > allow.
- `defer` only works in `-p` mode.
- A timed-out command/http PreToolUse hook **does not block**. The call goes through the normal permission flow [H#timeouts].
- For the pet, PreToolUse should stay **observational** (return empty). Approvals belong in PermissionRequest, which fires only when a prompt would actually appear (Inference, based on the docs' distinction in [H#permissionrequest-input]).

### Unverified and important

The docs do not say whether the **terminal permission dialog is shown while a PermissionRequest hook is still pending**, or only after it returns without a decision. clawd-on-desk's source comments state that when its server parked a request without answering, "CC would then hang for 600s before timing out with nothing in the terminal", and that closing the HTTP connection makes Claude Code fall back to the terminal prompt (**Observed (clawd)**). Treat it as likely that the dialog waits for the hook. Design implications:

- If the user dismisses the pet bubble or the pet is in DND mode, **close the connection or return an empty 2xx right away**, so the terminal prompt appears. Never leave the request parked.
- Choose a PermissionRequest `timeout` shorter than 600 s (e.g. 60–120 s) so a forgotten bubble degrades to the terminal prompt.
- Answering in the terminal while the hook is pending: the result is unknown. Needs a spike.

---

## 5. Identifying sessions and subagents (Verified unless marked)

- **Session**: `session_id` is present on every event [H#common-input-fields]. `SessionStart.source` tells startup, resume, clear, compact and fork apart. `/clear` produces `SessionEnd(reason=clear)` and then a new `SessionStart(source=clear)` [H#sessionend][H#sessionstart]. Inference: `/clear` starts a new `session_id`, consistent with [SL] "Resets to $0 when `/clear` starts a new session".
- **Subagent**: `SubagentStart` / `SubagentStop` carry `agent_id` + `agent_type`. Tool events fired inside a subagent carry the same `agent_id`/`agent_type`, while `session_id` stays the parent's [H#hook-locations][H#common-input-fields]. So the key is `(session_id, agent_id?)`. A missing `agent_id` means the main thread.
- Caveats [H#subagentstop-input]:
  - `SubagentStop` also fires for Claude Code's **internal** agents (prompt suggestions, `/btw`). There, `agent_type` may be empty or the session's agent name. A counter keyed by `SubagentStart` ids should ignore stops for unknown ids (Inference).
  - `SubagentStart` fires again when a subagent is **resumed**, and for each new message an in-process agent-team teammate handles [H#subagentstart].
  - Background subagents are the default since v2.1.198. The `Agent` tool's PostToolUse returns `async_launched` immediately, so use SubagentStart/Stop rather than the Agent tool for lifetime tracking [H#agent].
  - `Stop.background_tasks[]` lists in-flight subagents and shells. Use it to tell "turn done" apart from "still waiting on background work" [H#stop-input].
- **Which terminal or window**: not provided by the hook payload. `cwd` and `transcript_path` identify the project. The process tree (a command hook's parent PID) is the only route to a terminal (Inference, and what clawd does).

---

## 6. settings.json registration and safe auto-install/uninstall

### Structure (Verified, [H#configuration])

```json
{
  "hooks": {
    "<EventName>": [
      { "matcher": "<optional filter>", "hooks": [ { "type": "http", "url": "...", "timeout": 5 } ] }
    ]
  }
}
```

- `matcher`: `"*"`, `""` or omitted matches all. A string of letters, digits and `|` or `,` is an exact list. Anything else is an unanchored JS regex [H#matcher-patterns]. Matchers apply to tool name, notification type, agent type, and similar.
- Locations: `~/.claude/settings.json` (user, all projects), `.claude/settings.json` (project, shared), `.claude/settings.local.json` (project, private), managed policy, plugin `hooks/hooks.json`, skill/agent frontmatter [H#hook-locations].
- Entries **merge** across levels rather than override [H#hook-locations]. Lists in settings merge too [S#lists-merge-instead-of-overriding].
- Settings edits are picked up by the file watcher without a restart ("normally") [H#disable-or-remove-hooks].
- `disableAllHooks: true` switches all hooks off. There is no per-hook disable [H#disable-or-remove-hooks].
- **Workspace trust**: in interactive sessions, hooks from *every* settings file, including the user file, are held back until the folder's trust dialog is accepted [H#workspace-trust].
- **HTTP allowlist**: if `allowedHttpHookUrls` is set at any level, HTTP hooks run only when their URL matches. `allowManagedHooksOnly` blocks user hooks entirely [H#hook-locations]. The pet should detect these and report "hooks blocked by policy" (Inference).
- Cloud sessions don't read `~/.claude/settings.json` [H#hook-locations].

### Safe install/uninstall (Inference, design)

1. Target **`~/.claude/settings.json`** (user scope), because the pet is machine-wide. Never touch project files.
2. **Ownership marker**: recognise our entries by an unambiguous signature. For HTTP hooks, use a URL with a pet-specific path (e.g. `http://127.0.0.1:<port>/claude-pet/v1/<event>`). For command hooks, match the absolute binary path or a `--claude-pet` arg. Settings JSON has no comment or metadata slot, so the marker must live in `url` or `command`/`args`.
3. **Install**: read, parse, and abort if the file does not parse (never overwrite an unparsable file). For each event, append a new matcher group `{hooks:[ourHandler]}` only if no existing handler matches our signature. Never modify or reorder other groups. Update our own entry in place when the URL or port changes.
4. **Uninstall**: remove only handlers matching our signature. Drop a matcher group only if its `hooks` array becomes empty *and* it held ours. Drop an event key only if its array becomes empty. Leave `hooks: {}` alone or remove it only if we created it (optional).
5. **Write**: preserve unknown keys and do a round-trip that keeps key order (e.g. `serde_json` with the `preserve_order` feature). Write to a temp file in the same directory, `fsync`, then `rename`. Keep a timestamped backup. Re-read the file right before writing to shrink the race with Claude Code or the user editing it.
6. Never register `WorktreeCreate`/`WorktreeRemove`, and never make a state hook that can block (no exit 2, no `decision`).
7. **Alternative**: package the hooks as a **Claude Code plugin** (`hooks/hooks.json`, [H#hook-locations]). Install and uninstall are then handled by `/plugin`, and the user's settings.json is never edited. Trade-off: it needs a marketplace/plugin install step (evaluate in a follow-up).

---

## 7. Status line (Verified, [SL])

- `statusLine` is a single command slot (not a hook list). It gets JSON on stdin: `session_id`, `session_name`, `model`, `workspace.*`, `cost.*`, `context_window.*` (used_percentage), `rate_limits.*`, `effort`, `transcript_path`, `version`, `agent.name`, `worktree.*`, … [SL#available-data].
- It updates on a new assistant message, `/compact`, permission-mode change, and so on, debounced by 300 ms. `refreshInterval` is optional [SL#how-status-lines-work].
- `subagentStatusLine` gets a `tasks[]` array per refresh tick (id, name, type, status, tokenCount, …) [SL#subagent-status-lines].
- Relevance: it is the only official source for **context-window %, cost, and rate-limit** data. Taking over the user's statusLine is intrusive (they may already have one). If needed, wrap or chain the user's existing command (clawd does this with consent). Not required for the core states.

---

## 8. How clawd-on-desk wires Claude Code (Observed, behaviour only)

Repo: https://github.com/rullerzhou-afk/clawd-on-desk (read 2026-09-23).

- **State events** are registered as `command` hooks in `~/.claude/settings.json`, with `async: true` and a short timeout of about 5 s. They run a Node script given the event name as an argument. The script reads stdin, derives a state, and POSTs it to the pet's local HTTP server.
- Events: SessionStart, SessionEnd, UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, Stop, SubagentStart, SubagentStop, Notification, Elicitation, plus version-gated PreCompact/PostCompact/StopFailure. It **stopped registering WorktreeCreate** after it broke `claude -w`.
- Rough state mapping:

  | Event | State |
  |---|---|
  | UserPromptSubmit | thinking |
  | Pre/PostToolUse | working |
  | SubagentStart | "juggling" |
  | Stop | attention/done |
  | Failure | error |
  | PreCompact | sweeping |
  | Notification/Elicitation | notification |
  | SessionEnd | sleeping |

- **Permission**: a separate `http` hook on `PermissionRequest` → `http://127.0.0.1:23333/permission` with a 600 s timeout. It shows an Allow/Deny bubble. For DND, disabled bubbles, or disabled subagent gates, it **drops the connection** so Claude Code falls back to its terminal prompt, and it never forges a deny.
- **Port**: default 23333, with fallback to 23334–23337. The actual port is written to `~/.clawd/runtime.json`, which the command hooks read.
- **Settings ownership**: it identifies its entries by a marker substring in the command (the hook script filename) or by its permission URL. It writes atomically with backups, and a settings watcher re-checks or repairs its hooks. An optional statusLine integration is behind a consent prompt.

---

## Open questions (candidates for new tickets)

1. **Is the terminal permission prompt suppressed while a PermissionRequest hook is pending?** What happens if the user answers in the terminal first (is the HTTP request cancelled)? Not documented. Spike with a local server.
2. **Noise when the pet is not running**: does a refused HTTP connection print a `hook error` notice in the transcript on every event? If yes, use command hooks with a silent `exit 0` for state events, or an install/uninstall lifecycle tied to the pet running.
3. **Fixed port vs dynamic port**: HTTP hook URLs are static in settings. Choose a fixed port and a conflict strategy, or command hooks plus a runtime file. Also decide on auth (shared-secret header via `allowedEnvVars`) against other local processes.
4. **Plugin distribution vs direct settings.json editing** for install/uninstall.
5. **Terminal/window focus** ("jump to the session"): needs the process-tree approach, which is outside hooks.
