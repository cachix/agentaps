#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

build_tools=$(mktemp -d)
trap 'rm -rf "$build_tools"' EXIT

if command -v clang >/dev/null 2>&1; then
  wasm_clang=$(command -v clang)
else
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
    https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-x86_64-linux.tar.gz \
    -o "$build_tools/wasi-sdk.tar.gz"
  printf '%s  %s\n' \
    b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4 \
    "$build_tools/wasi-sdk.tar.gz" | sha256sum --check --status
  tar -xzf "$build_tools/wasi-sdk.tar.gz" -C "$build_tools"
  wasm_clang="$build_tools/wasi-sdk-34.0-x86_64-linux/bin/clang"
fi

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
    https://sh.rustup.rs -o "$build_tools/rustup-init.sh"
  sh "$build_tools/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain none
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.97.1 --profile minimal --target wasm32-unknown-unknown
rustup default 1.97.1

curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
  https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
  -o "$build_tools/trunk.tar.gz"
printf '%s  %s\n' \
  f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b \
  "$build_tools/trunk.tar.gz" | sha256sum --check --status
tar -xzf "$build_tools/trunk.tar.gz" -C "$build_tools"
export PATH="$build_tools:$PATH"

export CC_wasm32_unknown_unknown="$wasm_clang"
export CARGO_BUILD_JOBS=2
bash web/build.sh
