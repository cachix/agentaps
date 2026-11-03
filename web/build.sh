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
mkdir -p dist/licenses
cp ../licenses/LUCIDE-ISC-MIT.txt dist/licenses/lucide.txt
