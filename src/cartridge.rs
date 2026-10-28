use std::fs;
use std::io;
use std::path::Path;

const HEADER_SIZE: usize = 10;

pub struct Cartridge {
    rom: Vec<u8>,
}

impl Cartridge {
    /// Carga una ROM desde un archivo.
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let rom = fs::read(path)?;
        Self::from_bytes(rom)
    }

    /// Construye un cartucho a partir de bytes ya cargados.
    pub fn from_bytes(rom: Vec<u8>) -> io::Result<Self> {
        if rom.len() < HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "La ROM es demasiado pequeña para contener la cabecera",
            ));
        }

        Ok(Self { rom })
    }

    pub fn size(&self) -> usize {
        self.rom.len()
    }

    pub fn read_byte(&self, address: usize) -> Option<u8> {
        self.rom.get(address).copied()
    }

    /// Devuelve los últimos 10 bytes de la ROM.
    pub fn header_bytes(&self) -> &[u8] {
        &self.rom[self.rom.len() - HEADER_SIZE..]
    }

    fn header_byte(&self, index: usize) -> u8 {
        self.header_bytes()[index]
    }

    /// Identificador del fabricante almacenado en la cabecera.
    pub fn manufacturer_id(&self) -> u8 {
        self.header_byte(0)
    }

    pub fn manufacturer_name(&self) -> &'static str {
        match self.manufacturer_id() {
            0x01 => "Bandai",
            0x02 => "Taito",
            0x03 => "Tomy",
            0x04 => "Koei",
            0x05 => "Data East",
            0x06 => "Asmik Ace",
            0x0C => "Sunsoft",
            0x0E => "Banpresto",
            0x12 => "Konami",
            0x28 => "Squaresoft",
            0x36 => "Capcom",
            _ => "Desconocido",
        }
    }

    /// Requisito mínimo indicado por la cabecera.
    pub fn minimum_requirement(&self) -> &'static str {
        match self.header_byte(1) {
            0x00 => "WonderSwan",
            0x01 => "WonderSwan Color",
            _ => "Desconocido",
        }
    }

    pub fn cartridge_id(&self) -> u8 {
        self.header_byte(2)
    }

    /// Nombre conocido del juego, si coincide con nuestra pequeña base de datos.
    ///
    /// El título no está almacenado como texto en estos campos:
    /// se identifica mediante los datos de la cabecera.
    pub fn known_title(&self) -> Option<&'static str> {
        match (self.manufacturer_id(), self.cartridge_id()) {
            (0x01, 0x35) => Some("Rockman EXE WS"),
            _ => None,
        }
    }

    /// Código comercial conocido, cuando podemos identificarlo.
    pub fn known_serial(&self) -> Option<&'static str> {
        match (self.manufacturer_id(), self.cartridge_id()) {
            (0x01, 0x35) => Some("SWJ-BANC35"),
            _ => None,
        }
    }

    /// Código de tamaño declarado por la cabecera.
    pub fn rom_size_code(&self) -> u8 {
        self.header_byte(4)
    }

    /// Tamaño declarado por la cabecera, en bytes.
    pub fn declared_rom_size(&self) -> Option<usize> {
        match self.rom_size_code() {
            0x01 => Some(128 * 1024),
            0x02 => Some(512 * 1024),
            0x03 => Some(1024 * 1024),
            0x04 => Some(2 * 1024 * 1024),
            0x05 => Some(3 * 1024 * 1024),
            0x06 => Some(4 * 1024 * 1024),
            0x07 => Some(6 * 1024 * 1024),
            0x08 => Some(8 * 1024 * 1024),
            0x09 => Some(16 * 1024 * 1024),
            _ => None,
        }
    }

    pub fn save_type_code(&self) -> u8 {
        self.header_byte(5)
    }

    pub fn save_type_name(&self) -> &'static str {
        match self.save_type_code() {
            0x00 => "Sin memoria de guardado",
            0x01 => "SRAM de 8 KiB",
            0x02 => "SRAM de 32 KiB",
            0x03 => "SRAM de 128 KiB",
            0x04 => "SRAM de 256 KiB",
            0x10 => "EEPROM de 128 bytes",
            0x20 => "EEPROM de 2 KiB",
            0x50 => "EEPROM de 1 KiB",
            _ => "Tipo desconocido",
        }
    }

    pub fn extra_configuration(&self) -> u8 {
        self.header_byte(6)
    }

    pub fn has_rtc(&self) -> bool {
        self.header_byte(7) & 0x02 != 0
    }

    /// Checksum almacenado en la cabecera, interpretado como little-endian.
    pub fn stored_checksum(&self) -> u16 {
        u16::from_le_bytes([self.header_byte(8), self.header_byte(9)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rom() -> Vec<u8> {
        let mut rom = vec![0; 1024];

        let header = [0x01, 0x01, 0x35, 0x00, 0x06, 0x02, 0x04, 0x00, 0x0E, 0x84];

        let start = rom.len() - HEADER_SIZE;
        rom[start..].copy_from_slice(&header);

        rom
    }

    #[test]
    fn should_load_rom_from_bytes() {
        let cartridge = Cartridge::from_bytes(sample_rom()).unwrap();

        assert_eq!(cartridge.size(), 1024);
    }

    #[test]
    fn should_read_byte_from_rom() {
        let rom = vec![0x10, 0x20, 0x30, 0x40, 0x50, 0, 0, 0, 0, 0];
        let cartridge = Cartridge::from_bytes(rom).unwrap();

        assert_eq!(cartridge.read_byte(0), Some(0x10));
        assert_eq!(cartridge.read_byte(2), Some(0x30));
    }

    #[test]
    fn should_return_none_for_out_of_bounds_address() {
        let cartridge = Cartridge::from_bytes(vec![0; HEADER_SIZE]).unwrap();

        assert_eq!(cartridge.read_byte(HEADER_SIZE), None);
    }

    #[test]
    fn should_reject_rom_smaller_than_header() {
        assert!(Cartridge::from_bytes(vec![0; 9]).is_err());
    }

    #[test]
    fn should_return_last_ten_bytes_as_header() {
        let rom: Vec<u8> = (0..20).collect();
        let cartridge = Cartridge::from_bytes(rom).unwrap();

        assert_eq!(
            cartridge.header_bytes(),
            &[10, 11, 12, 13, 14, 15, 16, 17, 18, 19]
        );
    }

    #[test]
    fn should_identify_rockman_exe_ws() {
        let cartridge = Cartridge::from_bytes(sample_rom()).unwrap();

        assert_eq!(cartridge.known_title(), Some("Rockman EXE WS"));
        assert_eq!(cartridge.known_serial(), Some("SWJ-BANC35"));
        assert_eq!(cartridge.manufacturer_name(), "Bandai");
    }

    #[test]
    fn should_decode_rom_and_save_sizes() {
        let cartridge = Cartridge::from_bytes(sample_rom()).unwrap();

        assert_eq!(cartridge.declared_rom_size(), Some(4 * 1024 * 1024));
        assert_eq!(cartridge.save_type_name(), "SRAM de 32 KiB");
    }

    #[test]
    fn should_read_stored_checksum() {
        let cartridge = Cartridge::from_bytes(sample_rom()).unwrap();

        assert_eq!(cartridge.stored_checksum(), 0x840E);
    }
}
