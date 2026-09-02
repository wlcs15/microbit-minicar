//! Infer which HS1002 motor channel drives which wheel from chassis accel.
//!
//! The LSM303AGR sits on the **micro:bit**, not the axles. A short floor pulse
//! of one motor yaws/translates the body. Opposite planar signs ⇒ opposite
//! wheels. **Convention:** `PosX`/`PosY` → Right, `NegX`/`NegY` → Left, in
//! **board** axes (display up). Re-run after you confirm USB-aft vs USB-forward
//! mount; the stored log keeps the raw assignment.

use crate::motion::{ChassisMotion, MOVED_DELTA_MG};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelSide {
    Unknown = 0,
    Left = 1,
    Right = 2,
}

impl WheelSide {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Left,
            2 => Self::Right,
            _ => Self::Unknown,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn glyph(self) -> u8 {
        match self {
            Self::Left => b'L',
            Self::Right => b'R',
            Self::Unknown => b'U',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotorLayout {
    pub motor_a: WheelSide,
    pub motor_b: WheelSide,
}

impl MotorLayout {
    pub const fn unknown() -> Self {
        Self {
            motor_a: WheelSide::Unknown,
            motor_b: WheelSide::Unknown,
        }
    }

    pub fn both_known(self) -> bool {
        self.motor_a != WheelSide::Unknown && self.motor_b != WheelSide::Unknown
    }
}

pub fn side_from_motion(m: ChassisMotion) -> WheelSide {
    match m {
        ChassisMotion::PosX | ChassisMotion::PosY => WheelSide::Right,
        ChassisMotion::NegX | ChassisMotion::NegY => WheelSide::Left,
        ChassisMotion::None => WheelSide::Unknown,
    }
}

/// Map one-motor pulse results to left/right. `min_mag` defaults to
/// [`MOVED_DELTA_MG`] when 0.
pub fn infer(
    motion_a: ChassisMotion,
    mag_a: u32,
    motion_b: ChassisMotion,
    mag_b: u32,
    min_mag: u32,
) -> MotorLayout {
    let need = if min_mag == 0 {
        MOVED_DELTA_MG
    } else {
        min_mag
    };
    let motor_a = if mag_a >= need {
        side_from_motion(motion_a)
    } else {
        WheelSide::Unknown
    };
    let motor_b = if mag_b >= need {
        side_from_motion(motion_b)
    } else {
        WheelSide::Unknown
    };
    MotorLayout { motor_a, motor_b }
}

pub const PULSE_SPEED: u8 = 140;
pub const PULSE_MS: u32 = 350;
pub const SPIN_SPEED: u8 = 110;
/// Mag yaw target for a full turn (allow a little short of 360).
pub const YAW_TARGET_DEG: u16 = 330;
pub const SPIN_TIMEOUT_MS: u32 = 12_000;
/// Open-loop ~3 ft (0.9 m) at `STRAIGHT_SPEED` on a typical HS1002 floor.
pub const STRAIGHT_MS: u32 = 3_500;
pub const STRAIGHT_SPEED: u8 = 120;

/// Clockwise degrees from `from` to `to` in 0..359.
pub fn yaw_delta_cw(from: u16, to: u16) -> u16 {
    (to + 360 - from) % 360
}

pub fn yaw_delta_ccw(from: u16, to: u16) -> u16 {
    (from + 360 - to) % 360
}

/// In-place spin: left/right dirs for clockwise (`true`) or CCW.
/// Returns `(dir_a, dir_b)`. Unknown layout uses A=left, B=right.
pub fn spin_dirs(layout: MotorLayout, clockwise: bool) -> (crate::motor::Direction, crate::motor::Direction) {
    use crate::motor::Direction::{Backward, Forward};
    let a_is_left = layout.motor_a != WheelSide::Right;
    let (left, right) = if clockwise {
        (Backward, Forward)
    } else {
        (Forward, Backward)
    };
    if a_is_left {
        (left, right)
    } else {
        (right, left)
    }
}

/// `tan(0..=45°)` × 10000.
const TAN_X10000: [u16; 46] = [
    0, 175, 349, 524, 699, 875, 1051, 1228, 1405, 1584, 1763, 1944, 2126, 2309,
    2493, 2679, 2867, 3057, 3249, 3443, 3640, 3839, 4040, 4245, 4452, 4663, 4877,
    5095, 5317, 5543, 5774, 6009, 6249, 6494, 6745, 7002, 7265, 7536, 7813, 8098,
    8391, 8693, 9004, 9325, 9657, 10000,
];

fn atan_deg_q1(opp: u32, adj: u32) -> u16 {
    if adj == 0 {
        return 90;
    }
    if opp == 0 {
        return 0;
    }
    if opp == adj {
        return 45;
    }
    let (o, a, flip) = if opp > adj {
        (adj, opp, true)
    } else {
        (opp, adj, false)
    };
    let r = (u64::from(o) * 10_000) / u64::from(a);
    let mut deg = 0u16;
    for i in 0..=45 {
        if u64::from(TAN_X10000[i as usize]) <= r {
            deg = i;
        } else {
            break;
        }
    }
    if flip { 90 - deg } else { deg }
}

/// Standard math `atan2(y, x)` in 0..359. 0° = +X, 90° = +Y (CCW).
pub fn atan2_deg(y: i32, x: i32) -> u16 {
    if x == 0 && y == 0 {
        return 0;
    }
    let a = atan_deg_q1(y.unsigned_abs(), x.unsigned_abs());
    let d = match (x >= 0, y >= 0) {
        (true, true) => i32::from(a),
        (false, true) => 180 - i32::from(a),
        (false, false) => 180 + i32::from(a),
        (true, false) => 360 - i32::from(a),
    };
    ((d + 360) % 360) as u16
}

/// Board heading: 0° = +Y (LED-matrix top), 90° = +X, clockwise.
/// `None` if the planar delta is zero.
pub fn heading_deg(dx: i32, dy: i32) -> Option<u16> {
    if dx == 0 && dy == 0 {
        None
    } else {
        Some(atan2_deg(dx, dy))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinal8 {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Cardinal8 {
    pub fn from_deg(deg: u16) -> Self {
        const T: [Cardinal8; 8] = [
            Cardinal8::N,
            Cardinal8::NE,
            Cardinal8::E,
            Cardinal8::SE,
            Cardinal8::S,
            Cardinal8::SW,
            Cardinal8::W,
            Cardinal8::NW,
        ];
        T[(((u32::from(deg) + 22) % 360) / 45) as usize]
    }

    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::N => b"N",
            Self::NE => b"NE",
            Self::E => b"E",
            Self::SE => b"SE",
            Self::S => b"S",
            Self::SW => b"SW",
            Self::W => b"W",
            Self::NW => b"NW",
        }
    }
}

/// Flash payload: x = heading A (−1 none), y = heading B, z = sideA | sideB<<8.
pub fn pack_log(
    layout: MotorLayout,
    deg_a: Option<u16>,
    deg_b: Option<u16>,
) -> (i16, i16, i16) {
    let xa = deg_a.map(|d| d as i16).unwrap_or(-1);
    let yb = deg_b.map(|d| d as i16).unwrap_or(-1);
    let z = i16::from(layout.motor_a.as_u8()) | (i16::from(layout.motor_b.as_u8()) << 8);
    (xa, yb, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_pulse_is_unknown() {
        let m = infer(ChassisMotion::PosX, 10, ChassisMotion::NegX, 10, 60);
        assert_eq!(m, MotorLayout::unknown());
        assert!(!m.both_known());
    }

    #[test]
    fn opposite_x_is_right_and_left() {
        let m = infer(ChassisMotion::PosX, 200, ChassisMotion::NegX, 180, 60);
        assert_eq!(m.motor_a, WheelSide::Right);
        assert_eq!(m.motor_b, WheelSide::Left);
        assert!(m.both_known());
        assert_eq!(m.motor_a.glyph(), b'R');
        assert_eq!(m.motor_b.glyph(), b'L');
    }

    #[test]
    fn y_axis_same_rule() {
        let m = infer(ChassisMotion::NegY, 90, ChassisMotion::PosY, 90, 60);
        assert_eq!(m.motor_a, WheelSide::Left);
        assert_eq!(m.motor_b, WheelSide::Right);
    }

    #[test]
    fn from_u8_roundtrip() {
        assert_eq!(WheelSide::from_u8(1), WheelSide::Left);
        assert_eq!(WheelSide::from_u8(2).as_u8(), 2);
        assert_eq!(WheelSide::from_u8(9), WheelSide::Unknown);
    }

    #[test]
    fn heading_cardinal_axes() {
        assert_eq!(heading_deg(0, 100), Some(0));
        assert_eq!(heading_deg(100, 0), Some(90));
        assert_eq!(heading_deg(0, -100), Some(180));
        assert_eq!(heading_deg(-100, 0), Some(270));
        assert_eq!(heading_deg(0, 0), None);
        let ne = heading_deg(100, 100).unwrap();
        assert!((40..=50).contains(&ne), "{ne}");
        assert_eq!(Cardinal8::from_deg(0).as_bytes(), b"N");
        assert_eq!(Cardinal8::from_deg(45).as_bytes(), b"NE");
        assert_eq!(Cardinal8::from_deg(90).as_bytes(), b"E");
        assert_eq!(Cardinal8::from_deg(180).as_bytes(), b"S");
        assert_eq!(Cardinal8::from_deg(270).as_bytes(), b"W");
        assert_eq!(Cardinal8::from_deg(315).as_bytes(), b"NW");
    }

    #[test]
    fn pack_log_sides_and_angles() {
        let m = infer(ChassisMotion::PosX, 200, ChassisMotion::NegX, 180, 60);
        let (x, y, z) = pack_log(m, Some(90), Some(270));
        assert_eq!(x, 90);
        assert_eq!(y, 270);
        assert_eq!(z & 0xFF, i16::from(WheelSide::Right.as_u8()));
        assert_eq!((z >> 8) & 0xFF, i16::from(WheelSide::Left.as_u8()));
        let (x2, y2, _) = pack_log(MotorLayout::unknown(), None, None);
        assert_eq!(x2, -1);
        assert_eq!(y2, -1);
    }

    #[test]
    fn yaw_wraps_through_zero() {
        assert_eq!(yaw_delta_cw(350, 10), 20);
        assert_eq!(yaw_delta_ccw(10, 350), 20);
        assert_eq!(yaw_delta_cw(0, 180), 180);
        assert!(yaw_delta_cw(0, 0) < YAW_TARGET_DEG);
    }

    #[test]
    fn spin_dirs_opposite_wheels() {
        use crate::motor::Direction::{Backward, Forward};
        let m = infer(ChassisMotion::PosX, 200, ChassisMotion::NegX, 180, 60);
        let (a, b) = spin_dirs(m, true);
        assert_ne!(a, b);
        let (a2, b2) = spin_dirs(m, false);
        assert_eq!(a, b2);
        assert_eq!(b, a2);
        let _ = (Forward, Backward);
    }
}
