#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Approach 2: flashes a test ELF via onboard DAPLink (overwrites clock_idle).
# Default probe-rs SWD double-buffering often USB-times-out on DAPLink.
# Always apply USB-safe flags. A leftover CARGO_TARGET_*_RUNNER from an older
# session overrides .cargo/config.toml and causes DAPLink Transfer timeouts.
RUNNER="${MICROBIT_PROBE_RS_RUNNER:-probe-rs run --chip nRF52833_xxAA --probe 0d28:0204-5 --protocol swd --speed 100 --disable-double-buffering --non-interactive}"
export CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER="$RUNNER"

echo "Using runner:"
echo "  $CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER"
echo "Close clock_gui / serial terminals first (one process owns the micro:bit USB)."
echo
probe-rs list

run_tests() {
  cargo test --features on-target --test on_target --target thumbv7em-none-eabihf
}

if ! run_tests; then
  echo
  echo "Flash/USB timed out. Retrying once in 3s (unplug is not required for this retry)..."
  sleep 3
  if ! run_tests; then
    echo
    echo "On-target tests still failed. This is a DAPLink USB/SWD error, not a Rust compile error."
    echo "Recover, then re-run this script:"
    echo "  1. Close anything using the micro:bit CDC port (clock_gui, minicom, picocom)."
    echo "  2. Unplug the micro:bit USB, wait 2 seconds, plug it back in."
    echo "  3. Confirm: probe-rs list   (want BBC micro:bit CMSIS-DAP -- 0d28:0204)"
    echo "  4. ./utils/run_on_target_tests.sh"
    echo "Ignore a Raspberry Pi debug probe (2e8a:000c); this board has no SWD header."
    exit 1
  fi
fi

echo "Re-flash the app:"
echo "  cargo flash --chip nRF52833_xxAA --probe 0d28:0204-5 --protocol swd --speed 100 --disable-double-buffering --example clock_idle"
