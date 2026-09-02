# microbit-minicar

Rust drivers and examples for the **HolaSmart HS1002** car on a BBC **micro:bit v2**.

This is the **wlcs15** fork (`https://github.com/wlcs15/microbit-minicar`). Keyestudio MiniCar motor encoding is not kept. Branch for this port: `holasmart_HS1002`. Current tag: **v0.05** (+ mag 360 cal in tree).

## What this crate gives you

- Motor control (HS1002 PWM expander at I2C `0x30`)
- RGB LED control
- Line tracking, ultrasonic helpers
- Software wall clock and an **on-chip flash log** (last 8 KiB)
- IRQ-driven UART (separate RX/TX rings) in `clock_idle`

Library: `src/`. Board wiring: `examples/`.

## Quality gates

```bash
./utils/run_host_tests.sh     # cargo test --lib on x86_64
./utils/run_coverage.sh       # >= 95% lines on this crate's src/ (host llvm-cov)
./utils/run_clippy.sh         # Clippy -D warnings
./utils/run_lizard.sh         # cyclomatic complexity; fail if CCN > 10
```

**Cyclomatic complexity limit is 10** (not 15). Measured with [lizard](https://github.com/terryyin/lizard) (`-C 10`) on `src` and `examples`.

### Current complexity (v0.03 tree)

Totals: **2552** NLOC, **146** functions, **AvgCCN 2.1**. Three functions are **above 10**:

| CCN | Function | File |
|---|---|---|
| 14 | `measure_cm` | `src/ultra.rs` |
| 12 | `decode` | `src/log_store.rs` |
| 11 | `push_byte` | `src/serial_ui.rs` (lizard also folds the following `#[cfg(test)]` module into this count) |

Other library functions at CCN 6–9 (under the gate but worth watching):

| CCN | Function | File |
|---|---|---|
| 9 | `set_rgb`, `disable` | `src/led.rs` |
| 9 | `set` | `src/motor.rs` |
| 7 | `rest_status`, `classify_delta` | `src/motion.rs` |
| 6 | `parse_set_command` | `src/clock.rs` |
| 6 | `next_slot` | `src/log_store.rs` |
| 6 | `on_irq` | `examples/uart_irq.rs` |
| 6 | `main` | `examples/accel_motor_map.rs` |

Firmware `main` in `clock_idle` is long but lizard CCN is 2 (linear setup). Average CCN by file is highest in `src/serial_ui.rs` (4.0) and `examples/accel_motor_map.rs` / `examples/ultra.rs` (5.0).

### Current coverage

**Host** (`cargo llvm-cov --lib --target x86_64-unknown-linux-gnu`), this crate `src/` only:

| File | Lines | Regions |
|---|---|---|
| `bus.rs` | 100.00% | 100.00% |
| `clock.rs` | 97.28% | 97.79% |
| `led.rs` | 100.00% | 92.41% |
| `line_tracking.rs` | 93.18% | 90.28% |
| `log_store.rs` | 97.25% | 95.21% |
| `motion.rs` | 100.00% | 99.53% |
| `motor.rs` | 100.00% | 94.64% |
| `ring.rs` | 94.81% | 95.54% |
| `selftest.rs` | 95.33% | 96.79% |
| `serial_ui.rs` | 98.48% | 98.71% |
| `ultra.rs` | 98.20% | 95.85% |
| **TOTAL** | **97.67%** | **96.42%** |

Host unit tests: **61/61** pass.

**Target:** there is **no llvm-cov / gcov instrumentation** in the nRF52833 firmware, so there is **no line-coverage percentage on the board**. Menu `4` runs `check_eq` assertions (`src/selftest.rs::run_all`) and prints PASS/FAIL on serial. That is functional self-test, not coverage.

Of the 61 host `#[test]` functions, the **logic** is `no_std` and **can** be mirrored on-chip. Python PTY/GUI tests are host-only and are **not** required on the MCU.

## Debug connector: no user JTAG on micro:bit v2

The nRF52833 is debugged with **2-pin SWD** (SWDIO / SWCLK), not full JTAG. The BBC micro:bit v2 does **not** bring that SWD (or JTAG) out to the edge connector or a 10-pin ARM header.

- The application nRF52833 SWD is wired only to the **onboard interface MCU** (KL27 or nRF52820), which presents **USB CMSIS-DAP (DAPLink)** — the `0d28:0204` probe already used by `cargo flash`.
- Factory **test pads** TP11 (SWDCLK) and TP12 (SWDIO) exist under solder mask. They are not a connector.
- An external ARM JTAG/SWD probe that works on a Raspberry Pi Pico W (exposed SWD pads) **cannot attach to this board** without soldering those pads. It is not needed: DAPLink on USB is the supported debug path.

Do not flash J-Link OB firmware onto the interface MCU unless you intend to replace DAPLink.

## Unit tests: host and target

`cargo test` uses rustc’s **libtest** harness. That harness needs **`std`**, threads, and a process exit code. `thumbv7em-none-eabihf` is **bare metal** (tier 2, `core` only). So **`cargo test` without a custom harness never runs on the micro:bit**. That is the main difference from typical C/C++ on-target unit tests, where Unity/CppUTest/GoogleTest are just libraries you link into a firmware image.

This is **not** “Rust does not do on-target tests.” Industry practice for embedded Rust is the same three layers used in C: host unit tests, on-chip HIL, and host–target protocol tests. The default Cargo test runner is the missing piece; **custom harnesses fill it**.

### What we do today

| Layer | How | What it covers |
|---|---|---|
| Host unit | `cargo test --lib --target x86_64-unknown-linux-gnu` | 66 tests: clock, UI, log, motor FakeI2c, ring, ultra, wheel_map |
| On-target self-test | `clock_idle` menu **`4`** (**1**) | `selftest::run_all` over CDC |
| Probe on-chip | `./utils/run_on_target_tests.sh` (**2**) | `embedded-test` via DAPLink; **overwrites** `clock_idle` until you re-flash |

On-target “tests” that are **examples** (`led_color_set`, `ultra`, …) are bring-up, not a test report. Python GUI/PTY tests stay on the host only.

### Why not every host test is already on the chip

- **No libtest on `none`.** `#[test]` as Cargo knows it does not exist unless you replace the harness.
- **RAM/flash.** A 61-test binary plus panic strings plus FakeI2c is larger than menu `4`. The live app still has to fit beside the log region at `0x0007E000`.
- **`std`-only tests.** `serial_ui` random-sequence tests use host RNG/`String`.
- **Hardware I/O** (UARTE IRQ, EasyDMA, DAPLink DTR reset, NVMC erase) is integration, not FakeI2c unit tests.
- **Coverage dump.** llvm-cov on-chip needs counter sections, a way to export them (semihosting/RTT), and a huge instrumented image. We have not enabled that.

### Feasible ways to run more Rust unit tests on this target

Only options that work on the **micro:bit v2 nRF52833** with **this** Cargo/`thumbv7em-none-eabihf` tree and **onboard DAPLink USB** (no external JTAG, no Python on the MCU):

| # | Approach | Feasible here | Role |
|---|---|---|---|
| **1** | Expand `selftest::run_all` (menu `4`) | **Implemented** | Product binary; CDC; no extra crates. |
| **2** | [`embedded-test`](https://github.com/probe-rs/embedded-test) + `probe-rs` over **onboard CMSIS-DAP** | **Implemented** | `./utils/run_on_target_tests.sh`. Does **not** use an external ARM JTAG. Re-flash `clock_idle` after. |
| **3** | Custom UART TAP example | **Skipped (overlaps 1)** | Same `run_all` over CDC as menu `4`. |

Not listed (not fully feasible on this board/environment): external JTAG/SWD (no header); `defmt-test` + RTT (`cargo embed`/RTT failed here); QEMU/Renode (nRF52833 model incomplete); on-chip llvm-cov (no counter dump path); Python on the target.

### Is on-target unit testing more common in C/C++?

**Yes, in the sense that the C toolchain never assumed `std`.** Unity, CMock, CppUTest, GoogleTest (with a bare-metal port), and Ceedling routinely produce a firmware you flash and run; the “test runner” is just `main()` plus a UART/JTAG backend. ISO 26262 / DO-178 workflows often **require** compiling tests with the **same compiler and flags as the product**, which means on-target or instruction-set simulator.

Rust is different **only in the default runner**, not in the need:

| | Typical C/C++ embedded | Typical embedded Rust |
|---|---|---|
| Host unit tests | Optional; many teams skip and only test on HW | **Default and strongly recommended** (`no_std` crate + `#[cfg(test)]` + mocks). Fastest. |
| On-target unit/HIL | Unity/CppUTest linked into a test image; very common | **embedded-test** or **defmt-test** + probe-rs; common in 2024+ probe-rs projects, still less “automatic” than C because you must disable libtest |
| Same compiler as product | Often mandatory for certification | Same: you must build `thumbv7em-none-eabihf` tests, not only x86 |
| Coverage | gcov/lcov on host or simulator; on-chip gcov is painful | llvm-cov on **host** is the practical path |

So: **it is industry-normal to run unit tests on the MCU in both languages.** Approach **1** (menu `4`) is the floor-test path. Approach **2** is probe-rs `embedded-test`. Approach **3** was not added because it duplicates **1**.

## Next software loads (toward full HS1002)

Flash **`clock_idle` only** as the daily image. Do not flash `motor` until mapping is confirmed.

| Order | Load / change | Why | USB? |
|---|---|---|---|
| **Now** | `clock_idle` | IRQ UART, menu **8** erase flash log, menu **9** / **Button B** wheel map, menu 4 selftest | Optional |
| **A** | Confirm Motor A vs B → Left/Right | Car **on the floor**, batteries ON, USB unplugged. Button B: LED `A` then `L`/`R`/`U`, then `B` then side. Result in flash (`WheelMap`). Re-run if `U`/`N` (not level / wheels up). | No |
| **B** | Drive helpers using that map | `motor::set` with layout so “forward” means both wheels the same way. Short Button-A creep, not a drain loop. | No |
| **C** | Rest gate on every pulse | Already used in wheel map; keep it for all motion so a held car never drives. | No |
| **D** | Line sensors P12/P13 | Follow/stop on tape once wheels map is trusted. | No |
| **E** | Ultrasonic P14/P15 | Obstacle halt. | No |
| **F** | RGB / matrix status | Battery/log/map state without serial. | No |
| **G** | IR / extra kit features | Later. | — |
| **Never until G** | `examples/motor.rs` | Continuous drive; drains AAAs. |

Wheel-map **convention:** board axes, display up: `PosX`/`PosY` → Right, `NegX`/`NegY` → Left. If both sides come out `U`, the pulse did not move the chassis (wheels in the air or IMU fail `X`).

## Clock, serial, and flash log

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
- IRQ UART: RX 64-byte ring, TX 512-byte ring, 1-byte RX DMA / 16-byte TX DMA. Main loop does not poll UARTE registers.
- 5×5 shows **T** until wall time is set. A **U** flash means a UART byte was received.
- After `T=<unix>` (UTC), scrolls **`DD/MM/YYYY HH:MM:SS`** (19 characters).
- USB serial 115200. **Debug stream is ON by default** (`dbg ticks=...`). Any key that is not a `T=` line stops debug and prints the **full MENU**.
- Menu: `1` status, `2` dump, `3` start debug, `4` on-target tests, `5` LED 1–9, `6` show RTC, `7` clear RTC to 000000 msec, **`8` erase flash log**, **`9` wheel map**, `T=<10-digit unix>` set clock, `?` menu.
- **Button A:** log **count** on the LED matrix. Serial dump **only if debug logging is on**. Hold ~600 ms.
- **Button B:** same as menu `9` — IMU rest check, short Motor A then B pulses, store `WheelMap` in flash. Works with USB unplugged.
- Log region: `0x0007E000`–`0x00080000`.

### Serial GUI vs minicom vs debug

Only **one** program can own `/dev/ttyACM1`. Close the Python GUI before minicom (and the reverse). After a power cycle, wait ~2 s for CDC re-enumeration.

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

1. `clock_idle` — daily: clock, log, menu. Menu `9` is USB debug for wheel map.
2. `wheel_cal` — **floor calibration**. Button A: map, mag ~360 CW/CCW, then ~3 ft straight (`S`).
3. `line_follow` — P12/P13 idle **B/L/R/N**; Button **A** slow follow; ultrasonic P14/P15 halt &lt;12 cm (**X**); Button **B** stop. RGB/IR not used.
3. `led_color_set` — I2C RGB, motors stopped
4. `accel_motor_map` — older RTT pulse helper
5. `motor_spin` — one motor at a time
6. `motor` — last, full drive cycle; do not flash until mapping is confirmed

```bash
cargo flash --chip nRF52833_xxAA --probe 0d28:0204 --example clock_idle
cargo flash --chip nRF52833_xxAA --probe 0d28:0204 --example wheel_cal
```

Use `--probe 0d28:0204` so the micro:bit is selected, not another CMSIS-DAP.

## Library use

```toml
[dependencies]
microbit-minicar = { git = "https://github.com/wlcs15/microbit-minicar" }
```

Target: `thumbv7em-none-eabihf`.
