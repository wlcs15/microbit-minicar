# Cyclomatic complexity gate is 10 (not 15).
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
python -m lizard src examples -l rust -C 10 -L 1000
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK: lizard CCN <= 10 (src + examples)"
