use crate::keyboard::{Button, Keyboard};

pub struct Keypad {
    selection: u16,
}

impl Keypad {
    pub fn new() -> Self {
        Self { selection: 0 }
    }

    pub fn write_selection(&mut self, value: u16) {
        self.selection = value;
    }

    pub fn read_selection(&self) -> u16 {
        self.selection
    }

    pub fn read_inputs(&self, keyboard: &Keyboard) -> u16 {
        // Bits 12 al 14-  selección del grupo.
        let selection = self.selection & 0x7000;

        // Bits 8 al 11 - entradas del grupo seleccionado.
        let mut inputs = 0u16;

        match selection {
            // grupo Y.
            0x1000 => {
                if keyboard.is_pressed(Button::Y1) {
                    inputs |= 1 << 8;
                }
                if keyboard.is_pressed(Button::Y2) {
                    inputs |= 1 << 9;
                }
                if keyboard.is_pressed(Button::Y3) {
                    inputs |= 1 << 10;
                }
                if keyboard.is_pressed(Button::Y4) {
                    inputs |= 1 << 11;
                }
            }

            // grupo x
            0x2000 => {
                if keyboard.is_pressed(Button::X1) {
                    inputs |= 1 << 8;
                }
                if keyboard.is_pressed(Button::X2) {
                    inputs |= 1 << 9;
                }
                if keyboard.is_pressed(Button::X3) {
                    inputs |= 1 << 10;
                }
                if keyboard.is_pressed(Button::X4) {
                    inputs |= 1 << 11;
                }
            }

            // botones especiales
            0x4000 => {
                if keyboard.is_pressed(Button::Start) {
                    inputs |= 1 << 9;
                }
                if keyboard.is_pressed(Button::A) {
                    inputs |= 1 << 10;
                }
                if keyboard.is_pressed(Button::B) {
                    inputs |= 1 << 11;
                }
            }

            _ => {}
        }

        selection | inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_selection_should_update_selection() {
        let mut keypad = Keypad::new();

        keypad.write_selection(0x2000);

        assert_eq!(keypad.selection, 0x2000);
    }

    #[test]
    fn write_selection_should_overwrite_previous_value() {
        let mut keypad = Keypad::new();

        keypad.write_selection(0x1000);
        keypad.write_selection(0x3000);

        assert_eq!(keypad.selection, 0x3000);
    }

    #[test]
    fn read_selection_should_return_current_selection() {
        let mut keypad = Keypad::new();

        keypad.write_selection(0x3000);

        assert_eq!(keypad.read_selection(), 0x3000);
    }

    #[test]
    fn reading_x_group_should_return_pressed_x1() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x2000);
        keyboard.press(Button::X1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x2100);
    }

    #[test]
    fn reading_x_group_should_return_multiple_pressed_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x2000);
        keyboard.press(Button::X1);
        keyboard.press(Button::X3);

        assert_eq!(keypad.read_inputs(&keyboard), 0x2500);
    }

    #[test]
    fn reading_y_group_should_return_pressed_y1() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x1000);
        keyboard.press(Button::Y1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x1100);
    }

    #[test]
    fn reading_x_group_should_not_read_y_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x2000);
        keyboard.press(Button::Y1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x2000);
    }

    #[test]
    fn reading_special_group_should_return_pressed_a() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x4000);
        keyboard.press(Button::A);

        assert_eq!(keypad.read_inputs(&keyboard), 0x4400);
    }

    #[test]
    fn reading_special_group_should_return_pressed_start() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x4000);
        keyboard.press(Button::Start);

        assert_eq!(keypad.read_inputs(&keyboard), 0x4200);
    }

    #[test]
    fn reading_special_group_should_not_read_x_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x4000);
        keyboard.press(Button::X1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x4000);
    }

    #[test]
    fn selecting_multiple_groups_should_not_read_inputs() {
        let mut keypad = Keypad::new();
        let mut keyboard = Keyboard::new();

        keypad.write_selection(0x3000);
        keyboard.press(Button::X1);
        keyboard.press(Button::Y1);

        assert_eq!(keypad.read_inputs(&keyboard), 0x3000);
    }
}
