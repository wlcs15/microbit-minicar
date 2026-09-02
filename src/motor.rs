use embedded_hal::i2c::I2c;

use crate::bus::write_reg;

/// HS1002 PWM expander motor channels.
///
/// Direction is a full-scale (0 or 255) pin; speed is the PWM channel.
/// This matches the kit MicroPython `Motor_L` / `Motor_R` register pair
/// without extra unused writes.
///
/// Physical left vs right wheel is confirmed later with a serial spin test.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Motor {
    /// Channels 1 (PWM) and 2 (direction 0/255).
    A,
    /// Channels 4 (PWM) and 3 (direction 0/255).
    B,
}

const DIR_LOW: u8 = 0;
const DIR_HIGH: u8 = 255;

pub fn stop<I2C>(i2c: &mut I2C) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    set(i2c, 0, Motor::A, Direction::Forward)?;
    set(i2c, 0, Motor::B, Direction::Forward)?;
    Ok(())
}

pub fn set<I2C>(
    i2c: &mut I2C,
    speed: u8,
    motor: Motor,
    direction: Direction,
) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    match motor {
        Motor::A => {
            let dir = match direction {
                Direction::Forward => DIR_HIGH,
                Direction::Backward => DIR_LOW,
            };
            write_reg(i2c, 0x01, speed)?;
            write_reg(i2c, 0x02, dir)?;
        }
        Motor::B => {
            let dir = match direction {
                Direction::Forward => DIR_LOW,
                Direction::Backward => DIR_HIGH,
            };
            write_reg(i2c, 0x03, dir)?;
            write_reg(i2c, 0x04, speed)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::I2C_ADDR;
    use embedded_hal_mock::eh1::i2c::{Mock, Transaction};

    fn writes(pairs: &[(u8, u8)]) -> Vec<Transaction> {
        pairs
            .iter()
            .map(|(reg, val)| Transaction::write(I2C_ADDR, vec![*reg, *val]))
            .collect()
    }

    fn expect(pairs: &[(u8, u8)], f: impl FnOnce(&mut Mock)) {
        let mut i2c = Mock::new(&writes(pairs));
        f(&mut i2c);
        i2c.done();
    }

    #[test]
    fn motor_a_forward_pwm_and_dir_high() {
        expect(&[(0x01, 200), (0x02, 255)], |i2c| {
            set(i2c, 200, Motor::A, Direction::Forward).unwrap();
        });
    }

    #[test]
    fn motor_a_backward_pwm_and_dir_low() {
        expect(&[(0x01, 200), (0x02, 0)], |i2c| {
            set(i2c, 200, Motor::A, Direction::Backward).unwrap();
        });
    }

    #[test]
    fn motor_b_forward_dir_low_and_pwm() {
        expect(&[(0x03, 0), (0x04, 200)], |i2c| {
            set(i2c, 200, Motor::B, Direction::Forward).unwrap();
        });
    }

    #[test]
    fn motor_b_backward_dir_high_and_pwm() {
        expect(&[(0x03, 255), (0x04, 200)], |i2c| {
            set(i2c, 200, Motor::B, Direction::Backward).unwrap();
        });
    }

    #[test]
    fn stop_is_both_motors_forward_at_zero() {
        expect(&[(0x01, 0), (0x02, 255), (0x03, 0), (0x04, 0)], |i2c| {
            stop(i2c).unwrap();
        });
    }
}
