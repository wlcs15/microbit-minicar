#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Approach 2: flashes a test ELF via onboard DAPLink (overwrites clock_idle).
export CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER="${CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER:-probe-rs run --chip nRF52833_xxAA --probe 0d28:0204}"
cargo test --features on-target --test on_target --target thumbv7em-none-eabihf
echo "Re-flash the app: cargo flash --chip nRF52833_xxAA --probe 0d28:0204 --example clock_idle"
