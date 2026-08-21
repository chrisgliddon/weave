#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
example_dir="$repo_root/examples/web-player"
build_dir="$repo_root/target/web-player"
wasm_bindgen_version="0.2.127"
wasm_bindgen_bin="$repo_root/target/web-tools/bin/wasm-bindgen"

if [[ ! -x "$wasm_bindgen_bin" ]]; then
  wasm_bindgen_bin="$(command -v wasm-bindgen || true)"
fi
if [[ -z "$wasm_bindgen_bin" ]] || \
  [[ "$("$wasm_bindgen_bin" --version)" != "wasm-bindgen $wasm_bindgen_version" ]]; then
  echo "weave web build requires wasm-bindgen $wasm_bindgen_version" >&2
  echo "install it with: cargo install wasm-bindgen-cli --version $wasm_bindgen_version --locked --root target/web-tools" >&2
  exit 2
fi

cd "$repo_root"
CARGO_TARGET_DIR="$build_dir" RUSTC_WRAPPER= cargo run -p weave-compiler --locked -- \
  examples/web-player/story.weave \
  --format json \
  --output examples/web-player/story.json
CARGO_TARGET_DIR="$build_dir" RUSTC_WRAPPER= cargo build \
  -p weave-web \
  --target wasm32-unknown-unknown \
  --release \
  --locked
mkdir -p "$example_dir/pkg"
"$wasm_bindgen_bin" \
  --target web \
  --out-dir "$example_dir/pkg" \
  --out-name weave_web \
  --no-typescript \
  "$build_dir/wasm32-unknown-unknown/release/weave_web.wasm"

wasm_bytes="$(wc -c < "$example_dir/pkg/weave_web_bg.wasm" | tr -d ' ')"
js_bytes="$(wc -c < "$example_dir/pkg/weave_web.js" | tr -d ' ')"
echo "web player built: wasm=${wasm_bytes} bytes js=${js_bytes} bytes"
