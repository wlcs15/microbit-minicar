#![no_std]
#![no_main]

//! Stop motors and stay idle so AAA cells are not drained by a drive loop.

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
use microbit_minicar::motor;
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

    let _ = motor::stop(&mut i2c);
    let _ = led::disable(&mut i2c);
    rprintln!("idle: motors stopped");

    loop {
        let _ = motor::stop(&mut i2c);
        timer.delay_ms(500);
    }
}
