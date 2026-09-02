#![no_std]
#![no_main]

//! Standalone HS1002 wheel map (no USB required).
//!
//! Flash: `cargo flash --chip nRF52833_xxAA --probe 0d28:0204 --example wheel_cal`
//!
//! 1. Place the car on the floor, batteries ON, USB unplugged.
//! 2. LED `A` = waiting. Press **Button A** to run one A-then-B pulse.
//! 3. Result is stored in the flash log, motors stop. Press **A** again to repeat
//!    (does not auto-run). **Button B** toggles 8-point (N/NE/…) vs 0–359°.
//!
//! Headings are **board** axes (LED-matrix top = 0°), not geographic north.
//! Magnetometer true-north is a later pass (LSM303AGR mag is on the v2).

use core::fmt::Write;
use cortex_m_rt::entry;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::InputPin;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use lsm303agr::mode::MagOneShot;
use lsm303agr::{AccelMode, AccelOutputDataRate, Lsm303agr, MagMode, MagOutputDataRate};
use microbit::{
    display::blocking::Display,
    hal::{
        twim::{self, Twim},
        uarte::{Baudrate, Parity},
        Timer,
    },
    pac::interrupt,
    Board,
};
use microbit_minicar::hw_bus;
use microbit_minicar::led;
use microbit_minicar::log_store::{
    append, EventKind, LogError, LogRecord, Seq, PAGE_SIZE,
};
use microbit_minicar::motion::{self, MilliG, RestStatus};
use microbit_minicar::motor::{self, Direction, Motor};
use microbit_minicar::wheel_map::{self, Cardinal8, MotorLayout};
use panic_halt as _;

#[path = "uart_irq.rs"]
mod uart_irq;

const LOG_LEN: usize = 2 * PAGE_SIZE;

fn log_storage() -> &'static mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(0x0007_E000 as *mut u8, LOG_LEN) }
}

fn persist(
    nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>,
    rec: &LogRecord,
) {
    let mut ram = [0xFFu8; LOG_LEN];
    let _ = nvmc.read(0, &mut ram);
    match append(&mut ram, rec) {
        Ok(_) => {}
        Err(LogError::Full) => {
            ram.fill(0xFF);
            let _ = append(&mut ram, rec);
        }
    }
    let _ = nvmc.erase(0, LOG_LEN as u32);
    let _ = nvmc.write(0, &ram);
}

fn glyph(c: u8) -> [u8; 5] {
    match c {
        b'0' => [0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        b'1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b01110],
        b'2' => [0b01110, 0b10001, 0b00010, 0b00100, 0b11111],
        b'3' => [0b11110, 0b00001, 0b01110, 0b00001, 0b11110],
        b'4' => [0b10010, 0b10010, 0b11111, 0b00010, 0b00010],
        b'5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b11110],
        b'6' => [0b01110, 0b10000, 0b11110, 0b10001, 0b01110],
        b'7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b00100],
        b'8' => [0b01110, 0b10001, 0b01110, 0b10001, 0b01110],
        b'9' => [0b01110, 0b10001, 0b01111, 0b00001, 0b01110],
        b'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        b'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        b'R' => [0b11110, 0b10001, 0b11110, 0b10100, 0b10010],
        b'A' => [0b01110, 0b10001, 0b11111, 0b10001, 0b10001],
        b'B' => [0b11110, 0b10001, 0b11110, 0b10001, 0b11110],
        b'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001],
        b'E' => [0b11111, 0b10000, 0b11110, 0b10000, 0b11111],
        b'S' => [0b01111, 0b10000, 0b01110, 0b00001, 0b11110],
        b'W' => [0b10001, 0b10001, 0b10101, 0b10101, 0b01110],
        b'X' => [0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        b' ' => [0, 0, 0, 0, 0],
        _ => [0b11111, 0b10001, 0b10001, 0b10001, 0b11111],
    }
}

fn show_glyph<D: DelayNs>(display: &mut Display, delay: &mut D, c: u8, ms: u32) {
    let g = glyph(c);
    let mut frame = [[0u8; 5]; 5];
    for y in 0..5 {
        for x in 0..5 {
            frame[y][x] = u8::from((g[y] >> (4 - x)) & 1 == 1);
        }
    }
    display.show(delay, frame, ms);
}

fn scroll_text<D: DelayNs>(display: &mut Display, delay: &mut D, text: &[u8]) {
    let frames = text.len() * 6 + 5;
    for origin in 0..frames {
        let mut frame = [[0u8; 5]; 5];
        for x in 0..5 {
            let col = origin + x;
            let ch_i = col / 6;
            let bit = 4usize.saturating_sub(col % 6);
            if ch_i < text.len() && col % 6 < 5 {
                let g = glyph(text[ch_i]);
                for y in 0..5 {
                    frame[y][x] = u8::from((g[y] >> bit) & 1 == 1);
                }
            }
        }
        display.show(delay, frame, 80);
    }
    display.clear();
}

fn show_cardinal<D: DelayNs>(display: &mut Display, delay: &mut D, c: Cardinal8) {
    for b in c.as_bytes() {
        show_glyph(display, delay, *b, 280);
    }
}

fn show_deg<D: DelayNs>(display: &mut Display, delay: &mut D, deg: u16) {
    let mut buf = [b'0'; 3];
    let n = u32::from(deg % 360);
    buf[0] = b'0' + ((n / 100) % 10) as u8;
    buf[1] = b'0' + ((n / 10) % 10) as u8;
    buf[2] = b'0' + (n % 10) as u8;
    scroll_text(display, delay, &buf);
}

#[derive(Clone, Copy)]
enum Mode {
    Cardinal,
    Degrees,
}

struct PulseView {
    layout: MotorLayout,
    deg_a: Option<u16>,
    deg_b: Option<u16>,
}

fn show_result<D: DelayNs>(
    display: &mut Display,
    delay: &mut D,
    view: &PulseView,
    mode: Mode,
) {
    show_glyph(display, delay, b'A', 200);
    match (mode, view.deg_a) {
        (Mode::Cardinal, Some(d)) => show_cardinal(display, delay, Cardinal8::from_deg(d)),
        (Mode::Degrees, Some(d)) => show_deg(display, delay, d),
        _ => show_glyph(display, delay, b'U', 300),
    }
    show_glyph(display, delay, view.layout.motor_a.glyph(), 400);
    show_glyph(display, delay, b'B', 200);
    match (mode, view.deg_b) {
        (Mode::Cardinal, Some(d)) => show_cardinal(display, delay, Cardinal8::from_deg(d)),
        (Mode::Degrees, Some(d)) => show_deg(display, delay, d),
        _ => show_glyph(display, delay, b'U', 300),
    }
    show_glyph(display, delay, view.layout.motor_b.glyph(), 400);
}

fn read_mg<I2C, E>(
    sensor: &mut Lsm303agr<lsm303agr::interface::I2cInterface<I2C>, MagOneShot>,
) -> Option<MilliG>
where
    I2C: embedded_hal::i2c::I2c<Error = E>,
{
    if !sensor.accel_status().ok()?.xyz_new_data() {
        return None;
    }
    let a = sensor.acceleration().ok()?;
    Some(MilliG::new(a.x_mg(), a.y_mg(), a.z_mg()))
}

fn sample_rest<I2C, E, D>(
    sensor: &mut Lsm303agr<lsm303agr::interface::I2cInterface<I2C>, MagOneShot>,
    delay: &mut D,
) -> RestStatus
where
    I2C: embedded_hal::i2c::I2c<Error = E>,
    D: DelayNs,
{
    let mut buf = [MilliG::new(0, 0, 0); 8];
    let mut n = 0;
    for _ in 0..40 {
        if let Some(s) = read_mg(sensor) {
            if n < buf.len() {
                buf[n] = s;
                n += 1;
            } else {
                buf.copy_within(1.., 0);
                buf[7] = s;
            }
        }
        delay.delay_ms(20);
    }
    motion::rest_status(&buf[..n.min(8)])
}

fn wait_sample<I2C, E, D>(
    sensor: &mut Lsm303agr<lsm303agr::interface::I2cInterface<I2C>, MagOneShot>,
    delay: &mut D,
) -> MilliG
where
    I2C: embedded_hal::i2c::I2c<Error = E>,
    D: DelayNs,
{
    for _ in 0..50 {
        if let Some(s) = read_mg(sensor) {
            return s;
        }
        delay.delay_ms(10);
    }
    MilliG::new(0, 0, 0)
}

fn read_mag_heading<I2C, E, D>(
    sensor: &mut Lsm303agr<lsm303agr::interface::I2cInterface<I2C>, MagOneShot>,
    delay: &mut D,
) -> Option<u16>
where
    I2C: embedded_hal::i2c::I2c<Error = E>,
    D: DelayNs,
{
    for _ in 0..30 {
        if let Ok(f) = sensor.magnetic_field() {
            return wheel_map::heading_deg(f.x_nt(), f.y_nt());
        }
        delay.delay_ms(15);
    }
    None
}

fn yaw_spin<I2C, I2Ci, E, Ei, D>(
    i2c: &mut I2C,
    sensor: &mut Lsm303agr<lsm303agr::interface::I2cInterface<I2Ci>, MagOneShot>,
    delay: &mut D,
    layout: MotorLayout,
    clockwise: bool,
) -> u16
where
    I2C: embedded_hal::i2c::I2c<Error = E>,
    I2Ci: embedded_hal::i2c::I2c<Error = Ei>,
    D: DelayNs,
{
    let start = read_mag_heading(sensor, delay);
    let (da, db) = wheel_map::spin_dirs(layout, clockwise);
    let _ = motor::set(i2c, wheel_map::SPIN_SPEED, Motor::A, da);
    let _ = motor::set(i2c, wheel_map::SPIN_SPEED, Motor::B, db);
    let mut best = 0u16;
    let mut t = 0u32;
    while t < wheel_map::SPIN_TIMEOUT_MS {
        delay.delay_ms(50);
        t += 50;
        if let Some(h) = read_mag_heading(sensor, delay) {
            if let Some(s) = start {
                let d = if clockwise {
                    wheel_map::yaw_delta_cw(s, h)
                } else {
                    wheel_map::yaw_delta_ccw(s, h)
                };
                if d > best {
                    best = d;
                }
                if d >= wheel_map::YAW_TARGET_DEG {
                    break;
                }
            }
        } else if t >= 4000 {
            break;
        }
    }
    let _ = motor::stop(i2c);
    best
}

fn wait_a_released<Btn, D>(btn: &mut Btn, delay: &mut D)
where
    Btn: InputPin,
    D: DelayNs,
{
    for _ in 0..50 {
        if btn.is_high().unwrap_or(true) {
            break;
        }
        delay.delay_ms(20);
    }
}

#[entry]
fn main() -> ! {
    let board = Board::take().unwrap();
    let mut timer = Timer::new(board.TIMER0);
    let mut display = Display::new(board.display_pins);
    uart_irq::init(
        board.UARTE0,
        board.uart.into(),
        Baudrate::BAUD115200,
        Parity::EXCLUDED,
    );
    let mut nvmc = microbit::hal::nvmc::Nvmc::new(board.NVMC, log_storage());
    board.TWI1.enable.write(|w| w.enable().disabled());
    board.SPI1.enable.write(|w| w.enable().disabled());
    board.SPIS1.enable.write(|w| w.enable().disabled());
    let mut i2c = Twim::new(
        board.TWIM0,
        board.i2c_external.into(),
        twim::Frequency::K100,
    );
    let i2c_int = Twim::new(
        unsafe { microbit::pac::Peripherals::steal() }.TWIM1,
        board.i2c_internal.into(),
        twim::Frequency::K100,
    );
    let mut button_a = board.buttons.button_a.into_floating_input();
    let mut button_b = board.buttons.button_b.into_floating_input();

    let _ = motor::stop(&mut i2c);
    let _ = led::disable(&mut i2c);

    timer.delay_ms(20);
    let mut sensor = Lsm303agr::new_with_i2c(i2c_int);
    let id_ok = sensor
        .accelerometer_id()
        .map(|id| id.is_correct())
        .unwrap_or(false);
    let init_ok = sensor.init().is_ok();
    let odr_ok = sensor
        .set_accel_mode_and_odr(&mut timer, AccelMode::Normal, AccelOutputDataRate::Hz50)
        .is_ok();
    let imu_ok = hw_bus::imu_ready(init_ok, odr_ok, id_ok);
    let imu_code = hw_bus::imu_status_code(init_ok, odr_ok, id_ok);
    show_glyph(
        &mut display,
        &mut timer,
        hw_bus::imu_status_glyph(imu_ok, imu_code),
        400,
    );

    let mut seq = Seq::new();
    let mut mode = Mode::Cardinal;
    let mut last: Option<PulseView> = None;
    let _ = uart_irq::write_str("wheel_cal\r\n");
    let mut w = uart_irq::writer();
    if !imu_ok {
        persist(
            &mut nvmc,
            &seq.emit(
                EventKind::Note,
                0,
                0,
                i32::from(init_ok),
                i32::from(odr_ok),
                i32::from(id_ok),
            ),
        );
    }

    loop {
        let _ = motor::stop(&mut i2c);
        if !imu_ok {
            let _ = write!(
                w,
                "IMU fail init={} odr={} id={}\r\n",
                init_ok as u8, odr_ok as u8, id_ok as u8
            );
            show_glyph(
                &mut display,
                &mut timer,
                hw_bus::imu_status_glyph(false, imu_code),
                400,
            );
            continue;
        }

        if button_b.is_low().unwrap_or(false) {
            mode = match mode {
                Mode::Cardinal => Mode::Degrees,
                Mode::Degrees => Mode::Cardinal,
            };
            wait_a_released(&mut button_b, &mut timer);
            if let Some(v) = last.as_ref() {
                show_result(&mut display, &mut timer, v, mode);
            }
        }

        if last.is_none() {
            show_glyph(&mut display, &mut timer, b'A', 80);
        } else if button_a.is_high().unwrap_or(true) && button_b.is_high().unwrap_or(true) {
            if let Some(v) = last.as_ref() {
                show_glyph(&mut display, &mut timer, v.layout.motor_a.glyph(), 60);
            }
        }

        if !button_a.is_low().unwrap_or(false) {
            continue;
        }
        wait_a_released(&mut button_a, &mut timer);

        show_glyph(&mut display, &mut timer, b'A', 200);
        match sample_rest(&mut sensor, &mut timer) {
            RestStatus::Ready => {}
            other => {
                let _ = write!(w, "not ready {:?}\r\n", other);
                persist(
                    &mut nvmc,
                    &seq.emit(EventKind::RestBlocked, 0, 0, 0, 0, 0),
                );
                show_glyph(&mut display, &mut timer, b'N', 600);
                continue;
            }
        }

        let before_a = wait_sample(&mut sensor, &mut timer);
        let _ = motor::set(&mut i2c, wheel_map::PULSE_SPEED, Motor::A, Direction::Forward);
        timer.delay_ms(wheel_map::PULSE_MS);
        let _ = motor::stop(&mut i2c);
        timer.delay_ms(50);
        let after_a = wait_sample(&mut sensor, &mut timer);
        let (kind_a, mag_a) = motion::classify_delta(before_a, after_a);
        let deg_a = wheel_map::heading_deg(after_a.x - before_a.x, after_a.y - before_a.y);
        let _ = motor::stop(&mut i2c);
        timer.delay_ms(400);

        if sample_rest(&mut sensor, &mut timer) != RestStatus::Ready {
            let _ = write!(w, "not ready for B\r\n");
            show_glyph(&mut display, &mut timer, b'N', 600);
            continue;
        }
        let before_b = wait_sample(&mut sensor, &mut timer);
        let _ = motor::set(&mut i2c, wheel_map::PULSE_SPEED, Motor::B, Direction::Forward);
        timer.delay_ms(wheel_map::PULSE_MS);
        let _ = motor::stop(&mut i2c);
        timer.delay_ms(50);
        let after_b = wait_sample(&mut sensor, &mut timer);
        let (kind_b, mag_b) = motion::classify_delta(before_b, after_b);
        let deg_b = wheel_map::heading_deg(after_b.x - before_b.x, after_b.y - before_b.y);
        let _ = motor::stop(&mut i2c);

        let layout = wheel_map::infer(kind_a, mag_a, kind_b, mag_b, 0);
        let (px, py, pz) = wheel_map::pack_log(layout, deg_a, deg_b);
        persist(
            &mut nvmc,
            &seq.emit(EventKind::WheelMap, 0, 0, i32::from(px), i32::from(py), i32::from(pz)),
        );
        let _ = sensor.set_mag_mode_and_odr(
            &mut timer,
            MagMode::HighResolution,
            MagOutputDataRate::Hz50,
        );
        show_glyph(&mut display, &mut timer, b'C', 200);
        let cw = yaw_spin(&mut i2c, &mut sensor, &mut timer, layout, true);
        timer.delay_ms(500);
        show_glyph(&mut display, &mut timer, b'3', 200);
        let ccw = yaw_spin(&mut i2c, &mut sensor, &mut timer, layout, false);
        persist(
            &mut nvmc,
            &seq.emit(
                EventKind::Yaw360,
                0,
                0,
                i32::from(cw),
                i32::from(ccw),
                i32::from(
                    cw >= wheel_map::YAW_TARGET_DEG && ccw >= wheel_map::YAW_TARGET_DEG,
                ),
            ),
        );
        let _ = write!(
            w,
            "WHEEL A={:?} deg={:?} B={:?} deg={:?} La={:?} Lb={:?} yaw_cw={} ccw={}\r\n",
            kind_a, deg_a, kind_b, deg_b, layout.motor_a, layout.motor_b, cw, ccw
        );
        let view = PulseView {
            layout,
            deg_a,
            deg_b,
        };
        show_result(&mut display, &mut timer, &view, mode);
        last = Some(view);
    }
}

#[interrupt]
fn UARTE0_UART0() {
    uart_irq::on_irq();
}
