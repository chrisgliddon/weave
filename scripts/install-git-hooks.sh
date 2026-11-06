#!/usr/bin/env bash
# Point this clone at the repository hooks and make them executable.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

chmod +x .githooks/*
git config core.hooksPath .githooks
echo "Git hooks installed: core.hooksPath=.githooks"
echo "The pre-commit hook requires gitleaks: brew install gitleaks"
