//! USB-serial UI.
//!
//! Debug stream is on by default. While it is on, any key except a `T=<unix>`
//! line **stops debug and opens the menu** (`3` turns debug back on from the
//! menu). Menu digits `1`–`5` only run when debug is already off.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    SetTime(u32),
    Status,
    Dump,
    DebugOn,
    RunTests,
    LedCount,
    ShowRtc,
    ClearRtc,
    ClearLog,
    WheelCal,
    Menu,
}

pub struct SerialUi {
    pub debug_on: bool,
    line: [u8; 40],
    line_n: usize,
    pub rx_bytes: u32,
}

impl Default for SerialUi {
    fn default() -> Self {
        Self::new()
    }
}

/// Button A may print the flash dump only while debug logging is on.
pub const fn button_a_prints_log(debug_on: bool) -> bool {
    debug_on
}

impl SerialUi {
    pub const fn new() -> Self {
        Self {
            debug_on: true,
            line: [0; 40],
            line_n: 0,
            rx_bytes: 0,
        }
    }

    pub fn push_byte(&mut self, c: u8) -> Option<Cmd> {
        self.rx_bytes = self.rx_bytes.saturating_add(1);
        if c == b'\n' || c == b'\r' {
            if self.line_n == 0 {
                return None;
            }
            let n = self.line_n;
            self.line_n = 0;
            return self.finish_line(n);
        }

        if self.line_n == 0 {
            if self.debug_on && c != b'T' {
                self.debug_on = false;
                return Some(Cmd::Menu);
            }
            if c == b'?' {
                self.debug_on = false;
                return Some(Cmd::Menu);
            }
            let digit = match c {
                b'1' => Some(Cmd::Status),
                b'2' => Some(Cmd::Dump),
                b'3' => {
                    self.debug_on = true;
                    Some(Cmd::DebugOn)
                }
                b'4' => Some(Cmd::RunTests),
                b'5' => Some(Cmd::LedCount),
                b'6' => Some(Cmd::ShowRtc),
                b'7' => Some(Cmd::ClearRtc),
                b'8' => Some(Cmd::ClearLog),
                b'9' => Some(Cmd::WheelCal),
                _ => None,
            };
            if let Some(cmd) = digit {
                return Some(cmd);
            }
        }

        if self.line_n >= self.line.len() {
            self.line_n = 0;
            self.debug_on = false;
            return Some(Cmd::Menu);
        }
        self.line[self.line_n] = c;
        self.line_n += 1;
        if self.line_n == 1 && c != b'T' {
            self.debug_on = false;
            self.line_n = 0;
            return Some(Cmd::Menu);
        }
        None
    }

    fn finish_line(&mut self, n: usize) -> Option<Cmd> {
        let s = core::str::from_utf8(&self.line[..n]).ok()?;
        if let Some(unix) = crate::clock::parse_set_command(s) {
            return Some(Cmd::SetTime(unix));
        }
        self.debug_on = false;
        Some(Cmd::Menu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ui: &mut SerialUi, s: &str) -> Option<Cmd> {
        let mut last = None;
        for b in s.as_bytes() {
            if let Some(c) = ui.push_byte(*b) {
                last = Some(c);
            }
        }
        last
    }

    #[test]
    fn button_a_log_only_when_debug_on() {
        assert!(button_a_prints_log(true));
        assert!(!button_a_prints_log(false));
    }

    #[test]
    fn debug_on_by_default() {
        assert!(SerialUi::new().debug_on);
    }

    #[test]
    fn t_equals_sets_time_and_keeps_debug() {
        let mut ui = SerialUi::new();
        assert_eq!(
            line(&mut ui, "T=1700000000\n"),
            Some(Cmd::SetTime(1_700_000_000))
        );
        assert!(ui.debug_on);
    }

    #[test]
    fn any_key_during_debug_opens_menu_including_digit_three() {
        let mut ui = SerialUi::new();
        assert_eq!(ui.push_byte(b'3'), Some(Cmd::Menu));
        assert!(!ui.debug_on);
        let mut ui = SerialUi::new();
        assert_eq!(ui.push_byte(b'4'), Some(Cmd::Menu));
        let mut ui = SerialUi::new();
        assert_eq!(ui.push_byte(b'x'), Some(Cmd::Menu));
        let mut ui = SerialUi::new();
        assert_eq!(ui.push_byte(b'?'), Some(Cmd::Menu));
    }

    #[test]
    fn after_menu_three_enables_debug_and_next_key_stops() {
        let mut ui = SerialUi::new();
        assert_eq!(ui.push_byte(b'?'), Some(Cmd::Menu));
        assert_eq!(ui.push_byte(b'3'), Some(Cmd::DebugOn));
        assert!(ui.debug_on);
        assert_eq!(ui.push_byte(b'1'), Some(Cmd::Menu));
        assert!(!ui.debug_on);
    }

    #[test]
    fn menu_digits_only_when_debug_off() {
        let mut ui = SerialUi::new();
        ui.debug_on = false;
        assert_eq!(ui.push_byte(b'4'), Some(Cmd::RunTests));
        assert_eq!(ui.push_byte(b'2'), Some(Cmd::Dump));
        assert_eq!(ui.push_byte(b'1'), Some(Cmd::Status));
        assert_eq!(ui.push_byte(b'5'), Some(Cmd::LedCount));
        assert_eq!(ui.push_byte(b'6'), Some(Cmd::ShowRtc));
        assert_eq!(ui.push_byte(b'7'), Some(Cmd::ClearRtc));
        assert_eq!(ui.push_byte(b'8'), Some(Cmd::ClearLog));
        assert_eq!(ui.push_byte(b'9'), Some(Cmd::WheelCal));
        assert_eq!(ui.push_byte(b'3'), Some(Cmd::DebugOn));
        assert!(ui.debug_on);
    }

    #[test]
    fn crlf_t_command() {
        let mut ui = SerialUi::new();
        assert_eq!(line(&mut ui, "T=1700000000\r\n"), Some(Cmd::SetTime(1_700_000_000)));
    }

    #[test]
    fn empty_cr_does_nothing() {
        let mut ui = SerialUi::new();
        assert_eq!(ui.push_byte(b'\r'), None);
        assert_eq!(ui.push_byte(b'\n'), None);
        assert!(ui.debug_on);
    }

    #[test]
    fn bad_t_line_is_menu_and_stops_debug() {
        let mut ui = SerialUi::new();
        assert_eq!(line(&mut ui, "T=\n"), Some(Cmd::Menu));
        assert!(!ui.debug_on);
        let mut ui = SerialUi::new();
        assert_eq!(line(&mut ui, "T=12a\n"), Some(Cmd::Menu));
        let mut ui = SerialUi::new();
        assert_eq!(line(&mut ui, "T=9999999999999\n"), Some(Cmd::Menu));
    }

    #[test]
    fn overflow_line_opens_menu() {
        let mut ui = SerialUi::new();
        let mut saw_menu = false;
        for _ in 0..50 {
            if ui.push_byte(b'T') == Some(Cmd::Menu) {
                saw_menu = true;
                break;
            }
        }
        assert!(saw_menu);
        assert!(!ui.debug_on);
    }

    #[test]
    fn gui_question_crlf() {
        let mut ui = SerialUi::new();
        let mut cmds = vec![];
        for b in b"?\r\n" {
            if let Some(c) = ui.push_byte(*b) {
                cmds.push(c);
            }
        }
        assert_eq!(cmds, vec![Cmd::Menu]);
    }

    #[test]
    fn gui_set_time_then_stop_debug() {
        let mut ui = SerialUi::new();
        assert_eq!(line(&mut ui, "T=1700000000\r\n"), Some(Cmd::SetTime(1_700_000_000)));
        assert!(ui.debug_on);
        assert_eq!(ui.push_byte(b'?'), Some(Cmd::Menu));
        assert!(!ui.debug_on);
    }

    #[test]
    fn random_sequences_invariants() {
        let ops: [&[u8]; 10] = [
            b"T=1000000000\n",
            b"T=1700000000\n",
            b"?",
            b"1",
            b"2",
            b"3",
            b"4",
            b"5",
            b"x",
            b"T=nope\n",
        ];
        let mut rng: u64 = 0xC0FFEE;
        for _ in 0..100 {
            let mut ui = SerialUi::new();
            let mut last = None;
            for _ in 0..10 {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let op = ops[(rng as usize) % ops.len()];
                last = line(&mut ui, core::str::from_utf8(op).unwrap());
                if ui.debug_on {
                    assert_ne!(last, Some(Cmd::Menu));
                }
                if last == Some(Cmd::Menu) {
                    assert!(!ui.debug_on);
                }
                if last == Some(Cmd::DebugOn) {
                    assert!(ui.debug_on);
                }
            }
        }
    }
}
