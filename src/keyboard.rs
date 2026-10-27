// habiltiamos para copiarlo clonarlo etc
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    // accion
    A,
    B,
    Start,

    // X - cuatro entradas independientes
    X1, // arriba
    X2, // derecha
    X3, // abajo
    X4, // izquierda

    // Y- otras cuatro entradas independientes
    Y1, // arriba
    Y2, // derecha
    Y3, // abajo
    Y4, // izquierda
}

pub struct Keyboard {
    // 1 bool por cada control:
    // false = released, true = pressed.
    // Hay 11 variantes en Button, así que necesitamos 11 posiciones.
    buttons: [bool; 11],
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            // todos los controles comienzan released.
            buttons: [false; 11],
        }
    }

    pub fn press(&mut self, button: Button) {
        let index = Self::button_index(button);
        self.buttons[index] = true;
    }

    pub fn release(&mut self, button: Button) {
        let index = Self::button_index(button);
        self.buttons[index] = false;
    }

    // traduce las vairantes de Button a una posición del array.
    // cada control tiene un índice único.
    fn button_index(button: Button) -> usize {
        match button {
            Button::A => 0,
            Button::B => 1,
            Button::Start => 2,

            Button::X1 => 3,
            Button::X2 => 4,
            Button::X3 => 5,
            Button::X4 => 6,

            Button::Y1 => 7,
            Button::Y2 => 8,
            Button::Y3 => 9,
            Button::Y4 => 10,
        }
    }

    pub fn is_pressed(&self, button: Button) -> bool {
        let index = Self::button_index(button);
        self.buttons[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_keyboard_should_have_all_buttons_released() {
        let keyboard = Keyboard::new();

        for button in [
            Button::A,
            Button::B,
            Button::Start,
            Button::X1,
            Button::X2,
            Button::X3,
            Button::X4,
            Button::Y1,
            Button::Y2,
            Button::Y3,
            Button::Y4,
        ] {
            assert!(!keyboard.is_pressed(button));
        }
    }

    #[test]
    fn pressing_a_only_presses_a() {
        let mut keyboard = Keyboard::new();

        keyboard.press(Button::A);

        assert!(keyboard.is_pressed(Button::A));
        assert!(!keyboard.is_pressed(Button::B));
    }

    #[test]
    fn releasing_a_does_not_release_b() {
        let mut keyboard = Keyboard::new();

        keyboard.press(Button::A);
        keyboard.press(Button::B);
        keyboard.release(Button::A);

        assert!(!keyboard.is_pressed(Button::A));
        assert!(keyboard.is_pressed(Button::B));
    }

    #[test]
    fn pressing_a_twice_does_not_change_state() {
        let mut keyboard = Keyboard::new();

        keyboard.press(Button::A);
        keyboard.press(Button::A);

        assert!(keyboard.is_pressed(Button::A));
    }

    #[test]
    fn pressing_x1_does_not_press_y1() {
        let mut keyboard = Keyboard::new();

        keyboard.press(Button::X1);

        assert!(keyboard.is_pressed(Button::X1));
        assert!(!keyboard.is_pressed(Button::Y1));
    }
}
