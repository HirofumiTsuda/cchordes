# cchordes

[![CI](https://github.com/HirofumiTsuda/cchordes/actions/workflows/ci.yml/badge.svg)](https://github.com/HirofumiTsuda/cchordes/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/HirofumiTsuda/cchordes)](LICENSE)
![Rust edition 2024](https://img.shields.io/badge/rust-edition%202024-orange)

Manage hordes of Claude Code sessions in one terminal.

cchordes is a TUI that runs multiple Claude Code sessions side by side, shows
which ones are working or waiting for you, and lets you browse and search past
conversations. It owns the sessions directly (no tmux needed).

## Features

- **Split panes**: several sessions on one screen
- **Session list** with live status: working / waiting / done
- **Unified history**: search and read past conversations from `~/.claude/projects`

## Install & run

```sh
cargo install --git https://github.com/HirofumiTsuda/cchordes
cchordes            # runs `claude`
cchordes <cmd> ...  # runs another command instead
```

## Keys

All commands start with the prefix `Ctrl-b`.

| Key | Action |
| --- | --- |
| `c` | new session |
| `x` | close focused session |
| `h` / `l` | move focus |
| `1`-`9` | jump to pane |
| `v` | toggle side-by-side / stacked |
| `s` | session list |
| `i` | send input to the right session (see below) |
| `H` | history (type to search, Enter to open, `Ctrl-r` to resume the selected one; `r` also resumes from the opened view) |
| `[` | scroll back through the focused session's output (see below) |
| `b` | send a literal `Ctrl-b` |
| `q` | quit |

### Scrolling a pane

`Ctrl-b [` enters scroll mode for the focused session (10,000 lines are kept
per session). The pane title shows how far back you are.

| Key | Action |
| --- | --- |
| `k` / `Up`, `j` / `Down` | one line back / forward |
| `u` / `PageUp`, `d` / `PageDown` / `Space` | one page back / forward |
| `g` / `Home` | oldest line |
| `G` / `End` | back to live output |
| `Esc` / `q` | leave scroll mode (returns to live output) |

### Routing input

`Ctrl-b i` opens a prompt. Type anything and cchordes guesses the target session
from `@name` / `@N` (explicit), or from word overlap with each session's screen,
name and working directory. The guess is shown live; `Tab` changes it (the last
choice starts a new session with your text as its first prompt), `Enter` sends.
No API calls are involved.

Status is detected from the screen ("esc to interrupt" means working), so it
may need adjusting if Claude Code's UI changes.

See [docs/product.md](docs/product.md) for scope and plans.

## Run from a checkout

`scripts/run.sh [cmd ...]` fast-forwards `main` (only if it is clean), rebuilds
in release mode and starts cchordes in your current directory. Link it onto
your `PATH` to start or restart from anywhere:

```sh
ln -s "$PWD/scripts/run.sh" ~/.local/bin/cchordes-run
```

## Development

```sh
cargo test                      # unit, PTY and screen snapshot tests
cargo insta review              # accept or reject changed snapshots (cargo install cargo-insta)
INSTA_UPDATE=always cargo test  # or overwrite them all, then check `git diff src/snapshots`
```

Screen snapshots live in `src/snapshots/`. They render the TUI to a test
backend and replay key presses, so no real terminal is needed.

## License

MIT
