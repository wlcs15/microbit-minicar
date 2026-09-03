# Approach 2: flashes a test ELF via onboard DAPLink (overwrites clock_idle).
# Default probe-rs SWD double-buffering often USB-times-out on Windows DAPLink.
$ErrorActionPreference = "Continue"
Set-Location (Split-Path -Parent $PSScriptRoot)

# Always apply USB-safe flags. A leftover CARGO_TARGET_*_RUNNER from an older
# session (without --disable-double-buffering) overrides .cargo/config.toml and
# causes DAPLink Transfer timeouts at ~97% program.
$runner = "probe-rs run --chip nRF52833_xxAA --probe 0d28:0204-5 --protocol swd --speed 100 --disable-double-buffering --non-interactive"
if ($env:MICROBIT_PROBE_RS_RUNNER) {
    $runner = $env:MICROBIT_PROBE_RS_RUNNER
}
$env:CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER = $runner

Write-Host "Using runner:"
Write-Host "  $($env:CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER)"
Write-Host "Close clock_gui / serial terminals first (one process owns the micro:bit USB)."
Write-Host ""
probe-rs list
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

function Invoke-OnTargetTests {
    cargo test --features on-target --test on_target --target thumbv7em-none-eabihf
    return $LASTEXITCODE
}

$code = Invoke-OnTargetTests
if ($code -ne 0) {
    Write-Host ""
    Write-Host "Flash/USB timed out. Retrying once in 3s (unplug is not required for this retry)..."
    Start-Sleep -Seconds 3
    $code = Invoke-OnTargetTests
}

if ($code -ne 0) {
    Write-Host ""
    Write-Host "On-target tests still failed. This is a DAPLink USB/SWD error, not a Rust compile error."
    Write-Host "Recover, then re-run this script:"
    Write-Host "  1. Close anything using the micro:bit COM port (clock_gui, miniterm, PuTTY)."
    Write-Host "  2. Unplug the micro:bit USB, wait 2 seconds, plug it back in."
    Write-Host "  3. Confirm: probe-rs list   (want BBC micro:bit CMSIS-DAP -- 0d28:0204)"
    Write-Host "  4. .\utils\run_on_target_tests.ps1"
    Write-Host "Ignore the Raspberry Pi debug probe (2e8a:000c); this board has no SWD header."
    exit $code
}

Write-Host "Re-flash the app:"
Write-Host "  cargo flash --chip nRF52833_xxAA --probe 0d28:0204-5 --protocol swd --speed 100 --disable-double-buffering --example clock_idle"
exit 0
