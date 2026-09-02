# microbit-minicar

Rust drivers and examples for the **HolaSmart HS1002** car on a BBC **micro:bit v2**.

This is the **wlcs15** fork (`https://github.com/wlcs15/microbit-minicar`). Keyestudio MiniCar motor encoding is not kept. Branch for this port: `holasmart_HS1002`.

## What this crate gives you

- Motor control (HS1002 PWM expander at I2C `0x30`)
- RGB LED control
- Line tracking, ultrasonic helpers
- Software wall clock and an **on-chip flash log** (last 8 KiB)

Library: `src/`. Board wiring: `examples/`.

## Unit tests: host only

Tests run on the PC, not on the micro:bit:

```bash
./utils/run_host_tests.sh
./utils/run_coverage.sh    # 95% lines on this crate's src/ only
./utils/run_clippy.sh      # Clippy is the complexity gate
```

On-target “tests” are the flashed examples.

## Clock, serial, and flash log

The nRF52833 has RTC **counters**, not a battery-backed calendar. Time runs only while the MCU is powered.

**Preferred workflow:** log events to **internal flash**, use USB serial only in short sessions to set time and dump the log. That is better for floor runs than keeping a serial console open. A host GUI (`tools/clock_gui.py`) is for those short sessions. It does **not** replace flash logging.

| Power | Live clock | Flash log |
|---|---|---|
| USB, or batteries + switch feeding the micro:bit | Runs | Kept |
| USB unplugged and switch off / batteries out | Stops, time lost | **Kept** |
| Reset with power still on | Counter restarts; set `T=` again | Kept |

Keep the RTC running: **power switch ON**, batteries in, avoid reset/reflash. USB can stay plugged; it always powers the micro:bit.

### `clock_idle` (what should be on the board)

- Motors **stopped** (safe with switch ON).
- 5×5 shows **T** until wall time is set. A **U** flash means a UART byte was received.
- After `T=<unix>` (UTC), scrolls **`MMDDYYYY HHMMSS`**.
- USB serial 115200. **Debug stream is ON by default** (`dbg ticks=...`). Any key that is not a `T=` line stops debug and prints a **MENU**.
- Menu: `1` status, `2` dump, `3` start debug, `4` on-target tests, `5` LED 1–9, `6` show RTC, `7` clear RTC to 000000 msec, `T=<10-digit unix>` set clock, `?` menu. While debug is on, any key except `T=` stops debug and shows the menu.
- **Button A:** shows flash-log **count** as a single digit (a `1` means one record, not a 1–9 animation). Serial dump if CDC works.
- Log region: `0x0007E000`–`0x00080000`.

On-target tests (menu `4`) run the same checks as host `src/selftest.rs` and print `PASS`/`FAIL` on serial. `cargo test` still runs only on the host.

### Serial GUI vs minicom vs debug

Only **one** program can own `/dev/ttyACM1`. Close the Python GUI before minicom (and the reverse).

DAPLink **resets the nRF on DTR/RTS**. That is why minicom at 115200 can look dead while `clock_gui.py` works: the GUI forces DTR/RTS off. Host `cargo test` / PTY tests never open DAPLink, so they can pass while a DTR terminal still fails on hardware.

**Preferred terminal (same as the GUI):**

```bash
python3 -m serial.tools.miniterm /dev/ttyACM1 115200 --dtr 0 --rts 0
# Ctrl-] to quit
```

**picocom:**

```bash
picocom -b 115200 --lower-dtr --lower-rts /dev/ttyACM1
# Ctrl-A Ctrl-X to quit
```

**minicom** (baud is not enough; disable flow control, skip modem init):

```bash
stty -F /dev/ttyACM1 115200 raw -echo -hupcl -crtscts
minicom -D /dev/ttyACM1 -b 115200 -o
```

Then `Ctrl-A O` → Serial port setup → **Hardware flow control: No**, **Software flow control: No**. Local echo is off by default — type `?` then Enter; you should see `---DBG OFF---` and `MENU`. Type `T=` plus UTC unix plus Enter to set time.

```bash
python3 tools/clock_gui.py   # Open /dev/ttyACM1, not ACM0 (Pi debug probe)
```

## Examples (flash order)

Do **not** flash `motor` until mapping is done. It is a continuous drive loop and drains AAAs.

1. `clock_idle` — stop motors, set clock, flash log (current)
2. `led_color_set` — I2C RGB, motors stopped
3. `accel_motor_map` — short motor pulses if chassis is at rest
4. `motor_spin` — one motor at a time
5. `motor` — last, full drive cycle

```bash
cargo flash --chip nRF52833_xxAA --probe 0d28:0204 --example clock_idle
```

Use `--probe 0d28:0204` so the micro:bit is selected, not another CMSIS-DAP.

## Library use

```toml
[dependencies]
microbit-minicar = { git = "https://github.com/wlcs15/microbit-minicar" }
```

Target: `thumbv7em-none-eabihf`.
