#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Clippy is the complexity source of truth (cognitive_complexity).
HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
cargo clippy --lib --target "$HOST_TARGET" -- -D warnings
echo "OK: clippy -D warnings on lib (host)"
