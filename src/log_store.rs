//! Persistent calibration log for untethered floor runs.
//!
//! Records live in the last nRF52833 flash pages (4 KiB erase). USB/RTT is
//! not required while driving; reconnect later and scan flash. That improves
//! motor/IMU tests: the car can move freely, then dump the log.

pub const RECORD_SIZE: usize = 24;
pub const MAGIC: u32 = 0x4853_3101; // HS1 + version
pub const PAGE_SIZE: usize = 4096;
/// Last two 4 KiB pages of the 512 KiB flash map.
pub const FLASH_LOG_BASE: u32 = 0x0007_E000;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    ClockSet = 1,
    RestReady = 2,
    RestBlocked = 3,
    MotorPulse = 4,
    ChassisDelta = 5,
    Note = 6,
    /// Motor A/B → left/right from a chassis pulse (`x`/`y` = [`crate::wheel_map::WheelSide`]).
    WheelMap = 7,
    /// Mag 360° spin: x=CW deg, y=CCW deg, z=1 if both hit target.
    Yaw360 = 8,
}

impl EventKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::ClockSet),
            2 => Some(Self::RestReady),
            3 => Some(Self::RestBlocked),
            4 => Some(Self::MotorPulse),
            5 => Some(Self::ChassisDelta),
            6 => Some(Self::Note),
            7 => Some(Self::WheelMap),
            8 => Some(Self::Yaw360),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogRecord {
    pub seq: u16,
    pub kind: EventKind,
    /// Unix seconds if the clock was set this session; 0 if unknown.
    pub unix: u32,
    pub ticks: u32,
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

pub fn clamp_mg(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Sequential record numbers for a session (wraps).
pub struct Seq {
    next: u16,
}

impl Default for Seq {
    fn default() -> Self {
        Self::new()
    }
}

impl Seq {
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    pub fn emit(
        &mut self,
        kind: EventKind,
        unix: u32,
        ticks: u32,
        x: i32,
        y: i32,
        z: i32,
    ) -> LogRecord {
        let seq = self.next;
        self.next = self.next.wrapping_add(1);
        if self.next == 0 {
            self.next = 1;
        }
        LogRecord {
            seq,
            kind,
            unix,
            ticks,
            x: clamp_mg(x),
            y: clamp_mg(y),
            z: clamp_mg(z),
        }
    }
}

pub fn checksum(bytes: &[u8]) -> u16 {
    let mut s: u16 = 0;
    for (i, b) in bytes.iter().enumerate() {
        s = s.wrapping_add(u16::from(*b).wrapping_mul((i as u16).wrapping_add(1)));
    }
    s
}

pub fn encode(rec: &LogRecord) -> [u8; RECORD_SIZE] {
    let mut buf = [0u8; RECORD_SIZE];
    buf[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buf[4..6].copy_from_slice(&rec.seq.to_le_bytes());
    buf[6] = rec.kind as u8;
    buf[7] = 0;
    buf[8..12].copy_from_slice(&rec.unix.to_le_bytes());
    buf[12..16].copy_from_slice(&rec.ticks.to_le_bytes());
    buf[16..18].copy_from_slice(&rec.x.to_le_bytes());
    buf[18..20].copy_from_slice(&rec.y.to_le_bytes());
    buf[20..22].copy_from_slice(&rec.z.to_le_bytes());
    let csum = checksum(&buf[..22]);
    buf[22..24].copy_from_slice(&csum.to_le_bytes());
    buf
}

pub fn decode(buf: &[u8; RECORD_SIZE]) -> Option<LogRecord> {
    let magic = u32::from_le_bytes(buf[0..4].try_into().ok()?);
    if magic != MAGIC {
        return None;
    }
    let csum_stored = u16::from_le_bytes(buf[22..24].try_into().ok()?);
    if checksum(&buf[..22]) != csum_stored {
        return None;
    }
    let kind = EventKind::from_u8(buf[6])?;
    Some(LogRecord {
        seq: u16::from_le_bytes(buf[4..6].try_into().ok()?),
        kind,
        unix: u32::from_le_bytes(buf[8..12].try_into().ok()?),
        ticks: u32::from_le_bytes(buf[12..16].try_into().ok()?),
        x: i16::from_le_bytes(buf[16..18].try_into().ok()?),
        y: i16::from_le_bytes(buf[18..20].try_into().ok()?),
        z: i16::from_le_bytes(buf[20..22].try_into().ok()?),
    })
}

pub fn next_slot(area: &[u8]) -> Option<usize> {
    if area.len() < RECORD_SIZE {
        return None;
    }
    let n = area.len() / RECORD_SIZE;
    for i in 0..n {
        let off = i * RECORD_SIZE;
        let chunk: [u8; RECORD_SIZE] = area[off..off + RECORD_SIZE].try_into().ok()?;
        if chunk.iter().all(|b| *b == 0xFF) {
            return Some(off);
        }
        if decode(&chunk).is_none() {
            return Some(off);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogError {
    Full,
}

/// Mark the RAM image empty (erased flash is 0xFF). Firmware must erase NVMC.
pub fn clear(area: &mut [u8]) {
    area.fill(0xFF);
}

pub fn append(area: &mut [u8], rec: &LogRecord) -> Result<usize, LogError> {
    let off = next_slot(area).ok_or(LogError::Full)?;
    let bytes = encode(rec);
    area[off..off + RECORD_SIZE].copy_from_slice(&bytes);
    Ok(off)
}

pub fn iter_valid(area: &[u8]) -> impl Iterator<Item = LogRecord> + '_ {
    area.as_chunks::<RECORD_SIZE>().0.iter().filter_map(decode)
}

/// Latest wheel map in the log (re-run cal on new hardware; this is the live set).
pub fn latest_wheel_map(area: &[u8]) -> Option<LogRecord> {
    iter_valid(area).filter(|r| r.kind == EventKind::WheelMap).last()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u16) -> LogRecord {
        LogRecord {
            seq,
            kind: EventKind::MotorPulse,
            unix: 1_700_000_042,
            ticks: 99,
            x: 150,
            y: -20,
            z: 980,
        }
    }

    #[test]
    fn round_trip() {
        let r = rec(3);
        let out = decode(&encode(&r)).unwrap();
        assert_eq!(out, r);
    }

    #[test]
    fn all_kinds_round_trip() {
        for k in [
            EventKind::ClockSet,
            EventKind::RestReady,
            EventKind::RestBlocked,
            EventKind::MotorPulse,
            EventKind::ChassisDelta,
            EventKind::Note,
            EventKind::WheelMap,
            EventKind::Yaw360,
        ] {
            let mut r = rec(1);
            r.kind = k;
            assert_eq!(decode(&encode(&r)).unwrap().kind, k);
        }
    }

    #[test]
    fn bad_magic_rejected() {
        let mut b = encode(&rec(1));
        b[0] ^= 1;
        assert!(decode(&b).is_none());
    }

    #[test]
    fn bad_checksum_rejected() {
        let mut b = encode(&rec(1));
        b[10] ^= 1;
        assert!(decode(&b).is_none());
    }

    #[test]
    fn kind_from_u8() {
        assert_eq!(EventKind::from_u8(1), Some(EventKind::ClockSet));
        assert_eq!(EventKind::from_u8(6), Some(EventKind::Note));
        assert_eq!(EventKind::from_u8(7), Some(EventKind::WheelMap));
        assert_eq!(EventKind::from_u8(8), Some(EventKind::Yaw360));
        assert_eq!(EventKind::from_u8(0), None);
        assert_eq!(EventKind::from_u8(99), None);
    }

    #[test]
    fn append_and_scan() {
        let mut area = [0xFFu8; PAGE_SIZE];
        assert_eq!(next_slot(&area), Some(0));
        append(&mut area, &rec(1)).unwrap();
        append(&mut area, &rec(2)).unwrap();
        let v: Vec<_> = iter_valid(&area).collect();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].seq, 2);
        assert_eq!(next_slot(&area), Some(2 * RECORD_SIZE));
    }

    #[test]
    fn full_area_returns_err() {
        let mut area = [0u8; RECORD_SIZE];
        area.copy_from_slice(&encode(&rec(1)));
        assert_eq!(next_slot(&area), None);
        assert_eq!(append(&mut area, &rec(2)), Err(LogError::Full));
    }

    #[test]
    fn tiny_area_has_no_slot() {
        assert_eq!(next_slot(&[0xFF; 8]), None);
    }

    #[test]
    fn seq_and_clamp() {
        let mut s = Seq::new();
        let a = s.emit(EventKind::RestReady, 1, 2, 40_000, -40_000, 0);
        let b = s.emit(EventKind::Note, 1, 2, 0, 0, 0);
        assert_eq!(a.seq, 1);
        assert_eq!(b.seq, 2);
        assert_eq!(a.x, i16::MAX);
        assert_eq!(a.y, i16::MIN);
    }

    #[test]
    fn clear_wipes_records() {
        let mut area = [0xFFu8; PAGE_SIZE];
        append(&mut area, &rec(1)).unwrap();
        assert_eq!(iter_valid(&area).count(), 1);
        clear(&mut area);
        assert_eq!(iter_valid(&area).count(), 0);
        assert_eq!(next_slot(&area), Some(0));
    }

    #[test]
    fn latest_wheel_map_is_last_record() {
        let mut area = [0xFFu8; PAGE_SIZE];
        let mut a = rec(1);
        a.kind = EventKind::WheelMap;
        a.x = 1;
        append(&mut area, &a).unwrap();
        let mut b = rec(2);
        b.kind = EventKind::WheelMap;
        b.x = 90;
        append(&mut area, &b).unwrap();
        let last = latest_wheel_map(&area).unwrap();
        assert_eq!(last.x, 90);
        assert_eq!(last.seq, 2);
    }
}
