use crate::cartridge::Cartridge;
use crate::io::IO;
use crate::memory::Memory;

const RAM_START: u32 = 0x0000;
const RAM_END: u32 = 0x3FFF;

const RAM_SIZE: usize = (RAM_END - RAM_START + 1) as usize;

pub struct Bus {
    memory: Memory,
    io: IO,
    ram: [u8; RAM_SIZE],
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            memory: Memory::new(cartridge),
            io: IO::new(),
            ram: [0; RAM_SIZE],
        }
    }

    /// lee un byte de RAM o de las regiones ROM gestionadas por Memory
    pub fn read_byte(&self, address: u32) -> Option<u8> {
        match address {
            RAM_START..=RAM_END => Some(self.ram[(address - RAM_START) as usize]),
            _ => self.memory.read_byte(address),
        }
    }

    /// escribe en RAM
    pub fn write_byte(&mut self, address: u32, value: u8) -> bool {
        match address {
            RAM_START..=RAM_END => {
                self.ram[(address - RAM_START) as usize] = value;
                true
            }
            _ => false,
        }
    }

    /// Lee un puerto de e/s.
    pub fn read_port(&self, port: u8) -> u8 {
        self.io.read_port(port)
    }

    /// Escribe un puerto de e/s y aplica automáticamente los cambios
    /// de selección de bancos a Memory
    pub fn write_port(&mut self, port: u8, value: u8) {
        self.io.write_port(port, value);

        match port {
            0xC0 | 0xC2 | 0xC3 => {
                self.memory.sync_bank_registers(&self.io);
            }
            _ => {}
        }
    }

    /// permite acceder al teclado para pruebas y para la interfas
    pub fn keyboard_mut(&mut self) -> &mut crate::keyboard::Keyboard {
        self.io.keyboard_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ROM_SIZE: usize = 4 * 1024 * 1024;
    const TEST_BANK_SIZE: usize = 0x10000;

    fn test_cartridge() -> Cartridge {
        let mut rom = vec![0; TEST_ROM_SIZE];

        for (bank, chunk) in rom.chunks_mut(TEST_BANK_SIZE).enumerate() {
            chunk.fill(bank as u8);
        }

        Cartridge::from_bytes(rom).expect("La ROM de prueba debe ser válida")
    }

    #[test]
    fn test_ram_can_be_written_and_read() {
        let mut bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x0010), Some(0));

        assert!(bus.write_byte(0x0010, 0xA5));

        assert_eq!(bus.read_byte(0x0010), Some(0xA5));
    }

    #[test]
    fn test_ram_addresses_are_independent() {
        let mut bus = Bus::new(test_cartridge());

        assert!(bus.write_byte(0x0010, 0x12));
        assert!(bus.write_byte(0x0020, 0x34));

        assert_eq!(bus.read_byte(0x0010), Some(0x12));
        assert_eq!(bus.read_byte(0x0020), Some(0x34));
    }

    #[test]
    fn test_rom_can_be_read() {
        let bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x20000), Some(0));
        assert_eq!(bus.read_byte(0x30000), Some(1));
    }

    #[test]
    fn test_bank_port_changes_rom_reading_automatically() {
        let mut bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x20000), Some(0));

        bus.write_port(0xC2, 2);

        assert_eq!(bus.read_byte(0x20000), Some(2));
        assert_eq!(bus.read_port(0xC2), 2);
    }

    #[test]
    fn test_linear_bank_port_changes_rom_reading_automatically() {
        let mut bus = Bus::new(test_cartridge());

        bus.write_port(0xC0, 1);

        // calcula el desplazamiento 1 MiB + 0x40000.
        assert_eq!(bus.read_byte(0x40000), Some(20));
    }

    #[test]
    fn test_rom_cannot_be_written_through_memory_bus() {
        let mut bus = Bus::new(test_cartridge());

        assert!(!bus.write_byte(0x20000, 0xAA));
        assert_eq!(bus.read_byte(0x20000), Some(0));
    }

    #[test]
    fn test_unmapped_address_returns_none() {
        let bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x10000), None);
        assert_eq!(bus.read_byte(0x100000), None);
    }

    #[test]
    fn test_keypad_port_still_works_through_bus() {
        let mut bus = Bus::new(test_cartridge());

        bus.keyboard_mut().press(crate::keyboard::Button::A);
        bus.write_port(0xB5, 0x40);

        assert_eq!(bus.read_port(0xB5), 0x44);
    }
}
