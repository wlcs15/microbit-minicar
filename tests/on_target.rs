//! Approach 2: `embedded-test` + probe-rs over onboard DAPLink (not UART menu 4).
//!
//!   cargo test --features on-target --test on_target --target thumbv7em-none-eabihf
//!
//! with runner
//! `probe-rs run --chip nRF52833_xxAA --probe 0d28:0204-5 --protocol swd --speed 100 --disable-double-buffering`.
//! Overwrites `clock_idle` until you re-flash the app.
//! On Windows DAPLink USB timeouts, unplug/replug the micro:bit then re-run.

#![no_std]
#![no_main]

use cortex_m_rt as _;
use microbit as _;

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    #[init]
    fn init() {}

    #[test]
    fn selftest_logic() {
        let r = microbit_minicar::selftest::run_all(&mut |_, ok| assert!(ok));
        assert!(r.ok());
        assert!(r.pass >= 55);
    }

    #[test]
    fn wheel_map_opposite() {
        use microbit_minicar::motion::ChassisMotion;
        use microbit_minicar::wheel_map::{infer, WheelSide};
        let m = infer(ChassisMotion::PosX, 200, ChassisMotion::NegX, 180, 60);
        assert_eq!(m.motor_a, WheelSide::Right);
        assert_eq!(m.motor_b, WheelSide::Left);
    }

    #[test]
    fn imu_policy_rejects_init_only_x_bug() {
        use microbit_minicar::hw_bus::{
            imu_ready, imu_status_glyph, TWIM1_CONFLICTS, motors_on_twim0,
        };
        assert!(imu_ready(false, false, true));
        assert!(!imu_ready(false, false, false));
        assert_eq!(imu_status_glyph(false, 0), b'0');
        assert_eq!(imu_status_glyph(true, 0), b'A');
        assert!(TWIM1_CONFLICTS.contains(&"TWI1"));
        assert!(motors_on_twim0());
    }

    /// HIL: LSM303AGR WHO_AM_I on internal I2C after disabling TWIM1 siblings.
    #[test]
    fn lsm303agr_whoami_after_twim1_unshare() {
        use lsm303agr::Lsm303agr;
        use microbit::hal::twim::{self, Twim};
        let board = microbit::Board::take().expect("board");
        board.TWI1.enable.write(|w| w.enable().disabled());
        board.SPI1.enable.write(|w| w.enable().disabled());
        board.SPIS1.enable.write(|w| w.enable().disabled());
        let i2c_int = Twim::new(
            unsafe { microbit::pac::Peripherals::steal() }.TWIM1,
            board.i2c_internal.into(),
            twim::Frequency::K100,
        );
        let mut sensor = Lsm303agr::new_with_i2c(i2c_int);
        let id = sensor.accelerometer_id().expect("whoami");
        assert!(id.is_correct());
    }
}
