#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Clippy is the complexity source of truth (cognitive_complexity).
cargo clippy --lib --target x86_64-unknown-linux-gnu -- -D warnings
echo "OK: clippy -D warnings on lib (host)"
