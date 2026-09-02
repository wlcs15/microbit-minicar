#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# 95% lines on this crate only (cargo-llvm-cov ignores deps by default).
cargo llvm-cov --lib --target x86_64-unknown-linux-gnu --fail-under-lines 95
echo "OK: src/ line coverage >= 95%"
