use crate::keyboard::Keyboard;
use crate::keypad::Keypad;

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

    pub fn write_keypad_selection(&mut self, value: u8) {
        self.keypad.write_selection(value);
    }

    pub fn read_keypad_inputs(&self) -> u8 {
        self.keypad.read_inputs(&self.keyboard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::Button;

    #[test]
    fn reading_special_group_should_return_pressed_a() {
        let mut io = IO::new();

        io.keyboard_mut().press(Button::A);
        io.write_keypad_selection(0x40);

        assert_eq!(io.read_keypad_inputs(), 0x44);
    }

    #[test]
    fn reading_x_group_should_not_read_special_buttons() {
        let mut io = IO::new();

        io.keyboard_mut().press(Button::A);
        io.write_keypad_selection(0x20);

        assert_eq!(io.read_keypad_inputs(), 0x20);
    }
}
