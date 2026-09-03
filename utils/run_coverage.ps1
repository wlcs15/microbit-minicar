# 95% lines on this crate only (cargo-llvm-cov ignores deps by default).
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$hostTarget = (rustc -vV | Select-String "^host:").ToString().Substring(6).Trim()
cargo llvm-cov --lib --target $hostTarget --fail-under-lines 95
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK: src/ line coverage >= 95%"
