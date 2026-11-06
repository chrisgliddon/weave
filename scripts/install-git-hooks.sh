#!/usr/bin/env bash
# Install the repository hooks into this clone.
#
# The hooks are copied into the clone's own .git/hooks directory rather than
# loaded from the tracked working tree, so checking out a branch cannot change
# the code that runs on your next commit. Re-run this script after changing a
# file in .githooks/.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [ "$(git config --get core.hooksPath || true)" = ".githooks" ]; then
  git config --unset core.hooksPath
  echo "Removed the previous core.hooksPath override"
fi

hooks_dir="$(git rev-parse --git-path hooks)"
mkdir -p "$hooks_dir"

for hook in .githooks/*; do
  [ -f "$hook" ] || continue
  name="$(basename "$hook")"
  cp "$hook" "$hooks_dir/$name"
  chmod +x "$hooks_dir/$name"
  echo "Installed $name"
done

echo "The pre-commit hook requires gitleaks: brew install gitleaks"
