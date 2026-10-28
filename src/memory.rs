use crate::cartridge::Cartridge;
use crate::io::IO;

const ROM_BANK_SIZE: usize = 0x10000; // 64 KiB
const LINEAR_BANK_SIZE: usize = 0x100000; // 1 MiB

const ROM0_START: u32 = 0x20000;
const ROM0_END: u32 = 0x2FFFF;

const ROM1_START: u32 = 0x30000;
const ROM1_END: u32 = 0x3FFFF;

const LINEAR_START: u32 = 0x40000;
const LINEAR_END: u32 = 0xFFFFF;
const FIXED_START: u32 = 0xF0000;
const FIXED_END: u32 = 0xFFFFF;

pub struct Memory {
    cartridge: Cartridge,
    rom_bank0: u8,
    rom_bank1: u8,
    linear_bank: u8,
}

impl Memory {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            rom_bank0: 0,
            rom_bank1: 1,
            linear_bank: 0,
        }
    }

    pub fn set_rom_bank0(&mut self, bank: u8) {
        self.rom_bank0 = bank;
    }

    pub fn set_rom_bank1(&mut self, bank: u8) {
        self.rom_bank1 = bank;
    }

    pub fn set_linear_bank(&mut self, bank: u8) {
        self.linear_bank = bank;
    }

    /// Copia a Memory la selección actual de bancos registrada en IO.
    pub fn sync_bank_registers(&mut self, io: &IO) {
        self.set_rom_bank0(io.rom_bank0());
        self.set_rom_bank1(io.rom_bank1());
        self.set_linear_bank(io.rom_linear_bank());
    }

    pub fn rom_size(&self) -> usize {
        self.cartridge.size()
    }

    pub fn read_byte(&self, address: u32) -> Option<u8> {
        let offset = match address {
            ROM0_START..=ROM0_END => {
                usize::from(self.rom_bank0) * ROM_BANK_SIZE + (address - ROM0_START) as usize
            }
            ROM1_START..=ROM1_END => {
                usize::from(self.rom_bank1) * ROM_BANK_SIZE + (address - ROM1_START) as usize
            }
            FIXED_START..=FIXED_END => {
                self.cartridge.size().checked_sub(ROM_BANK_SIZE)? + (address - FIXED_START) as usize
            }
            LINEAR_START..=LINEAR_END => {
                usize::from(self.linear_bank) * LINEAR_BANK_SIZE
                    + 0x40000
                    + (address - LINEAR_START) as usize
            }
            _ => return None,
        };

        self.cartridge.read_byte(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cartridge() -> Cartridge {
        // ROM de 4 MiB: cada banco de 64 KiB contiene su número.
        let mut rom = vec![0; 4 * 1024 * 1024];

        for (bank, chunk) in rom.chunks_mut(ROM_BANK_SIZE).enumerate() {
            chunk.fill(bank as u8);
        }

        Cartridge::from_bytes(rom).expect("La ROM de prueba debe ser válida")
    }

    #[test]
    fn test_default_rom_bank_windows() {
        let memory = Memory::new(test_cartridge());

        assert_eq!(memory.read_byte(0x20000), Some(0));
        assert_eq!(memory.read_byte(0x2FFFF), Some(0));
        assert_eq!(memory.read_byte(0x30000), Some(1));
        assert_eq!(memory.read_byte(0x3FFFF), Some(1));
    }

    #[test]
    fn test_rom_bank0_can_be_changed() {
        let mut memory = Memory::new(test_cartridge());

        memory.set_rom_bank0(2);

        assert_eq!(memory.read_byte(0x20000), Some(2));
    }

    #[test]
    fn test_linear_bank_can_be_changed() {
        let mut memory = Memory::new(test_cartridge());

        memory.set_linear_bank(1);

        assert_eq!(memory.read_byte(0x40000), Some(20));
    }

    #[test]
    fn fixed_window_maps_last_rom_bank() {
        let memory = Memory::new(test_cartridge());

        assert_eq!(memory.read_byte(0xF0000), Some(63));
        assert_eq!(memory.read_byte(0xFFFFF), Some(63));
    }

    #[test]
    fn test_address_outside_rom_windows_returns_none() {
        let memory = Memory::new(test_cartridge());

        assert_eq!(memory.read_byte(0x10000), None);
        assert_eq!(memory.read_byte(0x100000), None);
    }

    #[test]
    fn test_invalid_bank_returns_none_when_outside_cartridge() {
        let mut memory = Memory::new(test_cartridge());

        memory.set_rom_bank0(100);

        assert_eq!(memory.read_byte(0x20000), None);
    }

    #[test]
    fn test_io_registers_can_update_memory_banks() {
        let mut memory = Memory::new(test_cartridge());
        let mut io = IO::new();

        io.write_port(0xC2, 2);
        io.write_port(0xC3, 3);
        io.write_port(0xC0, 1);

        memory.sync_bank_registers(&io);

        assert_eq!(memory.read_byte(0x20000), Some(2));
        assert_eq!(memory.read_byte(0x30000), Some(3));
        assert_eq!(memory.read_byte(0x40000), Some(20));
    }
}
