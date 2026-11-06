#!/usr/bin/env bash
# Scan for committed credentials with gitleaks.
#
# Usage:
#   scripts/check-secrets.sh staged    # staged changes, used by the pre-commit hook
#   scripts/check-secrets.sh history   # every commit reachable from HEAD
#
# The staged scan reads the policy from HEAD, not the working copy, so a commit
# cannot weaken the scan that checks it. The history scan reads the checked-out
# policy and warns when it differs from HEAD.
#
# Exit codes: 0 clean, 1 leaks found, 2 gitleaks missing or policy unreadable.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

mode="${1:-history}"
case "$mode" in
  staged | history) ;;
  *)
    echo "usage: $0 [staged|history]" >&2
    exit 2
    ;;
esac

if ! command -v gitleaks >/dev/null 2>&1; then
  echo "gitleaks is not installed. See https://github.com/gitleaks/gitleaks#installing" >&2
  echo "Homebrew: brew install gitleaks" >&2
  exit 2
fi

policy="$(mktemp)"
trap 'rm -f "$policy"' EXIT

committed_policy() {
  git cat-file -e HEAD:.gitleaks.toml 2>/dev/null && git show HEAD:.gitleaks.toml
}

if [ "$mode" = staged ]; then
  # Never trust a policy file from the commit under test.
  if committed_policy >"$policy"; then
    :
  elif [ -f .gitleaks.toml ]; then
    cp .gitleaks.toml "$policy"
  else
    echo "no .gitleaks.toml in HEAD or the working tree" >&2
    exit 2
  fi
else
  if [ ! -f .gitleaks.toml ]; then
    echo "no .gitleaks.toml in the working tree" >&2
    exit 2
  fi
  cp .gitleaks.toml "$policy"
  if committed_policy >/dev/null 2>&1 && ! committed_policy | cmp -s - .gitleaks.toml; then
    echo "note: .gitleaks.toml differs from HEAD" >&2
  fi
fi

case "$mode" in
  staged)
    gitleaks git --staged --no-banner --redact --config "$policy"
    ;;
  history)
    gitleaks git --no-banner --redact --config "$policy"
    ;;
esac
