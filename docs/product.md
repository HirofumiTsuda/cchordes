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
- **So far**: conversation history from past sessions can be listed and viewed in one place

It is a TUI that runs inside an existing terminal (the same position as tmux or zellij).
cchordes starts and owns the sessions directly, with no dependency on tmux. It is keyboard-driven.

## Scope

### Works today

- Run multiple sessions (`claude` by default) in split panes; key input, paste and resize pass through
- Session list with status (working / waiting / done), detected from the screen
- Unified, searchable history of past conversations (`~/.claude/projects`), viewable in place

All commands go through the `Ctrl-b` prefix: `c` new, `x` close, `h`/`l` focus, `v` toggle layout, `s` sessions, `H` history, `1`-`9` jump, `q` quit.

### Candidates (undecided)

- Resume a past session where it left off
- Give each session its own git worktree so their work doesn't collide
- Attach names or notes to sessions
- Config file (launch command, key bindings, etc.)

## Out of scope

- **Kanban or task management**: managing tasks is a job for another tool
- **Agent-to-agent coordination**: no swarm-style orchestration that breaks a task down and assigns it to multiple agents. A person operates each session directly
- **Replacing a general-purpose terminal**: the focus is on running and managing agents side by side
- **Background execution**: sessions do not keep running after the terminal is closed (no tmux-style detach / attach). Closing stops the work; what remains is the conversation history

## Open questions

- Whether to support agents other than Claude Code (Codex, etc.)
- Whether to become a standalone GUI window (for now, a TUI inside an existing terminal)
- Where the session list and the history list appear (an always-visible sidebar, a screen shown on demand, etc.), and whether the two should be merged into one list
- Whether to publish on crates.io
