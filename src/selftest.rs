//! Same checks on the host (`cargo test`) and on the target (menu `4`).

use core::convert::Infallible;

use embedded_hal::digital::{ErrorType as PinError, InputPin};
use embedded_hal::i2c::{ErrorType as I2cError, I2c, Operation, SevenBitAddress};

use crate::bus::{write_reg, I2C_ADDR};
use crate::clock::{
    format_mmddyyyy_hhmmss, format_msec6, parse_set_command, WallClock,
};
use crate::led::{color_to_pwm, LedColor};
use crate::line_tracking::{self, LineTrackingSensor};
use crate::log_store::{
    append, clear, decode, encode, latest_wheel_map, next_slot, EventKind, LogRecord, PAGE_SIZE,
    RECORD_SIZE,
};
use crate::motion::{self, ChassisMotion, MilliG, RestStatus};
use crate::motor::{self, Direction, Motor};
use crate::ring::Ring;
use crate::serial_ui::{button_a_prints_log, Cmd, SerialUi};
use crate::hw_bus;
use crate::ultra;
use crate::wheel_map::{self, WheelSide};

pub struct Report {
    pub pass: u16,
    pub fail: u16,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.fail == 0
    }
}

pub fn check_eq(rep: &mut Report, name: &str, ok: bool, out: &mut impl FnMut(&str, bool)) {
    if ok {
        rep.pass = rep.pass.saturating_add(1);
    } else {
        rep.fail = rep.fail.saturating_add(1);
    }
    out(name, ok);
}

struct FakeI2c {
    last: [u8; 2],
}

impl I2cError for FakeI2c {
    type Error = Infallible;
}

impl I2c<SevenBitAddress> for FakeI2c {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        if address != I2C_ADDR {
            return Ok(());
        }
        for op in operations {
            if let Operation::Write(bytes) = op {
                if bytes.len() >= 2 {
                    self.last = [bytes[0], bytes[1]];
                }
            }
        }
        Ok(())
    }
}

struct StubPin {
    high: bool,
}

impl PinError for StubPin {
    type Error = Infallible;
}

impl InputPin for StubPin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.high)
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.high)
    }
}

pub fn run_all(out: &mut impl FnMut(&str, bool)) -> Report {
    let mut rep = Report { pass: 0, fail: 0 };

    check_eq(
        &mut rep,
        "fmt_epoch",
        &format_mmddyyyy_hhmmss(0) == b"01/01/1970 00:00:00",
        out,
    );
    check_eq(
        &mut rep,
        "fmt_known",
        &format_mmddyyyy_hhmmss(1_700_000_000) == b"14/11/2023 22:13:20",
        out,
    );
    check_eq(&mut rep, "msec6", &format_msec6(1234) == b"001234", out);
    check_eq(&mut rep, "msec6_0", &format_msec6(0) == b"000000", out);
    check_eq(
        &mut rep,
        "msec6_wrap",
        &format_msec6(1_000_000) == b"000000",
        out,
    );
    check_eq(
        &mut rep,
        "fmt_leap",
        &format_mmddyyyy_hhmmss(1_709_164_800) == b"29/02/2024 00:00:00",
        out,
    );
    check_eq(
        &mut rep,
        "fmt_epoch_1s",
        &format_mmddyyyy_hhmmss(1) == b"01/01/1970 00:00:01",
        out,
    );
    check_eq(
        &mut rep,
        "parse_t",
        parse_set_command("T=1700000000\r\n") == Some(1_700_000_000),
        out,
    );
    check_eq(
        &mut rep,
        "parse_min_unix",
        parse_set_command("T=1000000000") == Some(1_000_000_000),
        out,
    );
    check_eq(
        &mut rep,
        "parse_short_rejected",
        parse_set_command("T=12").is_none(),
        out,
    );
    check_eq(
        &mut rep,
        "parse_junk",
        parse_set_command("T=12a").is_none() && parse_set_command("time=1").is_none(),
        out,
    );

    let mut clk = WallClock::new_unset(8);
    check_eq(&mut rep, "clock_unset", !clk.is_set() && clk.unix_at(80).is_none(), out);
    clk.set(1_700_000_000, 8);
    check_eq(
        &mut rep,
        "clock_set",
        clk.unix_at(16) == Some(1_700_000_001) && clk.ticks_per_sec() == 8,
        out,
    );
    clk.clear(24);
    check_eq(
        &mut rep,
        "clock_clear",
        !clk.is_set() && clk.msec_since(24) == 0 && clk.msec_since(32) == 1000,
        out,
    );
    let mut wrap = WallClock::new_unset(1);
    wrap.set(100, u32::MAX - 1);
    check_eq(&mut rep, "clock_wrap", wrap.unix_at(1) == Some(103), out);

    let rec = LogRecord {
        seq: 1,
        kind: EventKind::ClockSet,
        unix: 9,
        ticks: 4,
        x: 0,
        y: 0,
        z: 0,
    };
    check_eq(
        &mut rep,
        "log_roundtrip",
        decode(&encode(&rec)) == Some(rec),
        out,
    );
    let mut area = [0xFFu8; PAGE_SIZE];
    check_eq(&mut rep, "log_append", append(&mut area, &rec).is_ok(), out);
    check_eq(
        &mut rep,
        "log_kinds",
        EventKind::from_u8(1) == Some(EventKind::ClockSet)
            && EventKind::from_u8(6) == Some(EventKind::Note)
            && EventKind::from_u8(0).is_none(),
        out,
    );
    let mut bad = encode(&rec);
    bad[0] = 0;
    check_eq(&mut rep, "log_bad_magic", decode(&bad).is_none(), out);
    let mut bad_c = encode(&rec);
    bad_c[22] ^= 1;
    check_eq(&mut rep, "log_bad_csum", decode(&bad_c).is_none(), out);
    check_eq(
        &mut rep,
        "log_tiny",
        next_slot(&[0xFFu8; 8]).is_none(),
        out,
    );
    let mut full = [0u8; RECORD_SIZE];
    full.copy_from_slice(&encode(&rec));
    check_eq(&mut rep, "log_full", next_slot(&full).is_none(), out);
    let mut wipe = [0xFFu8; PAGE_SIZE];
    let _ = append(&mut wipe, &rec);
    clear(&mut wipe);
    check_eq(&mut rep, "log_clear", next_slot(&wipe) == Some(0), out);
    check_eq(
        &mut rep,
        "wheel_map",
        {
            let m = wheel_map::infer(
                ChassisMotion::PosX,
                200,
                ChassisMotion::NegX,
                180,
                60,
            );
            m.motor_a == WheelSide::Right && m.motor_b == WheelSide::Left
        },
        out,
    );
    check_eq(
        &mut rep,
        "heading_n",
        wheel_map::heading_deg(0, 100) == Some(0),
        out,
    );
    check_eq(
        &mut rep,
        "heading_e",
        wheel_map::heading_deg(100, 0) == Some(90)
            && wheel_map::Cardinal8::from_deg(90).as_bytes() == b"E",
        out,
    );
    check_eq(
        &mut rep,
        "imu_mag_nack",
        hw_bus::imu_ready(false, false, true),
        out,
    );
    check_eq(
        &mut rep,
        "imu_all_fail",
        !hw_bus::imu_ready(false, false, false)
            && hw_bus::imu_status_glyph(false, 0) == b'0',
        out,
    );
    check_eq(
        &mut rep,
        "twim1_conflicts",
        hw_bus::TWIM1_CONFLICTS.len() == 3 && hw_bus::motors_on_twim0(),
        out,
    );
    check_eq(
        &mut rep,
        "yaw_wrap",
        wheel_map::yaw_delta_cw(350, 10) == 20,
        out,
    );
    check_eq(
        &mut rep,
        "straight_ms",
        wheel_map::STRAIGHT_MS >= 3_000 && wheel_map::STRAIGHT_SPEED > 0,
        out,
    );
    check_eq(
        &mut rep,
        "latest_map",
        {
            let mut a = [0xFFu8; PAGE_SIZE];
            let mut r = rec;
            r.kind = EventKind::WheelMap;
            let _ = append(&mut a, &r);
            latest_wheel_map(&a).map(|x| x.kind) == Some(EventKind::WheelMap)
        },
        out,
    );

    let mut ui = SerialUi::new();
    check_eq(&mut rep, "ui_debug_default", ui.debug_on, out);
    check_eq(
        &mut rep,
        "ui_t_set",
        {
            let mut last = None;
            for b in b"T=1700000000\n" {
                last = ui.push_byte(*b);
            }
            last == Some(Cmd::SetTime(1_700_000_000))
        },
        out,
    );
    check_eq(
        &mut rep,
        "ui_key_stops_debug",
        SerialUi::new().push_byte(b'x') == Some(Cmd::Menu) && {
            let mut u = SerialUi::new();
            u.push_byte(b'x');
            !u.debug_on
        },
        out,
    );
    let mut menu = SerialUi::new();
    menu.debug_on = false;
    check_eq(
        &mut rep,
        "ui_show_rtc",
        menu.push_byte(b'6') == Some(Cmd::ShowRtc),
        out,
    );
    check_eq(
        &mut rep,
        "ui_clear_rtc",
        menu.push_byte(b'7') == Some(Cmd::ClearRtc),
        out,
    );
    check_eq(
        &mut rep,
        "ui_clear_log",
        menu.push_byte(b'8') == Some(Cmd::ClearLog),
        out,
    );
    check_eq(
        &mut rep,
        "ui_wheel_cal",
        menu.push_byte(b'9') == Some(Cmd::WheelCal),
        out,
    );
    check_eq(&mut rep, "btnA_log_off", !button_a_prints_log(false), out);
    check_eq(&mut rep, "btnA_log_on", button_a_prints_log(true), out);
    let mut digits = SerialUi::new();
    digits.debug_on = false;
    check_eq(
        &mut rep,
        "ui_status",
        digits.push_byte(b'1') == Some(Cmd::Status),
        out,
    );
    check_eq(
        &mut rep,
        "ui_dump",
        digits.push_byte(b'2') == Some(Cmd::Dump),
        out,
    );
    check_eq(
        &mut rep,
        "ui_dbg_on",
        digits.push_byte(b'3') == Some(Cmd::DebugOn) && digits.debug_on,
        out,
    );
    digits.debug_on = false;
    check_eq(
        &mut rep,
        "ui_runtests",
        digits.push_byte(b'4') == Some(Cmd::RunTests),
        out,
    );
    check_eq(
        &mut rep,
        "ui_led",
        digits.push_byte(b'5') == Some(Cmd::LedCount),
        out,
    );
    let mut ov = SerialUi::new();
    let mut overflow_menu = false;
    for _ in 0..41 {
        overflow_menu = ov.push_byte(b'T') == Some(Cmd::Menu);
    }
    check_eq(&mut rep, "ui_overflow", overflow_menu && !ov.debug_on, out);

    let mut ring = Ring::<8>::new();
    check_eq(&mut rep, "ring_cap", ring.cap() == 7, out);
    check_eq(&mut rep, "ring_push", ring.push(9), out);
    check_eq(&mut rep, "ring_pop", ring.pop() == Some(9), out);
    let mut filled = 0u8;
    while ring.push(filled) {
        filled = filled.saturating_add(1);
    }
    check_eq(&mut rep, "ring_full", filled == 7 && ring.pop() == Some(0), out);

    check_eq(
        &mut rep,
        "led_red",
        color_to_pwm(LedColor::Red) == (0, 255, 255),
        out,
    );
    check_eq(
        &mut rep,
        "led_green",
        color_to_pwm(LedColor::Green) == (255, 0, 255),
        out,
    );
    check_eq(
        &mut rep,
        "led_off",
        color_to_pwm(LedColor::Black) == (255, 255, 255),
        out,
    );
    check_eq(&mut rep, "ultra_div", ultra::pulse_us_to_cm(580) == Some(10), out);
    check_eq(
        &mut rep,
        "ultra_short",
        ultra::pulse_us_to_cm(50).is_none(),
        out,
    );
    check_eq(
        &mut rep,
        "motion_ready",
        motion::rest_status(&[MilliG::new(0, 0, 1000)]) == RestStatus::Ready,
        out,
    );
    check_eq(
        &mut rep,
        "motion_empty",
        motion::rest_status(&[]) == RestStatus::NotLevel,
        out,
    );
    check_eq(
        &mut rep,
        "motion_fall",
        motion::rest_status(&[MilliG::new(0, 0, 0)]) == RestStatus::Freefall,
        out,
    );
    check_eq(
        &mut rep,
        "motion_posx",
        motion::classify_delta(MilliG::new(0, 0, 1000), MilliG::new(200, 0, 1000)).0
            == ChassisMotion::PosX,
        out,
    );
    check_eq(
        &mut rep,
        "motion_none",
        !motion::chassis_moved(MilliG::new(0, 0, 1000), MilliG::new(10, 0, 1000)),
        out,
    );

    let mut l = StubPin { high: false };
    let mut r = StubPin { high: false };
    check_eq(
        &mut rep,
        "line_both",
        line_tracking::read(&mut l, &mut r) == Ok(LineTrackingSensor::Both),
        out,
    );
    l.high = true;
    check_eq(
        &mut rep,
        "line_left",
        line_tracking::read(&mut l, &mut r) == Ok(LineTrackingSensor::Left),
        out,
    );
    l.high = false;
    r.high = true;
    check_eq(
        &mut rep,
        "line_right",
        line_tracking::read(&mut l, &mut r) == Ok(LineTrackingSensor::Right),
        out,
    );
    l.high = true;
    check_eq(
        &mut rep,
        "line_none",
        line_tracking::read(&mut l, &mut r) == Ok(LineTrackingSensor::None),
        out,
    );
    check_eq(
        &mut rep,
        "follow_stop",
        line_tracking::follow_cmd(
            LineTrackingSensor::Both,
            true,
            LineTrackingSensor::None,
        ) == line_tracking::FollowCmd::Stop,
        out,
    );

    let mut i2c = FakeI2c { last: [0, 0] };
    let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Forward);
    check_eq(&mut rep, "motor_a_fwd_dir", i2c.last == [0x02, 255], out);
    let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Backward);
    check_eq(&mut rep, "motor_a_rev_dir", i2c.last == [0x02, 0], out);
    let _ = motor::set(&mut i2c, 180, Motor::B, Direction::Forward);
    check_eq(&mut rep, "motor_b_fwd_spd", i2c.last == [0x04, 180], out);
    let _ = motor::stop(&mut i2c);
    check_eq(&mut rep, "motor_stop", i2c.last == [0x04, 0], out);
    let _ = write_reg(&mut i2c, 0x08, 0xAA);
    check_eq(&mut rep, "bus_write", i2c.last == [0x08, 0xAA], out);

    out("done", rep.ok());
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selftest_all_pass_on_host() {
        let r = run_all(&mut |n, ok| assert!(ok, "{n} failed"));
        assert!(r.ok());
        assert!(r.pass >= 55);
    }
}
