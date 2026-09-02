#![no_std]
#![no_main]

//! On-device chassis motion check (do not flash until asked).
//!
//! 1. Sample the LSM303AGR at rest. Abort if not ~1 g or already moving.
//! 2. Pulse motor A, then motor B, only if rest looked safe.
//! 3. Log planar delta over RTT. That infers **body** motion, not axle RPM.

use cortex_m_rt::entry;
use embedded_hal::delay::DelayNs;
use lsm303agr::mode::MagOneShot;
use lsm303agr::{AccelMode, AccelOutputDataRate, Lsm303agr};
use microbit::{
    board::Board,
    hal::{
        Timer,
        twim::{self, Twim},
    },
};
use microbit_minicar::led;
use microbit_minicar::motion::{self, MilliG, RestStatus};
use microbit_minicar::motor::{self, Direction, Motor};
use panic_halt as _;
use rtt_target::{rprintln, rtt_init_print};

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

#[entry]
fn main() -> ! {
    rtt_init_print!();

    let board = Board::take().unwrap();
    let mut timer = Timer::new(board.TIMER0);
    let mut i2c_ext = Twim::new(
        board.TWIM0,
        board.i2c_external.into(),
        twim::Frequency::K100,
    );
    let i2c_int = Twim::new(
        board.TWIM1,
        board.i2c_internal.into(),
        twim::Frequency::K100,
    );

    let _ = led::disable(&mut i2c_ext);
    let _ = motor::stop(&mut i2c_ext);

    let mut sensor = Lsm303agr::new_with_i2c(i2c_int);
    if sensor.init().is_err() {
        rprintln!("accel init failed — not pulsing motors");
        loop {
            timer.delay_ms(1000);
        }
    }
    let _ = sensor.set_accel_mode_and_odr(&mut timer, AccelMode::Normal, AccelOutputDataRate::Hz50);

    loop {
        rprintln!("checking rest / free-to-move (do not hold the car)");
        match sample_rest(&mut sensor, &mut timer) {
            RestStatus::Ready => rprintln!("rest OK"),
            other => {
                rprintln!("NOT free/ready: {:?} — motors stay off", other);
                timer.delay_ms(2000);
                continue;
            }
        }

        let before = wait_sample(&mut sensor, &mut timer);

        rprintln!("pulse MOTOR A");
        let _ = motor::set(&mut i2c_ext, 150, Motor::A, Direction::Forward);
        timer.delay_ms(400);
        let _ = motor::stop(&mut i2c_ext);
        timer.delay_ms(50);
        let after_a = wait_sample(&mut sensor, &mut timer);
        let (kind_a, mag_a) = motion::classify_delta(before, after_a);
        if !motion::chassis_moved(before, after_a) {
            rprintln!(
                "MOTOR A produced no chassis accel ({}) — held or wheels up?",
                mag_a
            );
        } else {
            rprintln!("MOTOR A chassis {:?}", kind_a);
        }

        timer.delay_ms(800);
        match sample_rest(&mut sensor, &mut timer) {
            RestStatus::Ready => {}
            other => {
                rprintln!("not ready for motor B: {:?}", other);
                timer.delay_ms(2000);
                continue;
            }
        }
        let before_b = wait_sample(&mut sensor, &mut timer);
        rprintln!("pulse MOTOR B");
        let _ = motor::set(&mut i2c_ext, 150, Motor::B, Direction::Forward);
        timer.delay_ms(400);
        let _ = motor::stop(&mut i2c_ext);
        timer.delay_ms(50);
        let after_b = wait_sample(&mut sensor, &mut timer);
        let (kind_b, mag_b) = motion::classify_delta(before_b, after_b);
        if !motion::chassis_moved(before_b, after_b) {
            rprintln!("MOTOR B produced no chassis accel ({})", mag_b);
        } else {
            rprintln!("MOTOR B chassis {:?}", kind_b);
        }

        timer.delay_ms(3000);
    }
}
