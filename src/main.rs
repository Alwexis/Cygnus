mod io;
mod keyboard;
mod keypad;

use io::IO;
use keyboard::Button;

fn main() {
    let mut io = IO::new();

    io.keyboard_mut().press(Button::A);
    io.write_port(0xB5, 0x40);

    let value = io.read_port(0xB5);

    println!("Port 0xB5: {:#04X}", value);
}
