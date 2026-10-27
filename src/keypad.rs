use crate::keyboard::{Button, Keyboard};

pub struct Keypad {
    selection: u8,
}

impl Keypad {
    pub fn new() -> Self {
        Self { selection: 0 }
    }

    pub fn write_selection(&mut self, value: u8) {
        self.selection = value;
    }

    pub fn read_selection(&self) -> u8 {
        self.selection
    }

    pub fn read_inputs(&self, keyboard: &Keyboard) -> u8 {
        // conservamos los bits 4, 5 y 6 que indican el grupo seleccionado.
        let selection = self.selection & 0x70;

        // los 4 bits inferiores significan las entradas leídas.
        let mut inputs = 0;

        match selection {
            // grupo y
            0x10 => {
                if keyboard.is_pressed(Button::Y1) {
                    inputs |= 1 << 0;
                }

                if keyboard.is_pressed(Button::Y2) {
                    inputs |= 1 << 1;
                }

                if keyboard.is_pressed(Button::Y3) {
                    inputs |= 1 << 2;
                }

                if keyboard.is_pressed(Button::Y4) {
                    inputs |= 1 << 3;
                }
            }

            // grupo x
            0x20 => {
                if keyboard.is_pressed(Button::X1) {
                    inputs |= 1 << 0;
                }

                if keyboard.is_pressed(Button::X2) {
                    inputs |= 1 << 1;
                }

                if keyboard.is_pressed(Button::X3) {
                    inputs |= 1 << 2;
                }

                if keyboard.is_pressed(Button::X4) {
                    inputs |= 1 << 3;
                }
            }

            // botones especiales
            0x40 => {
                if keyboard.is_pressed(Button::Start) {
                    inputs |= 1 << 1;
                }

                if keyboard.is_pressed(Button::A) {
                    inputs |= 1 << 2;
                }

                if keyboard.is_pressed(Button::B) {
                    inputs |= 1 << 3;
                }
            }

            _ => {}
        }

        // lecutra combina la seleccion y estado de los inputs
        selection | inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_selection_should_update_selection() {
        let mut keypad = Keypad::new();
        keypad.write_selection(0x20);
        assert_eq!(keypad.selection, 32);
    }

    #[test]
    fn write_selection_should_overwrite_previous_value() {
        let mut keypad = Keypad::new();
        keypad.write_selection(0x10);
        keypad.write_selection(0x30);
        assert_eq!(keypad.selection, 48);
    }

    #[test]
    fn read_selection_should_return_current_selection() {
        let mut keypad = Keypad::new();
        keypad.write_selection(0x30);
        assert_eq!(keypad.read_selection(), 48);
    }

    #[test]
    fn reading_x_group_should_return_pressed_x1() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x20);
        keyboard.press(Button::X1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x21);
    }

    #[test]
    fn reading_x_group_should_return_multiple_pressed_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x20);
        keyboard.press(Button::X1);
        keyboard.press(Button::X3);

        assert_eq!(keypad.read_inputs(&keyboard), 0x25);
    }

    #[test]
    fn reading_y_group_should_return_pressed_y1() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x10);
        keyboard.press(Button::Y1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x11);
    }

    #[test]
    fn reading_x_group_should_not_read_y_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x20);
        keyboard.press(Button::Y1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x20);
    }

    #[test]
    fn reading_special_group_should_return_pressed_a() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x40);
        keyboard.press(Button::A);

        assert_eq!(keypad.read_inputs(&keyboard), 0x44);
    }

    #[test]
    fn reading_special_group_should_return_pressed_start() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x40);
        keyboard.press(Button::Start);

        assert_eq!(keypad.read_inputs(&keyboard), 0x42);
    }

    #[test]
    fn reading_special_group_should_not_read_x_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x40);
        keyboard.press(Button::X1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x40);
    }

    #[test]
    fn selecting_multiple_groups_should_not_read_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x30);
        keyboard.press(Button::X1);
        keyboard.press(Button::Y1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x30);
    }
}
