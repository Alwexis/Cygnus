use crate::cartridge::Cartridge;

const ROM_BANK_SIZE: usize = 0x10000; // 64 KiB
const LINEAR_BANK_SIZE: usize = 0x100000; // 1 MiB

const ROM0_START: u32 = 0x20000;
const ROM0_END: u32 = 0x2FFFF;

const ROM1_START: u32 = 0x30000;
const ROM1_END: u32 = 0x3FFFF;

const LINEAR_START: u32 = 0x40000;
const LINEAR_END: u32 = 0xFFFFF;

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
            // para prueba
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

    /// lee un byte de las ventanas ROM implementadas
    /// None significa que la dirección no pertenece a una ventana
    /// ROM implementada o que el desplazamiento excede el archivo
    pub fn read_byte(&self, address: u32) -> Option<u8> {
        let offset = match address {
            ROM0_START..=ROM0_END => {
                usize::from(self.rom_bank0) * ROM_BANK_SIZE + (address - ROM0_START) as usize
            }

            ROM1_START..=ROM1_END => {
                usize::from(self.rom_bank1) * ROM_BANK_SIZE + (address - ROM1_START) as usize
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

    fn test_memory() -> Memory {
        let mut rom = vec![0; 4 * 1024 * 1024];

        for (bank, chunk) in rom.chunks_mut(ROM_BANK_SIZE).enumerate() {
            chunk.fill(bank as u8);
        }

        Memory::new(Cartridge::from_bytes(rom).unwrap())
    }

    #[test]
    fn should_read_from_rom_bank0() {
        let memory = test_memory();

        assert_eq!(memory.read_byte(0x20000), Some(0));
        assert_eq!(memory.read_byte(0x2FFFF), Some(0));
    }

    #[test]
    fn should_read_from_rom_bank1() {
        let memory = test_memory();

        assert_eq!(memory.read_byte(0x30000), Some(1));
        assert_eq!(memory.read_byte(0x3FFFF), Some(1));
    }

    #[test]
    fn should_select_another_rom_bank() {
        let mut memory = test_memory();

        memory.set_rom_bank0(2);

        assert_eq!(memory.read_byte(0x20000), Some(2));
    }

    #[test]
    fn should_select_another_linear_bank() {
        let mut memory = test_memory();

        memory.set_linear_bank(1);

        assert_eq!(memory.read_byte(0x40000), Some(20));
    }

    #[test]
    fn should_reject_addresses_outside_rom_windows() {
        let memory = test_memory();

        assert_eq!(memory.read_byte(0x1FFFF), None);
        assert_eq!(memory.read_byte(0x100000), None);
    }

    #[test]
    fn should_return_none_when_selected_bank_exceeds_rom() {
        let mut memory = test_memory();

        memory.set_rom_bank0(100);

        assert_eq!(memory.read_byte(0x20000), None);
    }
}
