#!/usr/bin/env bash
# Build crates/weave-ffi for this machine and stage it into the Unreal plugin.
# Usage: integrations/unreal/stage-ffi.sh [path/to/Project/Plugins/Weave]   (default: the plugin in this repo)
set -euo pipefail
ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
PLUGIN="${1:-$ROOT/integrations/unreal/Weave}"
cargo build --release -p weave-ffi --manifest-path "$ROOT/Cargo.toml"
case "$(uname)" in
  Linux)  SRC=libweave_ffi.so;    PLAT=Linux ;;
  Darwin) SRC=libweave_ffi.dylib; PLAT=Mac ;;
  *)      SRC=weave_ffi.dll;      PLAT=Win64 ;;
esac
DEST="$PLUGIN/Binaries/ThirdParty/Weave/$PLAT"
mkdir -p "$DEST"
cp "$ROOT/target/release/$SRC" "$DEST/"
[ "$PLAT" = Mac ] && install_name_tool -id "@rpath/$SRC" "$DEST/$SRC"
echo "staged $DEST/$SRC"
