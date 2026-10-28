mod cartridge;
mod io;
mod keyboard;
mod keypad;

use cartridge::Cartridge;
use io::IO;
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

    println!("==================================");
    println!("       INFORMACION DEL CARTUCHO");
    println!("==================================");

    match cartridge.known_title() {
        Some(title) => println!("Titulo:             {}", title),
        None => println!("Titulo:             Desconocido"),
    }

    match cartridge.known_serial() {
        Some(serial) => println!("Codigo comercial:   {}", serial),
        None => println!(
            "ID de cartucho:     0x{:02X}",
            cartridge.cartridge_id()
        ),
    }

    println!("Archivo:            {}", path);
    println!("Fabricante (ID):    {}", cartridge.manufacturer_name());
    println!("ID fabricante:      0x{:02X}", cartridge.manufacturer_id());
    println!("Sistema requerido:  {}", cartridge.minimum_requirement());

    println!(
        "Tamano real:        {} bytes ({} MiB)",
        cartridge.size(),
        cartridge.size() / (1024 * 1024)
    );

    match cartridge.declared_rom_size() {
        Some(size) => {
            println!(
                "Tamano declarado:   {} bytes ({} MiB)",
                size,
                size / (1024 * 1024)
            );

            if size != cartridge.size() {
                println!("ADVERTENCIA: el tamano real no coincide con la cabecera.");
            }
        }
        None => println!(
            "Tamano declarado:   codigo desconocido (0x{:02X})",
            cartridge.rom_size_code()
        ),
    }

    println!("Memoria de guardado: {}", cartridge.save_type_name());
    println!(
        "Configuracion extra: 0x{:02X}",
        cartridge.extra_configuration()
    );
    println!(
        "RTC:                 {}",
        if cartridge.has_rtc() { "Si" } else { "No" }
    );
    println!(
        "Checksum almacenado: 0x{:04X}",
        cartridge.stored_checksum()
    );

    println!("\nPrimeros 16 bytes:");

    for address in 0..16.min(cartridge.size()) {
        let byte = cartridge.read_byte(address).unwrap();
        print!("{:02X} ", byte);
    }

    println!("\n\nCabecera (ultimos 10 bytes):");

    for byte in cartridge.header_bytes() {
        print!("{:02X} ", byte);
    }

    println!("\n");

    Ok(())
}

fn run_keyboard_demo() {
    let mut io = IO::new();

    io.keyboard_mut().press(Button::A);
    io.write_port(0xB5, 0x40);

    let value = io.read_port(0xB5);

    println!("Port 0xB5: {:#04X}", value);
}