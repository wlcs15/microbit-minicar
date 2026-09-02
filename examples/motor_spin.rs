#![no_std]
#![no_main]

//! Drive motor A then motor B so wheel rotation can be reported over RTT.
//! Watch with `probe-rs` / `cargo embed --example motor_spin` and note
//! clockwise vs counter-clockwise for each wheel.

use cortex_m_rt::entry;
use embedded_hal::delay::DelayNs;
use microbit::{
    board::Board,
    hal::{
        Timer,
        twim::{self, Twim},
    },
};
use microbit_minicar::led;
use microbit_minicar::motor::{self, Direction, Motor};
use panic_halt as _;
use rtt_target::{rprintln, rtt_init_print};

#[entry]
fn main() -> ! {
    rtt_init_print!();

    let board = Board::take().unwrap();
    let mut timer = Timer::new(board.TIMER0);
    let mut i2c = Twim::new(
        board.TWIM0,
        board.i2c_external.into(),
        twim::Frequency::K100,
    );

    let _ = led::disable(&mut i2c);
    let _ = motor::stop(&mut i2c);

    loop {
        rprintln!("MOTOR A forward 2s — report CW or CCW");
        let _ = motor::set(&mut i2c, 0, Motor::B, Direction::Forward);
        let _ = motor::set(&mut i2c, 150, Motor::A, Direction::Forward);
        timer.delay_ms(2_000);
        let _ = motor::stop(&mut i2c);
        timer.delay_ms(1_000);

        rprintln!("MOTOR B forward 2s — report CW or CCW");
        let _ = motor::set(&mut i2c, 0, Motor::A, Direction::Forward);
        let _ = motor::set(&mut i2c, 150, Motor::B, Direction::Forward);
        timer.delay_ms(2_000);
        let _ = motor::stop(&mut i2c);
        timer.delay_ms(2_000);
    }
}
