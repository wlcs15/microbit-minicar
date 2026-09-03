#!/usr/bin/env bash
# Check tools needed to build, test, cover, and flash this crate.
# Prints install commands for anything missing. Does not install.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

missing=()
have() { command -v "$1" >/dev/null 2>&1; }

ok() {
  if [ -n "${2-}" ]; then
    printf 'OK      %-22s %s\n' "$1" "$2"
  else
    printf 'OK      %s\n' "$1"
  fi
}

need() {
  printf 'MISSING %s\n' "$1"
  missing+=("$2")
}

py=""
if have python3; then
  py=python3
elif have python; then
  py=python
fi

echo "microbit-minicar tool check (Ubuntu x86 Linux)"
echo

echo "[host build / test / clippy]"
if have rustc; then
  ok rustc "$(rustc --version)"
else
  need rustc "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi
if have cargo; then
  ok cargo "$(cargo --version)"
else
  need cargo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi
if have rustup; then
  ok rustup "$(rustup --version 2>/dev/null | head -n1)"
else
  need rustup "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi
if have cargo && cargo clippy --version >/dev/null 2>&1; then
  ok clippy "$(cargo clippy --version)"
else
  need clippy "rustup component add clippy"
fi

echo
echo "[host coverage]"
if have rustup && rustup component list --installed 2>/dev/null | grep -q llvm-tools; then
  ok llvm-tools-preview
else
  need llvm-tools-preview "rustup component add llvm-tools-preview"
fi
if have cargo && cargo llvm-cov --version >/dev/null 2>&1; then
  ok cargo-llvm-cov "$(cargo llvm-cov --version)"
else
  need cargo-llvm-cov "cargo install cargo-llvm-cov --locked"
fi

echo
echo "[firmware / flash]"
if have rustup && rustup target list --installed 2>/dev/null | grep -qx thumbv7em-none-eabihf; then
  ok thumbv7em-none-eabihf
else
  need thumbv7em-none-eabihf "rustup target add thumbv7em-none-eabihf"
fi
if have probe-rs; then
  ok probe-rs "$(probe-rs --version | head -n1)"
else
  need probe-rs "curl --proto '=https' --tlsv1.2 -LsSf https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.sh | sh"
  need probe-rs "cargo install probe-rs-tools --locked"
fi
if have cargo && cargo flash --version >/dev/null 2>&1; then
  ok cargo-flash "$(cargo flash --version | head -n1)"
else
  need cargo-flash "curl --proto '=https' --tlsv1.2 -LsSf https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.sh | sh"
fi

echo
echo "[python / lizard / serial GUI]"
if [ -n "$py" ]; then
  ok python "$($py --version 2>&1)"
   if have lizard; then
    ok lizard "$(lizard --version 2>&1 | head -n1)"
  elif $py -m lizard --version >/dev/null 2>&1; then
    ok lizard "$($py -m lizard --version 2>&1 | head -n1)"
  else
    need lizard "pipx install lizard"
   fi 
  if $py -c "import serial" >/dev/null 2>&1; then
    ok pyserial
  else
    need pyserial "sudo apt-get install -y python3-serial"
    need pyserial "$py -m pip install --user pyserial"
  fi
  if $py -c "import tkinter" >/dev/null 2>&1; then
    ok tkinter
  else
    need tkinter "sudo apt-get install -y python3-tk"
  fi
else
  need python "sudo apt-get update && sudo apt-get install -y python3 python3-pip python3-tk python3-serial"
  need lizard "python3 -m pip install --user lizard"
fi

echo
if [ "${#missing[@]}" -gt 0 ]; then
  echo "Install commands for Ubuntu x86 Linux:"
  printf '  %s\n' "${missing[@]}" | awk 'NF && !seen[$0]++'
  echo
  echo "After rustup install, run: source \"\$HOME/.cargo/env\""
  exit 1
fi
echo "All required tools are installed."
exit 0
