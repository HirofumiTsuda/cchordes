#!/usr/bin/env bash
# Update the checkout (when safe) and start cchordes from it.
# Usage: scripts/run.sh [cchordes args...]   e.g. scripts/run.sh claude --model opus
# Works from any directory and through a symlink.
set -euo pipefail

repo="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
cd "$repo"

# Only fast-forward a clean checkout of main; never touch work in progress.
if [ "$(git rev-parse --abbrev-ref HEAD)" = main ] && [ -z "$(git status --porcelain)" ]; then
  git pull --ff-only --quiet || echo "warning: git pull failed, running the current checkout" >&2
else
  echo "note: not on a clean main, skipping git pull" >&2
fi

# cchordes runs in the caller's directory so new sessions start there.
cargo build --release --quiet --manifest-path "$repo/Cargo.toml"
cd "$OLDPWD"
exec "$repo/target/release/cchordes" "$@"
