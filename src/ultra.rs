use core::hint::spin_loop;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};

const TIMEOUT_US: u32 = 30_000;
const MIN_VALID_PULSE_US: u32 = 120;
/// Standard HC-SR04 centimetres. Kit Python used `t * 0.013`; confirmed later.
pub const CM_DIVISOR: u32 = 58;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum UltrasonicError<TriggerError, EchoError> {
    Trigger(TriggerError),
    Echo(EchoError),
}

pub fn pulse_us_to_cm(pulse_us: u32) -> Option<u32> {
    if pulse_us < MIN_VALID_PULSE_US {
        None
    } else {
        Some(pulse_us / CM_DIVISOR)
    }
}

pub fn measure_cm<Trig, Echo, Delay, Clock>(
    trigger: &mut Trig,
    echo: &mut Echo,
    pulse_delay: &mut Delay,
    now_us: &mut Clock,
) -> Result<Option<u32>, UltrasonicError<Trig::Error, Echo::Error>>
where
    Trig: OutputPin,
    Echo: InputPin,
    Delay: DelayNs,
    Clock: FnMut() -> u32,
{
    trigger.set_low().map_err(UltrasonicError::Trigger)?;
    pulse_delay.delay_us(2);
    trigger.set_high().map_err(UltrasonicError::Trigger)?;
    pulse_delay.delay_us(10);
    trigger.set_low().map_err(UltrasonicError::Trigger)?;

    let t_idle = now_us();

    while echo.is_high().map_err(UltrasonicError::Echo)? {
        if now_us().wrapping_sub(t_idle) > TIMEOUT_US {
            return Ok(None);
        }
        spin_loop();
    }

    let t_wait_rise = now_us();
    while echo.is_low().map_err(UltrasonicError::Echo)? {
        if now_us().wrapping_sub(t_wait_rise) > TIMEOUT_US {
            return Ok(None);
        }
        spin_loop();
    }

    let start = now_us();
    while echo.is_high().map_err(UltrasonicError::Echo)? {
        if now_us().wrapping_sub(start) > TIMEOUT_US {
            return Ok(None);
        }
        spin_loop();
    }

    let pulse_us = now_us().wrapping_sub(start);
    Ok(pulse_us_to_cm(pulse_us))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::delay::DelayNs;
    use embedded_hal::digital::{ErrorType, InputPin, OutputPin};

    struct NopDelay;

    impl DelayNs for NopDelay {
        fn delay_ns(&mut self, _ns: u32) {}
    }

    struct Trig;

    impl ErrorType for Trig {
        type Error = Infallible;
    }

    impl OutputPin for Trig {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    struct Echo {
        levels: &'static [bool],
        idx: usize,
    }

    impl Echo {
        fn new(levels: &'static [bool]) -> Self {
            Self { levels, idx: 0 }
        }

        fn sample(&mut self) -> bool {
            let last = *self.levels.last().unwrap_or(&false);
            let v = *self.levels.get(self.idx).unwrap_or(&last);
            self.idx = self.idx.saturating_add(1);
            v
        }
    }

    impl ErrorType for Echo {
        type Error = Infallible;
    }

    impl InputPin for Echo {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(self.sample())
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(!self.sample())
        }
    }

    fn clock_from(start: u32, step: u32) -> impl FnMut() -> u32 {
        let mut t = start;
        move || {
            t = t.wrapping_add(step);
            t
        }
    }

    #[test]
    fn short_pulse_is_none() {
        assert_eq!(pulse_us_to_cm(0), None);
        assert_eq!(pulse_us_to_cm(119), None);
    }

    #[test]
    fn valid_pulse_uses_divisor_58() {
        assert_eq!(pulse_us_to_cm(120), Some(2));
        assert_eq!(pulse_us_to_cm(580), Some(10));
    }

    #[test]
    fn timeout_if_echo_stays_high() {
        let mut trig = Trig;
        let mut echo = Echo::new(&[true]);
        let mut delay = NopDelay;
        let mut now = clock_from(0, TIMEOUT_US + 1);
        let d = measure_cm(&mut trig, &mut echo, &mut delay, &mut now).unwrap();
        assert_eq!(d, None);
    }

    #[test]
    fn timeout_waiting_for_rise() {
        // first is_high false (idle), then is_low stays true (pin stays low)
        let mut trig = Trig;
        let mut echo = Echo::new(&[false, true]);
        let mut delay = NopDelay;
        let mut now = clock_from(0, TIMEOUT_US + 1);
        let d = measure_cm(&mut trig, &mut echo, &mut delay, &mut now).unwrap();
        assert_eq!(d, None);
    }

    #[test]
    fn timeout_during_pulse() {
        // idle not high, rise (is_low false => pin high), pulse stays high
        let mut trig = Trig;
        let mut echo = Echo::new(&[false, false, true]);
        let mut delay = NopDelay;
        let mut now = clock_from(0, TIMEOUT_US + 1);
        let d = measure_cm(&mut trig, &mut echo, &mut delay, &mut now).unwrap();
        assert_eq!(d, None);
    }

    #[test]
    fn valid_echo_pulse_converts() {
        // idle low, wait-rise: is_low true then false, pulse: high then low
        // Sequence of sample() calls:
        // first while is_high: false -> exit
        // second while is_low: !sample; need one true (low) then false (high)
        //   is_low true => sample false (low)
        //   is_low false => sample true (high)
        // third while is_high: sample true then false
        let mut trig = Trig;
        let mut echo = Echo::new(&[false, false, true, true, false]);
        let mut delay = NopDelay;
        let mut now = clock_from(0, 200);
        let d = measure_cm(&mut trig, &mut echo, &mut delay, &mut now).unwrap();
        assert!(d.is_some());
    }
}
