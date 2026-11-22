# CLAUDE.md

cchordes is a Rust TUI that runs several Claude Code sessions side by side
(split panes, session status, input routing, unified history). It owns the
sessions on PTYs directly, with no tmux. Read `README.md` for the keys and
`docs/product.md` for scope.

## Layout

| File | Role |
| --- | --- |
| `src/main.rs` | `App`, input modes (`Mode`), key handling, drawing with ratatui, event loop |
| `src/session.rs` | A child process on a PTY plus its vt100 emulator state, status detection, scrollback |
| `src/router.rs` | Rule-based routing of free-form input to a session (`@name` or word overlap, no API calls) |
| `src/history.rs` | Reads past conversations from `~/.claude/projects` (`.jsonl`) |

All commands go through the `Ctrl-b` prefix. Add a new command in
`App::handle_key` (and a `Mode` variant if it needs its own screen), then list
the key in `README.md` and the prefix hint in `draw`.

## Build and test

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Run all four before pushing. `Session` tests spawn `sh` on a real PTY, so they
need a Unix environment. The TUI itself has no automated test yet; say so in the
PR's QA section when a change was not tried by hand.

## Scope: do not add

These are out of scope in `docs/product.md`:

- Kanban or task management.
- Agent-to-agent coordination, or any direct session-to-session communication.
  The person decides what gets done. A manager agent talking to each session
  is only a candidate, not decided.
- Replacing a general-purpose terminal.
- Background execution (no detach and attach). Closing cchordes ends the
  sessions.

If a change touches a candidate or an open question in `docs/product.md`,
update that file in the same PR.

## Gotchas

- Status is detected from screen text (`esc to interrupt` means working). It is
  cached in `Session::pump` from the live screen only, because `contents()`
  returns the scrolled-back view while the user is scrolling.
- vt100 scrollback is 10,000 lines per session. The parser's scroll offset
  affects `screen().cell()` and `contents()`.
- Keep the router free of network calls.

## Workflow

- One independent concern per PR. Branch name is `<type>/<short-slug>`
  (`feat`, `fix`, `docs`, `refactor`, `chore`, `test`, `ci`, ...).
- Open PRs with the `create-pr` skill and issues with the `create-issue`
  skill (`.claude/skills/`). Titles and bodies are in English.
- Link the issue from the PR body with `Closes #N`.
- Commit messages are short and in the imperative ("Add scrollback to session
  panes").
- A person merges. Do not merge or change repository settings unasked.
