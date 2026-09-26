#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v clang >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y clang
fi

build_tools=$(mktemp -d)
trap 'rm -rf "$build_tools"' EXIT

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

export CC_wasm32_unknown_unknown=clang
export CARGO_BUILD_JOBS=2
bash web/build.sh
