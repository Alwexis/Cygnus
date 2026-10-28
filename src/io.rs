use crate::keyboard::Keyboard;
use crate::keypad::Keypad;

const KEYPAD_PORT: u8 = 0xB5;

const ROM_LINEAR_BANK_PORT: u8 = 0xC0;
const RAM_BANK_PORT: u8 = 0xC1;
const ROM_BANK0_PORT: u8 = 0xC2;
const ROM_BANK1_PORT: u8 = 0xC3;

pub struct IO {
    keyboard: Keyboard,
    keypad: Keypad,

    rom_linear_bank: u8,
    ram_bank: u8,
    rom_bank0: u8,
    rom_bank1: u8,
}

impl IO {
    pub fn new() -> Self {
        Self::new_with_linear_bank(0)
    }

    pub fn new_with_linear_bank(linear_bank: u8) -> Self {
        Self {
            keyboard: Keyboard::new(),
            keypad: Keypad::new(),
            rom_linear_bank: linear_bank,
            ram_bank: 0,
            rom_bank0: 0,
            rom_bank1: 1,
        }
    }

    pub fn keyboard_mut(&mut self) -> &mut Keyboard {
        &mut self.keyboard
    }

    pub fn rom_linear_bank(&self) -> u8 {
        self.rom_linear_bank
    }

    pub fn ram_bank(&self) -> u8 {
        self.ram_bank
    }

    pub fn rom_bank0(&self) -> u8 {
        self.rom_bank0
    }

    pub fn rom_bank1(&self) -> u8 {
        self.rom_bank1
    }

    pub fn read_port(&self, port: u8) -> u8 {
        match port {
            KEYPAD_PORT => (self.keypad.read_inputs(&self.keyboard) >> 8) as u8,
            ROM_LINEAR_BANK_PORT => self.rom_linear_bank,
            RAM_BANK_PORT => self.ram_bank,
            ROM_BANK0_PORT => self.rom_bank0,
            ROM_BANK1_PORT => self.rom_bank1,
            _ => 0xFF,
        }
    }

    pub fn write_port(&mut self, port: u8, value: u8) {
        match port {
            KEYPAD_PORT => {
                self.keypad.write_selection((value as u16) << 8);
            }
            ROM_LINEAR_BANK_PORT => self.rom_linear_bank = value,
            RAM_BANK_PORT => self.ram_bank = value,
            ROM_BANK0_PORT => self.rom_bank0 = value,
            ROM_BANK1_PORT => self.rom_bank1 = value,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rom_bank_registers_can_be_written_and_read() {
        let mut io = IO::new();

        io.write_port(0xC0, 3);
        io.write_port(0xC2, 5);
        io.write_port(0xC3, 7);

        assert_eq!(io.read_port(0xC0), 3);
        assert_eq!(io.read_port(0xC2), 5);
        assert_eq!(io.read_port(0xC3), 7);
    }

    #[test]
    fn test_ram_bank_register_can_be_written_and_read() {
        let mut io = IO::new();

        io.write_port(0xC1, 9);

        assert_eq!(io.read_port(0xC1), 9);
        assert_eq!(io.ram_bank(), 9);
    }

    #[test]
    fn test_unknown_port_reads_ff() {
        let io = IO::new();

        assert_eq!(io.read_port(0x00), 0xFF);
    }

    #[test]
    fn test_keypad_port_still_works() {
        let mut io = IO::new();

        io.keyboard_mut().press(crate::keyboard::Button::A);
        io.write_port(0xB5, 0x40);

        assert_eq!(io.read_port(0xB5), 0x44);
    }
}
