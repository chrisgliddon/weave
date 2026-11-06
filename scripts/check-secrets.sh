#!/usr/bin/env bash
# Scan for committed credentials with gitleaks.
#
# Usage:
#   scripts/check-secrets.sh staged    # staged changes, used by the pre-commit hook
#   scripts/check-secrets.sh history   # every commit reachable from HEAD
#
# Exit codes: 0 clean, 1 leaks found, 2 gitleaks missing.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! command -v gitleaks >/dev/null 2>&1; then
  echo "gitleaks is not installed. See https://github.com/gitleaks/gitleaks#installing" >&2
  echo "Homebrew: brew install gitleaks" >&2
  exit 2
fi

mode="${1:-history}"
case "$mode" in
  staged)
    gitleaks git --staged --no-banner --redact --config .gitleaks.toml
    ;;
  history)
    gitleaks git --no-banner --redact --config .gitleaks.toml
    ;;
  *)
    echo "usage: $0 [staged|history]" >&2
    exit 2
    ;;
esac
