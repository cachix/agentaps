#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
: "${CC_wasm32_unknown_unknown:=clang}"
export CC_wasm32_unknown_unknown
# GPUI Kit's default web feature compiles wasm_thread with an unstable feature.
export RUSTC_BOOTSTRAP=wasm_thread
if [[ -n "${NO_COLOR:-}" ]]; then
  export NO_COLOR=true
fi
npm --prefix site ci
npm --prefix site run build
trunk build index.html --locked --release --public-url /connect/ --dist dist/connect
# Trunk 0.21 records the initializer's download size before wasm-opt runs.
# Use the final bundle size so the progress bar reflects the actual download.
wasm_size=$(wc -c < dist/connect/agentaps-web-*_bg.wasm)
sed -E "s@(__trunkInitializer\(init, '[^']+\.wasm', )[0-9]+@\1$((wasm_size))@" \
  dist/connect/index.html > dist/connect/index.html.tmp
mv dist/connect/index.html.tmp dist/connect/index.html
mkdir -p dist/licenses
cp ../licenses/LUCIDE-ISC-MIT.txt dist/licenses/lucide.txt
