#!/usr/bin/env bash
# Builds the two browser bundles (public and app) from the one frontend crate.
# Steps: cargo (wasm32) -> wasm-bindgen -> wasm-opt -> hashed names -> generated boot.js -> Brotli.
# Output: dist/public and dist/app. The gateway serves them from LEADS_DIST.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
DIST="${LEADS_DIST:-$ROOT/dist}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
if ! command -v wasm-opt >/dev/null 2>&1; then
  echo "wasm-opt is required to build optimized browser bundles (install Binaryen)." >&2
  exit 1
fi
rm -rf "$DIST"
mkdir -p "$DIST"

build_bundle() {
  local bundle="$1" features="$2"
  echo "== $bundle bundle"
  cargo build -p frontend --lib --release --target wasm32-unknown-unknown --no-default-features --features "$features"
  local raw="$TARGET_DIR/wasm32-unknown-unknown/release/frontend.wasm"
  local work="$DIST/.work-$bundle"
  rm -rf "$work"; mkdir -p "$work"
  wasm-bindgen "$raw" --target web --no-typescript --out-dir "$work" --out-name frontend
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types \
    "$work/frontend_bg.wasm" -o "$work/frontend_bg.opt.wasm"
  local out="$DIST/$bundle"
  mkdir -p "$out"
  local wh jh
  wh="$(sha256sum "$work/frontend_bg.opt.wasm" | cut -c1-12)"
  jh="$(sha256sum "$work/frontend.js" | cut -c1-12)"
  cp "$work/frontend_bg.opt.wasm" "$out/app.$wh.wasm"
  cp "$work/frontend.js" "$out/app.$jh.js"
  # The only script the page loads. It starts the module; the module starts itself.
  cat > "$out/boot.js" <<JS
import init from "/assets/$bundle/app.$jh.js";
await init({ module_or_path: "/assets/$bundle/app.$wh.wasm" });
document.getElementById("boot")?.remove();
JS
  for f in "$out"/*; do brotli -q 11 -f -k "$f"; done
  rm -rf "$work"
  echo "   sizes (bytes): wasm $(stat -c %s "$out/app.$wh.wasm") -> br $(stat -c %s "$out/app.$wh.wasm.br"), js $(stat -c %s "$out/app.$jh.js") -> br $(stat -c %s "$out/app.$jh.js.br")"
}

build_bundle public public
build_bundle app app
echo "Done. Files are in $DIST"
