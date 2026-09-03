#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Default .cargo/config.toml target is thumbv7em-none-eabihf (no std / no libtest).
HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
cargo test --lib --target "$HOST_TARGET"
echo "OK: host lib tests"
