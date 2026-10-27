mod keyboard;
mod keypad;

fn main() {
    let mut kbrd = keyboard::Keyboard::new();
    kbrd.press(keyboard::Button::A);
    println!("A pressed: {}", kbrd.is_pressed(keyboard::Button::A));
}
