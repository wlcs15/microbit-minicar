#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Cyclomatic complexity gate is 10 (not 15).
lizard src examples -l rust -C 10 -L 1000
echo "OK: lizard CCN <= 10 (src + examples)"
