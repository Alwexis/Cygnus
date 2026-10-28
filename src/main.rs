mod bus;
mod cartridge;
mod io;
mod keyboard;
mod keypad;
mod memory;

use bus::Bus;
use cartridge::Cartridge;
use keyboard::Button;
use std::env;
use std::process;

fn main() {
    let mut args = env::args();
    let program_name = args.next().unwrap_or_else(|| "cygnus".to_string());

    if let Some(path) = args.next() {
        if args.next().is_some() {
            eprintln!("Uso: {} [ruta_de_la_rom.wsc]", program_name);
            process::exit(1);
        }

        if let Err(error) = load_cartridge(&path) {
            eprintln!("Error al cargar el cartucho: {}", error);
            process::exit(1);
        }

        return;
    }

    run_keyboard_demo();
}

fn load_cartridge(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cartridge = Cartridge::load(path)?;

    println!("Cartucho cargado: {}", path);
    println!("Tamaño: {} bytes", cartridge.size());

    let mut bus = Bus::new(cartridge);

    println!("\nLectura de ROM mediante el bus:");

    for address in 0x20000..0x20010 {
        match bus.read_byte(address) {
            Some(byte) => println!("{:#07X}: {:#04X}", address, byte),
            None => println!("{:#07X}: sin datos", address),
        }
    }

    println!("\nSeleccionando el banco ROM 2 mediante el puerto 0xC2...");
    bus.write_port(0xC2, 2);

    println!("Puerto 0xC2: {:#04X}", bus.read_port(0xC2));

    match bus.read_byte(0x20000) {
        Some(byte) => println!("Lectura de 0x20000: {:#04X}", byte),
        None => println!("Lectura de 0x20000: sin datos"),
    }

    println!("\nPrueba de RAM:");
    bus.write_byte(0x0010, 0xA5);

    match bus.read_byte(0x0010) {
        Some(byte) => println!("RAM[0x0010]: {:#04X}", byte),
        None => println!("RAM[0x0010]: sin datos"),
    }

    Ok(())
}

fn run_keyboard_demo() {
    let mut bus = Bus::new(
        Cartridge::from_bytes(vec![0; 0x10000])
            .expect("La ROM mínima de demostración debe ser válida"),
    );

    bus.keyboard_mut().press(Button::A);
    bus.write_port(0xB5, 0x40);

    println!("Port 0xB5: {:#04X}", bus.read_port(0xB5));
}
