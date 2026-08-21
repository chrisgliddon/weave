#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
required_version="0.2.127"
runner="$repo_root/target/web-tools/bin/wasm-bindgen-test-runner"

if [[ ! -x "$runner" ]]; then
  runner="$(command -v wasm-bindgen-test-runner || true)"
fi
if [[ -z "$runner" ]] || \
  [[ "$("$runner" --version)" != "wasm-bindgen-test-runner $required_version" ]]; then
  echo "Weave WASM tests require wasm-bindgen-test-runner $required_version" >&2
  echo "install it with: cargo install wasm-bindgen-cli --version $required_version --locked --root target/web-tools" >&2
  exit 2
fi

exec "$runner" "$@"
