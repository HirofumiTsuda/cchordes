# cchordes

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
| `H` | history (type to search, Enter to open) |
| `b` | send a literal `Ctrl-b` |
| `q` | quit |

Status is detected from the screen ("esc to interrupt" means working), so it
may need adjusting if Claude Code's UI changes.

See [docs/product.md](docs/product.md) for scope and plans.

## License

MIT
