use embedded_hal::i2c::I2c;

pub const I2C_ADDR: u8 = 0x30;

pub fn write_reg<I2C>(i2c: &mut I2C, reg: u8, value: u8) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    i2c.write(I2C_ADDR, &[reg, value])
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal_mock::eh1::i2c::{Mock, Transaction};

    #[test]
    fn write_reg_sends_address_register_and_value() {
        let mut i2c = Mock::new(&[Transaction::write(I2C_ADDR, vec![0x08, 0xAA])]);
        write_reg(&mut i2c, 0x08, 0xAA).unwrap();
        i2c.done();
    }
}
