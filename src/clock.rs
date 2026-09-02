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
}

/// Parse a set-clock command `T=<unix>` (decimal, optional CR/LF).
pub fn parse_set_command(line: &str) -> Option<u32> {
    let line = line.trim();
    let rest = line.strip_prefix("T=")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
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
        assert_eq!(parse_set_command("  T=12 "), Some(12));
        assert_eq!(parse_set_command("T="), None);
        assert_eq!(parse_set_command("T=12a"), None);
        assert_eq!(parse_set_command("time=1"), None);
    }
}
