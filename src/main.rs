mod io;
mod keyboard;
mod keypad;

use io::IO;
use keyboard::Button;

fn main() {
    let mut io = IO::new();

    io.keyboard_mut().press(Button::A);
    io.write_keypad_selection(0x4000);

    let input_value = io.read_keypad_inputs();

    println!("Keypad register: {:#06X}", input_value);
}
