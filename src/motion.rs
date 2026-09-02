//! Chassis motion helpers for the micro:bit v2 LSM303AGR accelerometer.
//!
//! The IMU sits on the **board**, not on the wheel axles. These checks can
//! tell whether the **body** is level/resting and whether it **moved** after a
//! motor pulse. They cannot measure wheel RPM or true axle clockwise vs
//! counter-clockwise if the wheels spin in the air.

/// Milli-g, same units as `lsm303agr` `x_mg()` / `y_mg()` / `z_mg()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MilliG {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl MilliG {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn magnitude(self) -> u32 {
        let x = self.x as i64;
        let y = self.y as i64;
        let z = self.z as i64;
        let sq = x * x + y * y + z * z;
        isqrt(sq as u64)
    }
}

fn isqrt(n: u64) -> u32 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestStatus {
    /// |a| ≈ 1 g and samples are quiet. Safe to consider a pulse.
    Ready,
    /// |a| too small — falling or sensor not reading gravity.
    Freefall,
    /// |a| far from 1 g (held at an odd attitude, or saturating).
    NotLevel,
    /// Samples already changing — do not start a motor pulse.
    AlreadyMoving,
}

/// Gravity ± this many milli-g is treated as “about 1 g”.
pub const G_TOLERANCE_MG: u32 = 250;
pub const ONE_G_MG: u32 = 1000;
/// Peak-to-peak on any axis above this means not stationary.
pub const STATIONARY_SPAN_MG: i32 = 80;
/// After a motor pulse, this delta means the chassis moved.
pub const MOVED_DELTA_MG: u32 = 60;

pub fn rest_status(samples: &[MilliG]) -> RestStatus {
    if samples.is_empty() {
        return RestStatus::NotLevel;
    }

    let last = samples[samples.len() - 1];
    let mag = last.magnitude();
    if mag < ONE_G_MG - 700 {
        return RestStatus::Freefall;
    }
    if mag.abs_diff(ONE_G_MG) > G_TOLERANCE_MG {
        return RestStatus::NotLevel;
    }

    if span(samples, |s| s.x) > STATIONARY_SPAN_MG
        || span(samples, |s| s.y) > STATIONARY_SPAN_MG
        || span(samples, |s| s.z) > STATIONARY_SPAN_MG
    {
        return RestStatus::AlreadyMoving;
    }

    RestStatus::Ready
}

fn span(samples: &[MilliG], axis: fn(MilliG) -> i32) -> i32 {
    let mut min = i32::MAX;
    let mut max = i32::MIN;
    for s in samples {
        let v = axis(*s);
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    max.saturating_sub(min)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChassisMotion {
    None,
    /// Largest change along +X (board coordinates).
    PosX,
    NegX,
    PosY,
    NegY,
}

pub fn classify_delta(before: MilliG, after: MilliG) -> (ChassisMotion, u32) {
    let dx = after.x - before.x;
    let dy = after.y - before.y;
    let dz = after.z - before.z;
    let mag = MilliG::new(dx, dy, dz).magnitude();
    if mag < MOVED_DELTA_MG {
        return (ChassisMotion::None, mag);
    }

    let ax = dx.unsigned_abs();
    let ay = dy.unsigned_abs();
    let az = dz.unsigned_abs();
    // Ignore Z for “which way did we drive”; gravity/tilt often lives there.
    let motion = if ax >= ay && ax >= az {
        if dx >= 0 {
            ChassisMotion::PosX
        } else {
            ChassisMotion::NegX
        }
    } else if ay >= az {
        if dy >= 0 {
            ChassisMotion::PosY
        } else {
            ChassisMotion::NegY
        }
    } else {
        // Dominant Z: treat as no planar drive (tilt / hop).
        ChassisMotion::None
    };

    (motion, mag)
}

/// True when a motor pulse produced planar chassis acceleration.
pub fn chassis_moved(before: MilliG, after: MilliG) -> bool {
    let (kind, _) = classify_delta(before, after);
    kind != ChassisMotion::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_of_1g_on_z() {
        assert_eq!(MilliG::new(0, 0, 1000).magnitude(), 1000);
        assert_eq!(MilliG::new(0, 0, 0).magnitude(), 0);
        assert_eq!(MilliG::new(300, 400, 0).magnitude(), 500);
    }

    #[test]
    fn empty_samples_not_level() {
        assert_eq!(rest_status(&[]), RestStatus::NotLevel);
    }

    #[test]
    fn quiet_1g_is_ready() {
        let s = [MilliG::new(10, -20, 990), MilliG::new(12, -18, 1005)];
        assert_eq!(rest_status(&s), RestStatus::Ready);
    }

    #[test]
    fn freefall_detected() {
        let s = [MilliG::new(0, 0, 50)];
        assert_eq!(rest_status(&s), RestStatus::Freefall);
    }

    #[test]
    fn odd_attitude_not_level() {
        let s = [MilliG::new(0, 0, 1600)];
        assert_eq!(rest_status(&s), RestStatus::NotLevel);
    }

    #[test]
    fn shaking_is_already_moving() {
        let s = [MilliG::new(0, 0, 1000), MilliG::new(200, 0, 1000)];
        assert_eq!(rest_status(&s), RestStatus::AlreadyMoving);
    }

    #[test]
    fn no_move_below_threshold() {
        let b = MilliG::new(0, 0, 1000);
        let a = MilliG::new(20, 10, 1000);
        assert_eq!(classify_delta(b, a).0, ChassisMotion::None);
        assert!(!chassis_moved(b, a));
    }

    #[test]
    fn pos_x_and_neg_y() {
        let b = MilliG::new(0, 0, 1000);
        assert_eq!(
            classify_delta(b, MilliG::new(200, 0, 1000)).0,
            ChassisMotion::PosX
        );
        assert_eq!(
            classify_delta(b, MilliG::new(-200, 0, 1000)).0,
            ChassisMotion::NegX
        );
        assert_eq!(
            classify_delta(b, MilliG::new(0, 180, 1000)).0,
            ChassisMotion::PosY
        );
        assert_eq!(
            classify_delta(b, MilliG::new(0, -180, 1000)).0,
            ChassisMotion::NegY
        );
        assert!(chassis_moved(b, MilliG::new(200, 0, 1000)));
    }

    #[test]
    fn dominant_z_is_not_planar_drive() {
        let b = MilliG::new(0, 0, 1000);
        assert_eq!(
            classify_delta(b, MilliG::new(0, 0, 1300)).0,
            ChassisMotion::None
        );
    }
}
