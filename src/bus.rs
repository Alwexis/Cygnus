use crate::cartridge::Cartridge;
use crate::io::IO;
use crate::memory::Memory;

const INTERNAL_RAM_SIZE: usize = 0x10000; // 64 KiB: WonderSwan Color
const EXTERNAL_RAM_SIZE: usize = 0x8000; // 32 KiB: SRAM de Rockman EXE WS

const INTERNAL_RAM_START: u32 = 0x00000;
const INTERNAL_RAM_END: u32 = 0x0FFFF;

const EXTERNAL_RAM_START: u32 = 0x10000;
const EXTERNAL_RAM_END: u32 = 0x1FFFF;

pub struct Bus {
    memory: Memory,
    io: IO,
    internal_ram: Vec<u8>,
    external_ram: Vec<u8>,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        let linear_bank = cartridge.size().div_ceil(0x100000).saturating_sub(1) as u8;

        let mut bus = Self {
            memory: Memory::new(cartridge),
            io: IO::new_with_linear_bank(linear_bank),
            internal_ram: vec![0; INTERNAL_RAM_SIZE],
            external_ram: vec![0; EXTERNAL_RAM_SIZE],
        };
        bus.memory.sync_bank_registers(&bus.io);
        bus
    }

    /// Lee RAM interna, SRAM del cartucho o ROM.
    /// Devuelve None si la dirección no está mapeada.
    pub fn read_byte(&self, address: u32) -> Option<u8> {
        match address {
            INTERNAL_RAM_START..=INTERNAL_RAM_END => Some(self.internal_ram[address as usize]),

            EXTERNAL_RAM_START..=EXTERNAL_RAM_END => {
                let bank = usize::from(self.io.ram_bank());
                let offset = (address - EXTERNAL_RAM_START) as usize;
                let physical_offset = (bank * 0x10000 + offset) % self.external_ram.len();

                Some(self.external_ram[physical_offset])
            }

            _ => self.memory.read_byte(address),
        }
    }

    /// Escribe en RAM interna o SRAM externa.
    /// Las regiones ROM y las direcciones no mapeadas son de solo lectura
    /// o no están implementadas en esta etapa.
    pub fn write_byte(&mut self, address: u32, value: u8) -> bool {
        match address {
            INTERNAL_RAM_START..=INTERNAL_RAM_END => {
                self.internal_ram[address as usize] = value;
                true
            }

            EXTERNAL_RAM_START..=EXTERNAL_RAM_END => {
                let bank = usize::from(self.io.ram_bank());
                let offset = (address - EXTERNAL_RAM_START) as usize;
                let physical_offset = (bank * 0x10000 + offset) % self.external_ram.len();

                self.external_ram[physical_offset] = value;
                true
            }

            _ => false,
        }
    }

    pub fn read_port(&self, port: u8) -> u8 {
        self.io.read_port(port)
    }

    pub fn write_port(&mut self, port: u8, value: u8) {
        let value = match port {
            0xC0 => {
                let bank_count = self.memory.rom_size() / 0x100000;
                if bank_count > 1 {
                    let mask = bank_count.next_power_of_two().saturating_sub(1);
                    value & mask as u8
                } else {
                    value
                }
            }

            0xC2 | 0xC3 => {
                let bank_count = self.memory.rom_size() / 0x10000;
                if bank_count > 1 {
                    let mask = bank_count.next_power_of_two().saturating_sub(1);
                    value & mask as u8
                } else {
                    value
                }
            }
            _ => value,
        };
        self.io.write_port(port, value);

        match port {
            0xC0 | 0xC2 | 0xC3 => {
                self.memory.sync_bank_registers(&self.io);
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        self.io.tick(cycles);
        if self.io.take_frame_ready() {
            self.render_frame();
        }
    }

    pub fn configure_timer(&mut self, period: u32, vector: u8) {
        self.io.configure_timer(period, vector);
    }

    pub fn request_interrupt(&mut self, vector: u8) {
        self.io.request_interrupt(vector);
    }

    pub fn take_pending_interrupt(&mut self) -> Option<u8> {
        self.io.take_pending_interrupt()
    }

    pub fn keyboard_mut(&mut self) -> &mut crate::keyboard::Keyboard {
        self.io.keyboard_mut()
    }

    pub fn framebuffer(&self) -> &[u8] {
        self.io.video().framebuffer()
    }

    pub fn render_frame(&mut self) {
        self.io.video_mut().render_background(&self.internal_ram);
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
    fn test_internal_ram_can_be_written_and_read() {
        let mut bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x0010), Some(0));
        assert!(bus.write_byte(0x0010, 0xA5));
        assert_eq!(bus.read_byte(0x0010), Some(0xA5));
    }

    #[test]
    fn test_internal_ram_extends_to_64_kib() {
        let mut bus = Bus::new(test_cartridge());

        assert!(bus.write_byte(0x04000, 0x42));
        assert!(bus.write_byte(0x0FFFF, 0x99));

        assert_eq!(bus.read_byte(0x04000), Some(0x42));
        assert_eq!(bus.read_byte(0x0FFFF), Some(0x99));
    }

    #[test]
    fn test_internal_ram_addresses_are_independent() {
        let mut bus = Bus::new(test_cartridge());

        assert!(bus.write_byte(0x0010, 0x12));
        assert!(bus.write_byte(0x0020, 0x34));

        assert_eq!(bus.read_byte(0x0010), Some(0x12));
        assert_eq!(bus.read_byte(0x0020), Some(0x34));
    }

    #[test]
    fn test_external_sram_can_be_written_and_read() {
        let mut bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x10000), Some(0));
        assert!(bus.write_byte(0x10000, 0x5A));
        assert_eq!(bus.read_byte(0x10000), Some(0x5A));
    }

    #[test]
    fn test_32k_sram_is_mirrored_in_64k_window() {
        let mut bus = Bus::new(test_cartridge());

        assert!(bus.write_byte(0x10000, 0xA5));

        assert_eq!(bus.read_byte(0x10000), Some(0xA5));
        assert_eq!(bus.read_byte(0x18000), Some(0xA5));
    }

    #[test]
    fn test_ram_bank_register_is_accessible() {
        let mut bus = Bus::new(test_cartridge());

        bus.write_port(0xC1, 1);

        assert_eq!(bus.read_port(0xC1), 1);
        assert!(bus.write_byte(0x10000, 0x6B));
        assert_eq!(bus.read_byte(0x10000), Some(0x6B));
    }

    #[test]
    fn test_rom_can_be_read() {
        let bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x20000), Some(0));
        assert_eq!(bus.read_byte(0x30000), Some(1));
    }

    #[test]
    fn test_rom_bank_port_changes_reading_automatically() {
        let mut bus = Bus::new(test_cartridge());

        assert_eq!(bus.read_byte(0x20000), Some(0));

        bus.write_port(0xC2, 2);

        assert_eq!(bus.read_byte(0x20000), Some(2));
        assert_eq!(bus.read_port(0xC2), 2);
    }

    #[test]
    fn test_rom_bank_ports_mask_values_to_cartridge_size() {
        let mut bus = Bus::new(test_cartridge());

        bus.write_port(0xC3, 0xDE);

        assert_eq!(bus.read_port(0xC3), 0x1E);
        assert_eq!(bus.read_byte(0x30000), Some(30));
    }

    #[test]
    fn test_linear_bank_port_changes_reading_automatically() {
        let mut bus = Bus::new(test_cartridge());

        bus.write_port(0xC0, 1);

        assert_eq!(bus.read_byte(0x40000), Some(20));
    }

    #[test]
    fn test_rom_cannot_be_written_through_bus() {
        let mut bus = Bus::new(test_cartridge());

        assert!(!bus.write_byte(0x20000, 0xAA));
        assert_eq!(bus.read_byte(0x20000), Some(0));
    }

    #[test]
    fn test_keypad_port_still_works() {
        let mut bus = Bus::new(test_cartridge());

        bus.keyboard_mut().press(crate::keyboard::Button::A);
        bus.write_port(0xB5, 0x40);

        assert_eq!(bus.read_port(0xB5), 0x44);
    }
}
