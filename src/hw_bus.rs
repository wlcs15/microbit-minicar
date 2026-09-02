//! Bus and IMU bring-up rules for the micro:bit v2 + HS1002.
//!
//! Bugs these encode (seen on hardware):
//! - Treating `Lsm303agr::init()` alone as IMU-ready (mag NACK ⇒ perpetual `X`,
//!   Button A/B ignored, no motor pulse).
//! - Enabling TWIM1 while TWI1/SPI1/SPIS1 still own the same nRF ENABLE block.
//! - Moving the motor expander off TWIM0 (the proven HS1002 bus) onto TWIM1.
//! - Blocking on UART TX flush before the first LED so a silent CDC looks like
//!   a dead board.

/// `init()` talks to the magnetometer as well as the accelerometer.
/// Accel WHO_AM_I or ODR success is enough to run wheel mapping.
pub const fn imu_ready(init_ok: bool, odr_ok: bool, id_ok: bool) -> bool {
    id_ok || odr_ok || init_ok
}

/// Bitmask shown as `0`–`7` when IMU is not ready: init=1, odr=2, id=4.
pub const fn imu_status_code(init_ok: bool, odr_ok: bool, id_ok: bool) -> u8 {
    (if init_ok { 1 } else { 0 })
        + (if odr_ok { 2 } else { 0 })
        + (if id_ok { 4 } else { 0 })
}

pub const fn imu_status_glyph(ready: bool, code: u8) -> u8 {
    if ready {
        b'A'
    } else {
        let c = if code > 7 { 7 } else { code };
        b'0' + c
    }
}

/// nRF52833 TWIM1 is the same ID as these peripherals; disable them first.
pub const TWIM1_CONFLICTS: &[&str] = &["TWI1", "SPI1", "SPIS1"];

pub const fn motors_on_twim0() -> bool {
    true
}

pub const fn imu_on_twim1_internal() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mag_init_fail_still_ready_if_whoami_ok() {
        assert!(imu_ready(false, false, true));
        assert!(imu_ready(false, true, false));
        assert!(!imu_ready(false, false, false));
        assert_eq!(imu_status_glyph(true, 0), b'A');
        assert_eq!(imu_status_glyph(false, imu_status_code(false, false, false)), b'0');
        assert_eq!(imu_status_glyph(false, imu_status_code(false, false, true)), b'4');
        assert_eq!(imu_status_glyph(false, imu_status_code(true, true, true)), b'7');
    }

    #[test]
    fn old_init_only_check_would_hide_working_accel() {
        let mag_nack = false;
        let accel_id_ok = true;
        assert!(!mag_nack, "old firmware used only init_ok");
        assert!(imu_ready(mag_nack, false, accel_id_ok));
        assert!(!mag_nack && accel_id_ok);
    }

    #[test]
    fn twim1_conflicts_named() {
        assert!(TWIM1_CONFLICTS.contains(&"TWI1"));
        assert!(TWIM1_CONFLICTS.contains(&"SPI1"));
        assert!(TWIM1_CONFLICTS.contains(&"SPIS1"));
        assert_eq!(TWIM1_CONFLICTS.len(), 3);
        assert!(motors_on_twim0());
        assert!(imu_on_twim1_internal());
    }
}
