mod cartridge;
mod io;
mod keyboard;
mod keypad;
mod memory;

use cartridge::Cartridge;
use io::IO;
use keyboard::Button;
use memory::Memory;
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

    // pra testear
    println!(
        "Byte físico ROM[0x20000]: {:#04X}",
        cartridge.read_byte(0x20000).unwrap_or(0xFF)
    );

    println!("Cartucho cargado: {}", path);
    println!("Tamaño: {} bytes", cartridge.size());

    let mut io = IO::new();
    let mut memory = Memory::new(cartridge);

    // Configuramos los registros de bancos mediante los puertos de E/S.
    io.write_port(0xC0, 0);
    io.write_port(0xC2, 0);
    io.write_port(0xC3, 1);

    // Aplicamos los valores registrados al mapa de memoria.
    memory.sync_bank_registers(&io);

    println!("\nRegistros de bancos:");
    println!("Puerto 0xC0: {:#04X}", io.read_port(0xC0));
    println!("Puerto 0xC2: {:#04X}", io.read_port(0xC2));
    println!("Puerto 0xC3: {:#04X}", io.read_port(0xC3));

    println!("\nLectura mediante el mapa de memoria:");

    for address in 0x20000..0x20010 {
        match memory.read_byte(address) {
            Some(byte) => println!("{:#07X}: {:#04X}", address, byte),
            None => println!("{:#07X}: sin datos", address),
        }
    }

    // Cambiamos el banco visible en 0x20000.
    io.write_port(0xC2, 2);
    memory.sync_bank_registers(&io);

    println!("\nTras seleccionar el banco 2 en el puerto 0xC2:");
    match memory.read_byte(0x20000) {
        Some(byte) => println!("0x20000: {:#04X}", byte),
        None => println!("0x20000: sin datos"),
    }

    Ok(())
}

fn run_keyboard_demo() {
    let mut io = IO::new();

    io.keyboard_mut().press(Button::A);
    io.write_port(0xB5, 0x40);

    let value = io.read_port(0xB5);

    println!("Port 0xB5: {:#04X}", value);
}
