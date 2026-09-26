#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
: "${CC_wasm32_unknown_unknown:=clang}"
export CC_wasm32_unknown_unknown
if [[ -n "${NO_COLOR:-}" ]]; then
  export NO_COLOR=true
fi
trunk build index.html --release --public-url /connect/ --dist dist/connect
cp -R site/. dist/
mkdir -p dist/fonts
cp fonts/IBMPlexSans-Regular.ttf dist/fonts/
cp fonts/LICENSE.txt dist/fonts/
