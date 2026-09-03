@echo off
setlocal EnableExtensions EnableDelayedExpansion
rem Check tools needed to build, test, cover, and flash this crate.
rem Prints install commands for anything missing. Does not install.

set "MISSING=0"
set "INSTALL=%TEMP%\microbit-minicar-install.txt"
type nul > "%INSTALL%"

echo microbit-minicar tool check (Windows 11 cmd)
echo.

echo [host build / test / clippy]
where rustc >nul 2>&1
if errorlevel 1 (
  echo MISSING rustc
  echo winget install Rustlang.Rustup>> "%INSTALL%"
  set MISSING=1
) else (
  echo OK      rustc
  rustc --version
)
where cargo >nul 2>&1
if errorlevel 1 (
  echo MISSING cargo
  echo winget install Rustlang.Rustup>> "%INSTALL%"
  set MISSING=1
) else (
  echo OK      cargo
  cargo --version
)
where rustup >nul 2>&1
if errorlevel 1 (
  echo MISSING rustup
  echo winget install Rustlang.Rustup>> "%INSTALL%"
  set MISSING=1
) else (
  echo OK      rustup
  rustup --version 2>nul
)
where cargo >nul 2>&1
if not errorlevel 1 (
  cargo clippy --version >nul 2>&1
  if errorlevel 1 (
    echo MISSING clippy
    echo rustup component add clippy>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      clippy
    cargo clippy --version
  )
) else (
  echo MISSING clippy
  echo rustup component add clippy>> "%INSTALL%"
  set MISSING=1
)

echo.
echo [host coverage]
where rustup >nul 2>&1
if not errorlevel 1 (
  rustup component list --installed 2>nul | findstr /I /C:"llvm-tools" >nul
  if errorlevel 1 (
    echo MISSING llvm-tools-preview
    echo rustup component add llvm-tools-preview>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      llvm-tools-preview
  )
) else (
  echo MISSING llvm-tools-preview
  echo rustup component add llvm-tools-preview>> "%INSTALL%"
  set MISSING=1
)
where cargo >nul 2>&1
if not errorlevel 1 (
  cargo llvm-cov --version >nul 2>&1
  if errorlevel 1 (
    echo MISSING cargo-llvm-cov
    echo cargo install cargo-llvm-cov --locked>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      cargo-llvm-cov
    cargo llvm-cov --version
  )
) else (
  echo MISSING cargo-llvm-cov
  echo cargo install cargo-llvm-cov --locked>> "%INSTALL%"
  set MISSING=1
)

echo.
echo [firmware / flash]
where rustup >nul 2>&1
if not errorlevel 1 (
  rustup target list --installed 2>nul | findstr /C:"thumbv7em-none-eabihf" >nul
  if errorlevel 1 (
    echo MISSING thumbv7em-none-eabihf
    echo rustup target add thumbv7em-none-eabihf>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      thumbv7em-none-eabihf
  )
) else (
  echo MISSING thumbv7em-none-eabihf
  echo rustup target add thumbv7em-none-eabihf>> "%INSTALL%"
  set MISSING=1
)
where probe-rs >nul 2>&1
if errorlevel 1 (
  echo MISSING probe-rs
  echo powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.ps1 | iex">> "%INSTALL%"
  echo cargo install probe-rs-tools --locked>> "%INSTALL%"
  set MISSING=1
) else (
  echo OK      probe-rs
  probe-rs --version
)
where cargo >nul 2>&1
if not errorlevel 1 (
  cargo flash --version >nul 2>&1
  if errorlevel 1 (
    echo MISSING cargo-flash
    echo powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.ps1 | iex">> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      cargo-flash
    cargo flash --version
  )
) else (
  echo MISSING cargo-flash
  echo powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.ps1 | iex">> "%INSTALL%"
  set MISSING=1
)

echo.
echo [python / lizard / serial GUI]
set "PY="
where python >nul 2>&1
if not errorlevel 1 set "PY=python"
if not defined PY (
  where python3 >nul 2>&1
  if not errorlevel 1 set "PY=python3"
)
if not defined PY (
  echo MISSING python
  echo winget install Python.Python.3.13>> "%INSTALL%"
  echo python -m pip install lizard pyserial>> "%INSTALL%"
  set MISSING=1
) else (
  echo OK      python
  %PY% --version
  %PY% -m lizard --version >nul 2>&1
  if errorlevel 1 (
    echo MISSING lizard
    echo python -m pip install lizard>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      lizard
    %PY% -m lizard --version
  )
  %PY% -c "import serial" >nul 2>&1
  if errorlevel 1 (
    echo MISSING pyserial
    echo python -m pip install pyserial>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      pyserial
  )
  %PY% -c "import tkinter" >nul 2>&1
  if errorlevel 1 (
    echo MISSING tkinter
    echo rem reinstall Python with tcl/tk ^(python.org or scoop install python^)>> "%INSTALL%"
    set MISSING=1
  ) else (
    echo OK      tkinter
  )
)

echo.
if "%MISSING%"=="1" (
  echo Install commands for Windows 11 cmd:
  for /f "usebackq delims=" %%L in ("%INSTALL%") do echo   %%L
  echo.
  echo After rustup or Python install, open a new cmd so PATH updates.
  del "%INSTALL%" >nul 2>&1
  exit /b 1
)
echo All required tools are installed.
del "%INSTALL%" >nul 2>&1
exit /b 0
