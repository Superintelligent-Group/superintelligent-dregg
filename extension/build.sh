#!/usr/bin/env bash
# Build the pyana-wasm crate and copy outputs into the extension directory.
# Requirements: cargo, wasm-bindgen-cli (cargo install wasm-bindgen-cli)
# No npm needed.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WASM_CRATE="$PROJECT_ROOT/wasm"
TARGET_DIR="$PROJECT_ROOT/target"
WASM_OUT="$TARGET_DIR/wasm32-unknown-unknown/release/pyana_wasm.wasm"

echo "[1/3] Building pyana-wasm (release, wasm32-unknown-unknown)..."
cargo build \
  --manifest-path "$WASM_CRATE/Cargo.toml" \
  -p pyana-wasm \
  --target wasm32-unknown-unknown \
  --release

if [ ! -f "$WASM_OUT" ]; then
  echo "ERROR: Expected output not found at $WASM_OUT"
  exit 1
fi

echo "[2/3] Running wasm-bindgen..."
wasm-bindgen "$WASM_OUT" \
  --out-dir "$SCRIPT_DIR" \
  --target web \
  --no-typescript \
  --omit-default-module-path

echo "[3/3] Verifying outputs..."
if [ -f "$SCRIPT_DIR/pyana_wasm_bg.wasm" ] && [ -f "$SCRIPT_DIR/pyana_wasm.js" ]; then
  WASM_SIZE=$(wc -c < "$SCRIPT_DIR/pyana_wasm_bg.wasm" | tr -d ' ')
  echo "Done. Output:"
  echo "  $SCRIPT_DIR/pyana_wasm.js"
  echo "  $SCRIPT_DIR/pyana_wasm_bg.wasm ($WASM_SIZE bytes)"
else
  echo "ERROR: wasm-bindgen did not produce expected outputs."
  echo "Expected: pyana_wasm.js and pyana_wasm_bg.wasm in $SCRIPT_DIR"
  ls -la "$SCRIPT_DIR"/pyana_wasm* 2>/dev/null || true
  exit 1
fi
