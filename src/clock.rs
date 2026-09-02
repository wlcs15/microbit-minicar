//! Software wall clock on top of the nRF52833 RTC counter.
//!
//! The micro:bit v2 has **RTC peripherals** (32.768 kHz LFCLK) but **no
//! battery-backed calendar**. Time runs only while the nRF52833 is powered
//! (USB, or car batteries if they still feed the board). Removing the AAAs
//! and switching the car off (if that cuts VDD) **stops and loses** the clock.
//! Each log record stores unix seconds at write time so the flash log still
//! has timestamps after power loss; the live clock does not.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallClock {
    /// Unix seconds at `origin_ticks`. `None` until `set`.
    unix_at_origin: Option<u32>,
    origin_ticks: u32,
    ticks_per_sec: u32,
}

impl WallClock {
    /// `ticks_per_sec` must match the RTC prescaler used in firmware (e.g. 8).
    pub const fn new_unset(ticks_per_sec: u32) -> Self {
        Self {
            unix_at_origin: None,
            origin_ticks: 0,
            ticks_per_sec,
        }
    }

    pub const fn is_set(&self) -> bool {
        self.unix_at_origin.is_some()
    }

    pub fn ticks_per_sec(&self) -> u32 {
        self.ticks_per_sec
    }

    /// Set wall time. `now_ticks` is the current RTC counter (wrapping 24-bit
    /// values should be passed already as a widening software counter).
    pub fn set(&mut self, unix_seconds: u32, now_ticks: u32) {
        self.unix_at_origin = Some(unix_seconds);
        self.origin_ticks = now_ticks;
    }

    pub fn unix_at(&self, now_ticks: u32) -> Option<u32> {
        let origin_unix = self.unix_at_origin?;
        let tps = self.ticks_per_sec.max(1);
        let dt = now_ticks.wrapping_sub(self.origin_ticks);
        Some(origin_unix.wrapping_add(dt / tps))
    }

    /// Clear wall time and restart the millisecond counter at 0.
    pub fn clear(&mut self, now_ticks: u32) {
        self.unix_at_origin = None;
        self.origin_ticks = now_ticks;
    }

    pub fn msec_since(&self, now_ticks: u32) -> u32 {
        let tps = self.ticks_per_sec.max(1);
        let dt = now_ticks.wrapping_sub(self.origin_ticks);
        dt.saturating_mul(1000) / tps
    }
}

/// `DD/MM/YYYY HH:MM:SS` in UTC (19 bytes, no NUL).
pub const STAMP_LEN: usize = 19;

fn unix_to_ymdhms(unix: u32) -> (u16, u8, u8, u8, u8, u8) {
    let days = i64::from(unix / 86_400);
    let tod = unix % 86_400;
    let hour = (tod / 3_600) as u8;
    let min = ((tod % 3_600) / 60) as u8;
    let sec = (tod % 60) as u8;
    let (year, month, day) = civil_from_unix_days(days);
    (year, month, day, hour, min, sec)
}

/// Howard Hinnant civil-from-days; `days` is days since 1970-01-01 UTC.
fn civil_from_unix_days(days: i64) -> (u16, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (y + i64::from(m <= 2)) as u16;
    (year, m as u8, d as u8)
}

fn push_2(buf: &mut [u8], at: usize, v: u8) {
    buf[at] = b'0' + (v / 10);
    buf[at + 1] = b'0' + (v % 10);
}

pub fn format_mmddyyyy_hhmmss(unix: u32) -> [u8; STAMP_LEN] {
    let (year, month, day, hour, min, sec) = unix_to_ymdhms(unix);
    let mut buf = [b' '; STAMP_LEN];
    push_2(&mut buf, 0, day);
    buf[2] = b'/';
    push_2(&mut buf, 3, month);
    buf[5] = b'/';
    buf[6] = b'0' + ((year / 1000) % 10) as u8;
    buf[7] = b'0' + ((year / 100) % 10) as u8;
    buf[8] = b'0' + ((year / 10) % 10) as u8;
    buf[9] = b'0' + (year % 10) as u8;
    buf[10] = b' ';
    push_2(&mut buf, 11, hour);
    buf[13] = b':';
    push_2(&mut buf, 14, min);
    buf[16] = b':';
    push_2(&mut buf, 17, sec);
    buf
}

pub fn format_mmddyyyy_hhmmss_str(unix: u32) -> [u8; STAMP_LEN] {
    format_mmddyyyy_hhmmss(unix)
}

pub fn format_msec6(ms: u32) -> [u8; 6] {
    let v = ms % 1_000_000;
    let mut buf = [b'0'; 6];
    let mut n = v;
    for i in (0..6).rev() {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    buf
}

/// Reject truncated `T=` values that would display as 1970.
pub const MIN_UNIX: u32 = 1_000_000_000;

/// Parse `T=<unix>` (UTC seconds). Requires 10+ digits so a dropped
/// `T=1` cannot become 1 Jan 1970.
pub fn parse_set_command(line: &str) -> Option<u32> {
    let line = line.trim();
    let rest = line.strip_prefix("T=")?;
    if rest.len() < 10 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = rest.parse().ok()?;
    if n < MIN_UNIX {
        return None;
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_until_set() {
        let mut c = WallClock::new_unset(8);
        assert!(!c.is_set());
        assert_eq!(c.unix_at(80), None);
        c.set(1_700_000_000, 16);
        assert!(c.is_set());
        assert_eq!(c.unix_at(16), Some(1_700_000_000));
        assert_eq!(c.unix_at(24), Some(1_700_000_001));
        assert_eq!(c.ticks_per_sec(), 8);
        c.clear(32);
        assert!(!c.is_set());
        assert_eq!(c.msec_since(32), 0);
        assert_eq!(c.msec_since(40), 1000);
    }

    #[test]
    fn wrapping_ticks() {
        let mut c = WallClock::new_unset(1);
        c.set(100, u32::MAX - 1);
        assert_eq!(c.unix_at(1), Some(103));
    }

    #[test]
    fn parse_t_command() {
        assert_eq!(parse_set_command("T=1700000000\r\n"), Some(1_700_000_000));
        assert_eq!(parse_set_command("  T=12 "), None);
        assert_eq!(parse_set_command("T=1000000000"), Some(1_000_000_000));
        assert_eq!(parse_set_command("T="), None);
        assert_eq!(parse_set_command("T=12a"), None);
        assert_eq!(parse_set_command("time=1"), None);
    }

    fn stamp(unix: u32) -> String {
        let b = format_mmddyyyy_hhmmss(unix);
        String::from_utf8(b.to_vec()).unwrap()
    }

    #[test]
    fn formats_epoch() {
        assert_eq!(stamp(0), "01/01/1970 00:00:00");
        assert_eq!(stamp(1), "01/01/1970 00:00:01");
    }

    #[test]
    fn formats_known_unix() {
        assert_eq!(stamp(1_700_000_000), "14/11/2023 22:13:20");
        assert_eq!(stamp(1_709_164_800), "29/02/2024 00:00:00");
    }

    #[test]
    fn msec6_pads_and_wraps() {
        assert_eq!(&format_msec6(0), b"000000");
        assert_eq!(&format_msec6(1234), b"001234");
        assert_eq!(&format_msec6(1_000_000), b"000000");
    }
}
