#![no_std]
#![no_main]

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
use rtt_target::rtt_init_print;

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
        let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Forward);
        let _ = motor::set(&mut i2c, 200, Motor::B, Direction::Forward);
        timer.delay_ms(2_000);

        let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Backward);
        let _ = motor::set(&mut i2c, 200, Motor::B, Direction::Backward);
        timer.delay_ms(2_000);

        let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Backward);
        let _ = motor::set(&mut i2c, 200, Motor::B, Direction::Forward);
        timer.delay_ms(2_000);

        let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Forward);
        let _ = motor::set(&mut i2c, 200, Motor::B, Direction::Backward);
        timer.delay_ms(2_000);

        let _ = motor::stop(&mut i2c);
        timer.delay_ms(1_000);
    }
}
