use embedded_hal::i2c::I2c;

use crate::bus::write_reg;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LedColor {
    Red = 1,
    Green = 2,
    Blue = 3,
    Cyan = 4,
    Purple = 5,
    White = 6,
    Yellow = 7,
    Black = 8,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LedRgb {
    /// Right RGB (PWM 0x08 / 0x07 / 0x06).
    Led1 = 1,
    /// Left RGB (PWM 0x09 / 0x0A / 0x05).
    Led2 = 2,
}

#[repr(u8)]
enum RightLed {
    Red = 0x08,
    Green = 0x07,
    Blue = 0x06,
}

#[repr(u8)]
enum LeftLed {
    Red = 0x09,
    Green = 0x0A,
    Blue = 0x05,
}

pub fn color_to_pwm(color: LedColor) -> (u8, u8, u8) {
    match color {
        LedColor::Red => (0, 255, 255),
        LedColor::Green => (255, 0, 255),
        LedColor::Blue => (255, 255, 0),
        LedColor::Cyan => (255, 0, 0),
        LedColor::Purple => (0, 255, 0),
        LedColor::White => (0, 0, 0),
        LedColor::Yellow => (0, 0, 255),
        LedColor::Black => (255, 255, 255),
    }
}

pub fn set_color<I2C>(i2c: &mut I2C, led: LedRgb, color: LedColor) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    set_rgb(i2c, led, color_to_pwm(color))
}

pub fn set_rgb<I2C>(i2c: &mut I2C, led: LedRgb, rgb: (u8, u8, u8)) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    let (r, g, b) = rgb;

    match led {
        LedRgb::Led1 => {
            write_reg(i2c, RightLed::Red as u8, r)?;
            write_reg(i2c, RightLed::Green as u8, g)?;
            write_reg(i2c, RightLed::Blue as u8, b)?;
        }
        LedRgb::Led2 => {
            write_reg(i2c, LeftLed::Red as u8, r)?;
            write_reg(i2c, LeftLed::Green as u8, g)?;
            write_reg(i2c, LeftLed::Blue as u8, b)?;
        }
    }

    Ok(())
}

pub fn disable<I2C>(i2c: &mut I2C) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    write_reg(i2c, RightLed::Red as u8, 255)?;
    write_reg(i2c, RightLed::Blue as u8, 255)?;
    write_reg(i2c, RightLed::Green as u8, 255)?;
    write_reg(i2c, LeftLed::Red as u8, 255)?;
    write_reg(i2c, LeftLed::Blue as u8, 255)?;
    write_reg(i2c, LeftLed::Green as u8, 255)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::I2C_ADDR;
    use embedded_hal_mock::eh1::i2c::{Mock, Transaction};

    fn run(pairs: &[(u8, u8)], f: impl FnOnce(&mut Mock)) {
        let tx: Vec<_> = pairs
            .iter()
            .map(|(r, v)| Transaction::write(I2C_ADDR, vec![*r, *v]))
            .collect();
        let mut i2c = Mock::new(&tx);
        f(&mut i2c);
        i2c.done();
    }

    #[test]
    fn color_table_is_common_anode() {
        assert_eq!(color_to_pwm(LedColor::Red), (0, 255, 255));
        assert_eq!(color_to_pwm(LedColor::Green), (255, 0, 255));
        assert_eq!(color_to_pwm(LedColor::Blue), (255, 255, 0));
        assert_eq!(color_to_pwm(LedColor::Cyan), (255, 0, 0));
        assert_eq!(color_to_pwm(LedColor::Purple), (0, 255, 0));
        assert_eq!(color_to_pwm(LedColor::White), (0, 0, 0));
        assert_eq!(color_to_pwm(LedColor::Yellow), (0, 0, 255));
        assert_eq!(color_to_pwm(LedColor::Black), (255, 255, 255));
    }

    #[test]
    fn set_color_led1_red_writes_right_channels() {
        run(&[(0x08, 0), (0x07, 255), (0x06, 255)], |i2c| {
            set_color(i2c, LedRgb::Led1, LedColor::Red).unwrap();
        });
    }

    #[test]
    fn set_rgb_led2_writes_left_channels() {
        run(&[(0x09, 1), (0x0A, 2), (0x05, 3)], |i2c| {
            set_rgb(i2c, LedRgb::Led2, (1, 2, 3)).unwrap();
        });
    }

    #[test]
    fn disable_writes_255_to_all_six_channels() {
        run(
            &[
                (0x08, 255),
                (0x06, 255),
                (0x07, 255),
                (0x09, 255),
                (0x05, 255),
                (0x0A, 255),
            ],
            |i2c| {
                disable(i2c).unwrap();
            },
        );
    }
}
