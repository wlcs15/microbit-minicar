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
use crate::log_store::{append, decode, encode, EventKind, LogRecord, PAGE_SIZE};
use crate::motion::{self, MilliG};
use crate::motor::{self, Direction, Motor};
use crate::ring::Ring;
use crate::serial_ui::{button_a_prints_log, Cmd, SerialUi};
use crate::ultra;

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
    check_eq(
        &mut rep,
        "parse_t",
        parse_set_command("T=1700000000\r\n") == Some(1_700_000_000),
        out,
    );
    check_eq(
        &mut rep,
        "parse_short_rejected",
        parse_set_command("T=12").is_none(),
        out,
    );

    let mut clk = WallClock::new_unset(8);
    check_eq(&mut rep, "clock_unset", !clk.is_set(), out);
    clk.set(1_700_000_000, 8);
    check_eq(
        &mut rep,
        "clock_set",
        clk.unix_at(16) == Some(1_700_000_001),
        out,
    );
    clk.clear(24);
    check_eq(&mut rep, "clock_clear", !clk.is_set() && clk.msec_since(24) == 0, out);

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
    check_eq(&mut rep, "btnA_log_off", !button_a_prints_log(false), out);
    check_eq(&mut rep, "btnA_log_on", button_a_prints_log(true), out);

    let mut ring = Ring::<16>::new();
    check_eq(&mut rep, "ring_push", ring.push(9), out);
    check_eq(&mut rep, "ring_pop", ring.pop() == Some(9), out);

    check_eq(
        &mut rep,
        "led_red",
        color_to_pwm(LedColor::Red) == (0, 255, 255),
        out,
    );
    check_eq(&mut rep, "ultra_div", ultra::pulse_us_to_cm(580) == Some(10), out);
    check_eq(
        &mut rep,
        "motion_ready",
        motion::rest_status(&[MilliG::new(0, 0, 1000)]) == motion::RestStatus::Ready,
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

    let mut i2c = FakeI2c { last: [0, 0] };
    let _ = motor::set(&mut i2c, 200, Motor::A, Direction::Forward);
    check_eq(&mut rep, "motor_a_fwd_dir", i2c.last == [0x02, 255], out);
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
        assert!(r.pass >= 20);
    }
}
