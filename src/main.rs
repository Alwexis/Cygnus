mod bus;
mod cartridge;
mod cpu;
mod io;
mod keyboard;
mod keypad;
mod memory;
mod video;

use bus::Bus;
use cartridge::Cartridge;
use cpu::Cpu;
use keyboard::Button;
use minifb::{Scale, Window, WindowOptions};
use std::env;
use std::process;
use std::thread;
use std::time::Duration;

const MAX_INSTRUCTIONS: u64 = 1_000_000;
const DISPLAY_UPDATE_INTERVAL: u64 = 10_000;
const DEBUG_INTERVAL: u64 = 100_000;

fn main() {
    let mut args = env::args();
    let program_name = args.next().unwrap_or_else(|| "cygnus".to_string());

    if let Some(first_argument) = args.next() {
        let headless = first_argument == "--headless";
        let path = if headless {
            args.next()
        } else {
            Some(first_argument)
        };

        if args.next().is_some() || path.is_none() {
            eprintln!("Uso: {} [--headless] <ruta_de_la_rom.wsc>", program_name);
            process::exit(1);
        }

        if let Err(error) = load_cartridge(path.as_deref().unwrap(), !headless) {
            eprintln!("Error al cargar el cartucho: {}", error);
            process::exit(1);
        }

        return;
    }

    run_keyboard_demo();
}

fn load_cartridge(path: &str, show_window: bool) -> Result<(), Box<dyn std::error::Error>> {
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
    bus.write_port(0xC2, 0);

    println!("\nPrueba de RAM:");
    bus.write_byte(0x0010, 0xA5);

    match bus.read_byte(0x0010) {
        Some(byte) => println!("RAM[0x0010]: {:#04X}", byte),
        None => println!("RAM[0x0010]: sin datos"),
    }

    let mut cpu = Cpu::new();
    let mut window = if show_window {
        let mut window = Window::new(
            "Cygnus - WonderSwan Color",
            video::SCREEN_WIDTH,
            video::SCREEN_HEIGHT,
            WindowOptions {
                scale: Scale::X4,
                ..WindowOptions::default()
            },
        )?;
        window.set_target_fps(60);
        Some(window)
    } else {
        None
    };

    println!(
        "\nEjecutando CPU desde el vector de reset físico {:#07X}...",
        Cpu::physical_address(cpu.cs, cpu.ip)
    );

    let mut trace = Vec::with_capacity(32);
    let mut executed = 0u64;
    let result = loop {
        if executed >= MAX_INSTRUCTIONS
            || cpu.halted
            || window.as_ref().is_some_and(|window| !window.is_open())
        {
            break Ok(());
        }
        match cpu.trace_step(&mut bus) {
            Ok(entry) => {
                if trace.len() == 32 {
                    trace.remove(0);
                }
                trace.push(entry);
                executed += 1;
                if executed % DISPLAY_UPDATE_INTERVAL == 0 {
                    if let Some(window) = window.as_mut() {
                        update_window(window, bus.framebuffer(), &bus)?;
                    }
                }
                if executed % DEBUG_INTERVAL == 0 {
                    print_debug_status(executed, &cpu, &bus, entry.opcode);
                }
            }
            Err(error) => break Err(error),
        }
    };
    if let Some(window) = window.as_mut() {
        update_window(window, bus.framebuffer(), &bus)?;
    }
    if let Some(window) = window.as_mut() {
        while window.is_open() {
            window.update();
            thread::sleep(Duration::from_millis(16));
        }
    }

    match result {
        Ok(()) => println!(
            "CPU detenida después de {} instrucciones en {:04X}:{:04X}",
            executed, cpu.cs, cpu.ip
        ),
        Err(error) => println!(
            "CPU detenida después de {} instrucciones por un error en {:04X}:{:04X}: {:?}",
            executed, cpu.cs, cpu.ip, error
        ),
    }

    println!(
        "Bancos al detenerse: C0={:#04X}, C2={:#04X}, C3={:#04X}",
        bus.read_port(0xC0),
        bus.read_port(0xC2),
        bus.read_port(0xC3)
    );
    println!("Últimas instrucciones:");
    for entry in trace {
        println!(
            "  {:04X}:{:04X}  opcode {:#04X}",
            entry.cs, entry.ip, entry.opcode
        );
    }

    Ok(())
}

fn update_window(window: &mut Window, framebuffer: &[u8], bus: &Bus) -> Result<(), minifb::Error> {
    let mut pixels = vec![0; video::SCREEN_WIDTH * video::SCREEN_HEIGHT];
    for (pixel, &color) in pixels.iter_mut().zip(framebuffer) {
        *pixel = bus.palette_color(usize::from(color));
    }

    window.update_with_buffer(&pixels, video::SCREEN_WIDTH, video::SCREEN_HEIGHT)?;
    Ok(())
}

fn print_debug_status(executed: u64, cpu: &Cpu, bus: &Bus, opcode: u8) {
    let framebuffer = bus.framebuffer();
    let nonzero_pixels = framebuffer.iter().filter(|&&pixel| pixel != 0).count();
    let max_color = framebuffer.iter().copied().max().unwrap_or(0);
    let palette = (0..16)
        .map(|index| format!("{:03X}", bus.palette_color(index)))
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "\n[debug] instrucciones={} CPU={:04X}:{:04X} AX={:04X} CX={:04X} DI={:04X} SS:0140={:#04X} flags={:#06X} opcode={:#04X} halted={}",
        executed,
        cpu.cs,
        cpu.ip,
        cpu.ax,
        cpu.cx,
        cpu.di,
        bus.read_byte(Cpu::physical_address(cpu.ss, 0x0140))
            .unwrap_or(0xFF),
        cpu.flags,
        opcode,
        cpu.halted
    );
    println!(
        "[debug] bancos C0={:#04X} C1={:#04X} C2={:#04X} C3={:#04X}",
        bus.read_port(0xC0),
        bus.read_port(0xC1),
        bus.read_port(0xC2),
        bus.read_port(0xC3)
    );
    println!(
        "[debug] video display={:#04X} border={:#04X} map={:#04X} lcd={:#04X} color={:#04X} scroll=({},{}),({}, {})",
        bus.read_port(0x00),
        bus.read_port(0x01),
        bus.read_port(0x07),
        bus.read_port(0x14),
        bus.read_port(0x60),
        bus.read_port(0x10),
        bus.read_port(0x11),
        bus.read_port(0x12),
        bus.read_port(0x13)
    );
    println!(
        "[debug] irq base={:#04X} enable={:#04X} ack={:#04X} scanline={:#04X}",
        bus.read_port(0xB0),
        bus.read_port(0xB2),
        bus.read_port(0xB6),
        bus.read_port(0x02)
    );
    println!(
        "[debug] sprites table={:#04X} tile_base={:#04X} count={:#04X} pixels_no_cero={} max_indice={:#X}",
        bus.read_port(0x04),
        bus.read_port(0x05),
        bus.read_port(0x06),
        nonzero_pixels,
        max_color
    );
    println!("[debug] paleta RGB12: {}", palette);
    let writes = bus
        .video_writes()
        .iter()
        .map(|(port, value)| format!("{:#04X}={:#04X}", port, value))
        .collect::<Vec<_>>()
        .join(" ");
    let counts = bus
        .video_write_counts()
        .iter()
        .enumerate()
        .filter(|(_, count)| **count != 0)
        .map(|(port, count)| format!("{:#04X}:{}", port, count))
        .collect::<Vec<_>>()
        .join(" ");
    println!(
        "[debug] escrituras_video por puerto: {}",
        if counts.is_empty() {
            "(ninguna)"
        } else {
            &counts
        }
    );
    println!(
        "[debug] primeras_escrituras_video: {}",
        if writes.is_empty() {
            "(ninguna)"
        } else {
            &writes
        }
    );
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
