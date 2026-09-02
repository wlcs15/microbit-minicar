#![no_std]
#![no_main]

//! Line sensors + slow follow + ultrasonic halt (no RGB/IR).
//!
//! Flash: `cargo flash --chip nRF52833_xxAA --probe 0d28:0204 --example line_follow`
//!
//! 1-foot black tape is enough: idle shows **B/L/R/N** from P12/P13.
//! Button **A** creeps along the tape; **None** or ultrasonic &lt; 12 cm stops.
//! Button **B** stops. LED **X** = obstacle.

use cortex_m_rt::entry;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::InputPin;
use microbit::{
    display::blocking::Display,
    hal::{
        gpio::Level,
        twim::{self, Twim},
        Timer,
    },
    Board,
};
use microbit_minicar::led;
use microbit_minicar::line_tracking::{self, FollowCmd, LineTrackingSensor};
use microbit_minicar::motor::{self, Direction, Motor};
use microbit_minicar::ultra;
use panic_halt as _;

const FOLLOW_SPEED: u8 = 70;
const STEER_SLOW: u8 = 25;
const STOP_CM: u32 = 12;

fn glyph(c: u8) -> [u8; 5] {
    match c {
        b'B' => [0b11110, 0b10001, 0b11110, 0b10001, 0b11110],
        b'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        b'R' => [0b11110, 0b10001, 0b11110, 0b10100, 0b10010],
        b'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001],
        b'X' => [0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        b'A' => [0b01110, 0b10001, 0b11111, 0b10001, 0b10001],
        _ => [0, 0, 0, 0, 0],
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

fn line_glyph(s: LineTrackingSensor) -> u8 {
    match s {
        LineTrackingSensor::Both => b'B',
        LineTrackingSensor::Left => b'L',
        LineTrackingSensor::Right => b'R',
        LineTrackingSensor::None => b'N',
    }
}

fn apply_drive<I2C: embedded_hal::i2c::I2c>(i2c: &mut I2C, cmd: FollowCmd) {
    match cmd {
        FollowCmd::Stop => {
            let _ = motor::stop(i2c);
        }
        FollowCmd::Forward => {
            let _ = motor::set(i2c, FOLLOW_SPEED, Motor::A, Direction::Forward);
            let _ = motor::set(i2c, FOLLOW_SPEED, Motor::B, Direction::Forward);
        }
        FollowCmd::SteerLeft => {
            let _ = motor::set(i2c, STEER_SLOW, Motor::A, Direction::Forward);
            let _ = motor::set(i2c, FOLLOW_SPEED, Motor::B, Direction::Forward);
        }
        FollowCmd::SteerRight => {
            let _ = motor::set(i2c, FOLLOW_SPEED, Motor::A, Direction::Forward);
            let _ = motor::set(i2c, STEER_SLOW, Motor::B, Direction::Forward);
        }
    }
}

#[entry]
fn main() -> ! {
    let board = Board::take().unwrap();
    let mut timer = Timer::new(board.TIMER0);
    let mut clock = Timer::new(board.TIMER1);
    clock.start(u32::MAX);
    let mut display = Display::new(board.display_pins);
    let mut i2c = Twim::new(
        board.TWIM0,
        board.i2c_external.into(),
        twim::Frequency::K100,
    );
    let mut button_a = board.buttons.button_a.into_floating_input();
    let mut button_b = board.buttons.button_b.into_floating_input();
    let mut left = board.edge.e12.into_pullup_input();
    let mut right = board.pins.p0_17.into_pullup_input();
    let mut trig = board.pins.p0_01.into_push_pull_output(Level::Low);
    let mut echo = board.pins.p0_13.into_floating_input();

    let _ = motor::stop(&mut i2c);
    let _ = led::disable(&mut i2c);

    let mut following = false;
    loop {
        if button_b.is_low().unwrap_or(false) {
            following = false;
            let _ = motor::stop(&mut i2c);
        }
        if button_a.is_low().unwrap_or(false) {
            following = true;
        }

        let line = line_tracking::read(&mut left, &mut right)
            .unwrap_or(LineTrackingSensor::None);
        let dist = ultra::measure_cm(&mut trig, &mut echo, &mut timer, &mut || clock.read());
        let obstacle = matches!(dist, Ok(Some(cm)) if cm < STOP_CM);

        if following {
            let cmd = line_tracking::follow_cmd(line, obstacle);
            apply_drive(&mut i2c, cmd);
            if cmd == FollowCmd::Stop {
                following = false;
            }
            show_glyph(
                &mut display,
                &mut timer,
                if obstacle { b'X' } else { line_glyph(line) },
                40,
            );
        } else {
            let _ = motor::stop(&mut i2c);
            show_glyph(&mut display, &mut timer, line_glyph(line), 80);
        }
    }
}
