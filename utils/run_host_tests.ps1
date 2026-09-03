# Host lib tests. Default Cargo target is thumbv7em-none-eabihf (no std).
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$hostTarget = (rustc -vV | Select-String "^host:").ToString().Substring(6).Trim()
cargo test --lib --target $hostTarget
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK: host lib tests"
