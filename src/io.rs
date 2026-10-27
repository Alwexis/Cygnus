use crate::keyboard::Keyboard;
use crate::keypad::Keypad;

const KEYPAD_PORT: u8 = 0xB5;

pub struct IO {
    keyboard: Keyboard,
    keypad: Keypad,
}

impl IO {
    pub fn new() -> Self {
        Self {
            keyboard: Keyboard::new(),
            keypad: Keypad::new(),
        }
    }

    pub fn keyboard_mut(&mut self) -> &mut Keyboard {
        &mut self.keyboard
    }

    pub fn read_port(&self, port: u8) -> u8 {
        match port {
            KEYPAD_PORT => (self.keypad.read_inputs(&self.keyboard) >> 8) as u8,
            _ => 0xFF,
        }
    }

    pub fn write_port(&mut self, port: u8, value: u8) {
        match port {
            KEYPAD_PORT => {
                self.keypad.write_selection((value as u16) << 8);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::Button;

    #[test]
    fn writing_b5_and_reading_a_pressed_should_return_44() {
        let mut io = IO::new();

        io.keyboard_mut().press(Button::A);
        io.write_port(0xB5, 0x40);

        assert_eq!(io.read_port(0xB5), 0x44);
    }

    #[test]
    fn reading_b5_without_pressed_buttons_should_return_selection() {
        let mut io = IO::new();

        io.write_port(0xB5, 0x20);

        assert_eq!(io.read_port(0xB5), 0x20);
    }

    #[test]
    fn reading_unimplemented_port_should_return_ff() {
        let io = IO::new();

        assert_eq!(io.read_port(0x00), 0xFF);
    }

    #[test]
    fn reading_b5_should_not_include_buttons_from_another_group() {
        let mut io = IO::new();

        io.keyboard_mut().press(Button::A);
        io.keyboard_mut().press(Button::X1);
        io.write_port(0xB5, 0x20);

        assert_eq!(io.read_port(0xB5), 0x21);
    }
}
