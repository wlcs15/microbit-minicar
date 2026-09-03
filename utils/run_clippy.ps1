# Clippy is the complexity source of truth (cognitive_complexity).
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$hostTarget = (rustc -vV | Select-String "^host:").ToString().Substring(6).Trim()
cargo clippy --lib --target $hostTarget -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK: clippy -D warnings on lib (host)"
