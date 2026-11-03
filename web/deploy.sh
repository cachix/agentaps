#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
./build.sh
npx --yes wrangler@4.141.0 deploy --config site/wrangler.jsonc
