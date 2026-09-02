use embedded_hal::digital::InputPin;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LineTrackingSensor {
    Both = 0,
    Left = 1,
    Right = 2,
    None = 3,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LineTrackingError<LeftError, RightError> {
    Left(LeftError),
    Right(RightError),
}

pub fn read<LeftPin, RightPin>(
    left: &mut LeftPin,
    right: &mut RightPin,
) -> Result<LineTrackingSensor, LineTrackingError<LeftPin::Error, RightPin::Error>>
where
    LeftPin: InputPin,
    RightPin: InputPin,
{
    let left_high = left.is_high().map_err(LineTrackingError::Left)? as u8;
    let right_high = right.is_high().map_err(LineTrackingError::Right)? as u8;

    Ok(match left_high | (right_high << 1) {
        0 => LineTrackingSensor::Both,
        1 => LineTrackingSensor::Left,
        2 => LineTrackingSensor::Right,
        _ => LineTrackingSensor::None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowCmd {
    Stop,
    Forward,
    SteerLeft,
    SteerRight,
}

/// Open-loop follow. `obstacle` is ultrasonic halt (no RGB/IR).
pub fn follow_cmd(line: LineTrackingSensor, obstacle: bool) -> FollowCmd {
    if obstacle {
        return FollowCmd::Stop;
    }
    match line {
        LineTrackingSensor::Both => FollowCmd::Forward,
        LineTrackingSensor::Left => FollowCmd::SteerLeft,
        LineTrackingSensor::Right => FollowCmd::SteerRight,
        LineTrackingSensor::None => FollowCmd::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::digital::ErrorType;

    struct StubPin {
        high: bool,
    }

    impl ErrorType for StubPin {
        type Error = Infallible;
    }

    impl InputPin for StubPin {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(self.high)
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(!self.is_high()?)
        }
    }

    fn pin(high: bool) -> StubPin {
        StubPin { high }
    }

    #[test]
    fn both_low_is_both_on_line() {
        let mut l = pin(false);
        let mut r = pin(false);
        assert_eq!(read(&mut l, &mut r).unwrap(), LineTrackingSensor::Both);
    }

    #[test]
    fn left_high_right_low_is_left() {
        let mut l = pin(true);
        let mut r = pin(false);
        assert_eq!(read(&mut l, &mut r).unwrap(), LineTrackingSensor::Left);
    }

    #[test]
    fn left_low_right_high_is_right() {
        let mut l = pin(false);
        let mut r = pin(true);
        assert_eq!(read(&mut l, &mut r).unwrap(), LineTrackingSensor::Right);
    }

    #[test]
    fn both_high_is_none() {
        let mut l = pin(true);
        let mut r = pin(true);
        assert_eq!(read(&mut l, &mut r).unwrap(), LineTrackingSensor::None);
    }

    #[test]
    fn follow_stops_on_obstacle_or_none() {
        assert_eq!(follow_cmd(LineTrackingSensor::Both, true), FollowCmd::Stop);
        assert_eq!(follow_cmd(LineTrackingSensor::None, false), FollowCmd::Stop);
        assert_eq!(follow_cmd(LineTrackingSensor::Both, false), FollowCmd::Forward);
        assert_eq!(follow_cmd(LineTrackingSensor::Left, false), FollowCmd::SteerLeft);
        assert_eq!(follow_cmd(LineTrackingSensor::Right, false), FollowCmd::SteerRight);
    }
}
