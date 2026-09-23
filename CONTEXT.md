# claude-pet

A desktop pet that floats above other windows and reacts in real time to what an AI coding agent is doing. A Rust rewrite inspired by clawd-on-desk, keeping only the floating-pet core.

## Language

**Pet**:
The animated character that lives on the desktop in its own transparent, always-on-top window.
_Avoid_: Clawd, mascot, widget

**Agent**:
An external AI coding tool (Claude Code first) whose activity the Pet reflects.
_Avoid_: Client, integration, bot

**Agent Event**:
A single lifecycle notification from an Agent (e.g. prompt submitted, tool started, task finished) delivered to the Pet.
_Avoid_: Hook, message, signal

**Pet State**:
The named behaviour the Pet is currently showing (idle, sleeping, dragged, thinking, working, delegating, blocked, done, error), derived from Agent Events and user interaction. It lasts as long as its condition holds.
- **done**: the Agent finished its turn and waits for the next prompt.
- **blocked**: the Agent is mid-turn and waits for the user's permission or input.
- **delegating**: the Agent has subagents running.
_Avoid_: Animation, mode, status

**Reaction**:
A short, one-shot behaviour (e.g. to a double-click or rapid clicking) played on top of the current Pet State; when it ends, the Pet returns to whatever Pet State is current at that moment.
_Avoid_: Pet State, interaction, emote

**Built-in Integration**:
An Agent whose Agent Events the Pet knows how to receive without the user writing glue; Claude Code is the only one.
_Avoid_: Plugin, adapter

**State Endpoint**:
The generic local interface any other Agent can send Agent Events to, so it can drive the Pet without a Built-in Integration.
_Avoid_: API, webhook, /state
