# cchordes Product Overview

A tool for managing Claude Code agents in one place.
It runs multiple sessions side by side on a single screen and keeps past conversation history where you can find it.

The name comes from "hordes of Claude Code".

## Problems to solve

Running Claude Code in parallel means one terminal per session.

- It gets hard to tell which terminal is doing which task
- You can't tell which sessions are waiting for input and which are working without checking each one
- Switching between terminals is tedious in itself
- Past conversations are scattered across projects and terminals, so they are hard to find later

## What we are building

A management tool that covers both the "now" and the "so far" of your agents.

- **Now**: running sessions are laid out on one screen, and the state of each is visible
- **So far**: conversation history from past sessions can be listed, viewed and resumed in one place

It is a TUI that runs inside an existing terminal (the same position as tmux or zellij).
cchordes starts and owns the sessions directly, with no dependency on tmux. It is keyboard-driven.

## Scope

### Works today

- Run multiple sessions (`claude` by default) in split panes; key input, paste and resize pass through
- Session list with status (working / waiting / done), detected from the screen
- Input routing: type free-form text and it is sent to the best-matching session (rule-based, no API); a person confirms the target each time
- Scrollback for each pane (`Ctrl-b [`), and mouse support: click to focus, wheel to scroll
- Rename a session (`Ctrl-b ,`); a dot and the terminal bell mark an unfocused session that finished
- Confirmation before closing or quitting while a session is running
- Unified, searchable history of past conversations (`~/.claude/projects`), viewable in place
- Resume a past conversation from history in a new pane (`claude --resume <id>` in its original directory)

All commands go through the `Ctrl-b` prefix: `c` new, `x` close, `h`/`l` focus, `v` toggle layout, `s` sessions, `i` route input to a session, `H` history, `[` scroll back, `,` rename, `1`-`9` jump, `q` quit.

### Candidates (undecided)

- Give each session its own git worktree so their work doesn't collide
- Notes on sessions, and keeping names across runs (renaming itself works today)
- Config file (launch command, key bindings, etc.)
- **Manager agent**: one dedicated agent that talks with each session. The person talks to the manager; the manager sends instructions to the right session and reads back its state and results. Hub-and-spoke: sessions never talk to each other, and no swarm-style task breakdown. Rule-based input routing covers the simple case for now

## Out of scope

- **Kanban or task management**: managing tasks is a job for another tool
- **Agent-to-agent coordination**: no swarm-style orchestration where agents split up and assign work among themselves, and no direct session-to-session communication. Communication goes only between the manager (a candidate, see above) and each session; a person stays in charge of what gets done
- **Replacing a general-purpose terminal**: the focus is on running and managing agents side by side
- **Background execution**: sessions do not keep running after the terminal is closed (no tmux-style detach / attach). Closing stops the work; what remains is the conversation history

## Open questions

- Whether to support agents other than Claude Code (Codex, etc.)
- Whether to become a standalone GUI window (for now, a TUI inside an existing terminal)
- Where the session list and the history list appear (an always-visible sidebar, a screen shown on demand, etc.), and whether the two should be merged into one list
- Whether to publish on crates.io
