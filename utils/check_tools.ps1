# Check tools needed to build, test, cover, and flash this crate.
# Prints install commands for anything missing. Does not install.
$ErrorActionPreference = "Continue"

$script:missing = New-Object System.Collections.Generic.List[string]

function Have-Cmd([string]$Name) {
    [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Show-Ok([string]$Name, [string]$Detail = "") {
    if ($Detail) {
        Write-Host ("OK      {0,-22} {1}" -f $Name, $Detail)
    } else {
        Write-Host ("OK      {0}" -f $Name)
    }
}

function Show-Missing([string]$Name, [string]$Install) {
    Write-Host ("MISSING {0}" -f $Name)
    if (-not $script:missing.Contains($Install)) {
        $script:missing.Add($Install)
    }
}

function Get-Python {
    foreach ($cand in @("python", "python3", "py")) {
        if (Have-Cmd $cand) { return $cand }
    }
    return $null
}

Write-Host "microbit-minicar tool check (Windows 11 PowerShell)"
Write-Host ""

Write-Host "[host build / test / clippy]"
if (Have-Cmd "rustc") {
    Show-Ok "rustc" (rustc --version)
} else {
    Show-Missing "rustc" "winget install Rustlang.Rustup"
    Show-Missing "rustc" "# or: Invoke-WebRequest https://win.rustup.rs/x86_64 -OutFile `$env:TEMP\rustup-init.exe; & `$env:TEMP\rustup-init.exe -y"
}
if (Have-Cmd "cargo") {
    Show-Ok "cargo" (cargo --version)
} else {
    Show-Missing "cargo" "winget install Rustlang.Rustup"
}
if (Have-Cmd "rustup") {
    Show-Ok "rustup" (rustup --version 2>&1 | Select-Object -First 1)
} else {
    Show-Missing "rustup" "winget install Rustlang.Rustup"
}

$clippyOk = $false
if (Have-Cmd "cargo") {
    cargo clippy --version 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { $clippyOk = $true }
}
if ($clippyOk) {
    Show-Ok "clippy" (cargo clippy --version)
} else {
    Show-Missing "clippy" "rustup component add clippy"
}

Write-Host ""
Write-Host "[host coverage]"
$llvmOk = $false
if (Have-Cmd "rustup") {
    $comps = rustup component list --installed 2>$null
    if ("$comps" -match "llvm-tools") { $llvmOk = $true }
}
if ($llvmOk) {
    Show-Ok "llvm-tools-preview"
} else {
    Show-Missing "llvm-tools-preview" "rustup component add llvm-tools-preview"
}
$covOk = $false
if (Have-Cmd "cargo") {
    cargo llvm-cov --version 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { $covOk = $true }
}
if ($covOk) {
    Show-Ok "cargo-llvm-cov" (cargo llvm-cov --version)
} else {
    Show-Missing "cargo-llvm-cov" "cargo install cargo-llvm-cov --locked"
}

Write-Host ""
Write-Host "[firmware / flash]"
$thumbOk = $false
if (Have-Cmd "rustup") {
    $targets = rustup target list --installed 2>$null
    if ("$targets" -match "thumbv7em-none-eabihf") { $thumbOk = $true }
}
if ($thumbOk) {
    Show-Ok "thumbv7em-none-eabihf"
} else {
    Show-Missing "thumbv7em-none-eabihf" "rustup target add thumbv7em-none-eabihf"
}
if (Have-Cmd "probe-rs") {
    Show-Ok "probe-rs" (probe-rs --version 2>&1 | Select-Object -First 1)
} else {
    Show-Missing "probe-rs" "irm https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.ps1 | iex"
    Show-Missing "probe-rs" "# or: cargo install probe-rs-tools --locked"
}
$flashOk = $false
if (Have-Cmd "cargo") {
    cargo flash --version 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { $flashOk = $true }
}
if ($flashOk) {
    Show-Ok "cargo-flash" (cargo flash --version 2>&1 | Select-Object -First 1)
} else {
    Show-Missing "cargo-flash" "irm https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.ps1 | iex"
}

Write-Host ""
Write-Host "[python / lizard / serial GUI]"
$py = Get-Python
if ($py) {
    Show-Ok "python" (& $py --version 2>&1)
    & $py -m lizard --version 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Show-Ok "lizard" (& $py -m lizard --version 2>&1 | Select-Object -First 1)
    } else {
        Show-Missing "lizard" "python -m pip install lizard"
    }
    & $py -c "import serial" 2>$null
    if ($LASTEXITCODE -eq 0) {
        Show-Ok "pyserial"
    } else {
        Show-Missing "pyserial" "python -m pip install pyserial"
    }
    & $py -c "import tkinter" 2>$null
    if ($LASTEXITCODE -eq 0) {
        Show-Ok "tkinter"
    } else {
        Show-Missing "tkinter" "# reinstall Python with tcl/tk (python.org or: scoop install python)"
    }
} else {
    Show-Missing "python" "winget install Python.Python.3.13"
    Show-Missing "lizard" "python -m pip install lizard"
    Show-Missing "pyserial" "python -m pip install pyserial"
}

Write-Host ""
if ($script:missing.Count -gt 0) {
    Write-Host "Install commands for Windows 11 PowerShell:"
    foreach ($cmd in $script:missing) {
        Write-Host "  $cmd"
    }
    Write-Host ""
    Write-Host "After rustup or Python install, open a new PowerShell so PATH updates."
    exit 1
}
Write-Host "All required tools are installed."
exit 0
