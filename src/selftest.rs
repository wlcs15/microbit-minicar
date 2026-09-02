//! On-host and on-target checks over the same pure functions.
//! `cargo test` runs these on the PC. Firmware can call `run_all` and print
//! each line on UART.

use crate::clock::{format_mmddyyyy_hhmmss, parse_set_command, WallClock};
use crate::log_store::{append, decode, encode, EventKind, LogRecord, PAGE_SIZE};

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

pub fn run_all(out: &mut impl FnMut(&str, bool)) -> Report {
    let mut rep = Report { pass: 0, fail: 0 };

    let epoch = format_mmddyyyy_hhmmss(0);
    check_eq(&mut rep, "fmt_epoch", &epoch == b"01011970 000000", out);

    let known = format_mmddyyyy_hhmmss(1_700_000_000);
    check_eq(
        &mut rep,
        "fmt_1700000000",
        &known == b"11142023 221320",
        out,
    );

    check_eq(
        &mut rep,
        "parse_t",
        parse_set_command("T=1700000000\r\n") == Some(1_700_000_000),
        out,
    );
    check_eq(&mut rep, "parse_bad", parse_set_command("x").is_none(), out);

    let mut clk = WallClock::new_unset(8);
    check_eq(&mut rep, "clock_unset", !clk.is_set(), out);
    clk.set(1_000, 8);
    check_eq(&mut rep, "clock_set", clk.unix_at(16) == Some(1_001), out);

    let rec = LogRecord {
        seq: 1,
        kind: EventKind::ClockSet,
        unix: 9,
        ticks: 4,
        x: 0,
        y: 0,
        z: 0,
    };
    check_eq(&mut rep, "log_roundtrip", decode(&encode(&rec)) == Some(rec), out);

    let mut area = [0xFFu8; PAGE_SIZE];
    check_eq(&mut rep, "log_append", append(&mut area, &rec).is_ok(), out);

    out("done", rep.ok());
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selftest_all_pass_on_host() {
        let mut names = Vec::new();
        let r = run_all(&mut |n, ok| {
            names.push((n.to_string(), ok));
            assert!(ok, "{n} failed");
        });
        assert!(r.ok());
        assert!(r.pass >= 7);
    }
}
