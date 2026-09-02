# microbit-minicar

Small Rust library and example project for the **HolaSmart HS1002** car with a BBC micro:bit v2.

This tree is the wlcs15 port (`https://github.com/wlcs15/microbit-minicar`). Keyestudio MiniCar motor encoding is not kept.

## What this crate gives you

- motor control (HS1002 PWM expander at I2C `0x30`)
- RGB LED control
- line tracking sensor reading
- ultrasonic distance measurement
- software wall clock (`T=<unix>` to set) and a flash-backed calibration log

The reusable library lives in `src/`. Board wiring for the micro:bit stays in `examples/`.

## Clock and flash log

The nRF52833 has **RTC peripherals** (low-frequency counter) but **no battery-backed calendar**. Set time with `T=<unix-seconds>` over serial/RTT at the start of a powered session.

| Power | Clock | Flash log |
|---|---|---|
| USB or car batteries feeding the micro:bit | Runs | Readable/writable |
| Batteries removed and drive switched off (board unpowered) | **Stops, time lost** | **Kept** (on-chip flash) |
| Reset with power still applied | RTC counter restarts at 0; set time again | Kept |

Storing the same events in flash as well as RTT **does improve floor tests**: unplug USB, drive, plug back in, dump the log. Each record stores unix time *at write* if the clock was set, so timestamps survive power-off even though the live clock does not.

Log pages: `0x0007E000`–`0x00080000` (last 8 KiB). Firmware flash is limited to 504 KiB in `memory.x`.

## Use it as a library

```toml
[dependencies]
microbit-minicar = { git = "https://github.com/wlcs15/microbit-minicar" }
```

```rust
use microbit_minicar::led::{self, LedColor, LedRgb};
use microbit_minicar::motor::{self, Direction, Motor};

fn demo<I2C>(i2c: &mut I2C) -> Result<(), I2C::Error>
where
    I2C: embedded_hal::i2c::I2c,
{
    motor::set(i2c, 90, Motor::A, Direction::Forward)?;
    led::set_color(i2c, LedRgb::Led1, LedColor::Green)?;
    Ok(())
}
```

## Examples

- `led_color_set`: cycles through the LED colors
- `motor`: forward, back, left, right
- `motor_spin`: one motor at a time (manual CW / CCW over RTT)
- `accel_motor_map`: rest/free-to-move check then pulse each motor; logs chassis accel (not axle RPM)
- `line_tracking`: reads the line sensors and prints the state over RTT
- `ultra`: measures distance and changes the LED color based on the result

Do not flash these until host tests and coverage pass.

## Host tests and quality gates

Default Cargo target is `thumbv7em-none-eabihf`. Host tests override it:

```bash
./utils/run_host_tests.sh
./utils/run_coverage.sh    # 95% lines on this crate's src/
./utils/run_clippy.sh      # Clippy is the complexity gate
cargo fmt --check
```

## Target

```text
thumbv7em-none-eabihf
```

```bash
rustup target add thumbv7em-none-eabihf
```

## Flashing

```bash
cargo embed --example motor
```

Many examples print debug output over RTT.
