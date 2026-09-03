#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# 95% lines on this crate only (cargo-llvm-cov ignores deps by default).
HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
cargo llvm-cov --lib --target "$HOST_TARGET" --fail-under-lines 95
echo "OK: src/ line coverage >= 95%"
