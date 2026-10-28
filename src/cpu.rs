use crate::bus::Bus;

const ADDRESS_MASK: u32 = 0xFFFFF;

const FLAG_CARRY: u16 = 1 << 0;
const FLAG_INTERRUPT: u16 = 1 << 9;
const FLAG_DIRECTION: u16 = 1 << 10;
const FLAG_RESERVED: u16 = 1 << 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuError {
    UnmappedMemory(u32),
    UnsupportedOpcode { opcode: u8, cs: u16, ip: u16 },
}

#[derive(Debug, Clone)]
pub struct Cpu {
    pub ax: u16,
    pub bx: u16,
    pub cx: u16,
    pub dx: u16,

    pub sp: u16,
    pub bp: u16,
    pub si: u16,
    pub di: u16,

    pub cs: u16,
    pub ds: u16,
    pub es: u16,
    pub ss: u16,

    pub ip: u16,
    pub flags: u16,
    pub halted: bool,
}

impl Cpu {
    /// esstado provisional compatible con el modelo x86 de 16 bits.
    /// hay q contrastarlo con el arranque real del WonderSwan.
    pub fn new() -> Self {
        Self {
            ax: 0,
            bx: 0,
            cx: 0,
            dx: 0,
            sp: 0,
            bp: 0,
            si: 0,
            di: 0,
            cs: 0xFFFF,
            ds: 0,
            es: 0,
            ss: 0,
            ip: 0,
            flags: FLAG_RESERVED,
            halted: false,
        }
    }

    /// direccion física de 20 bits para una dirección segmentada
    pub fn physical_address(segment: u16, offset: u16) -> u32 {
        (((segment as u32) << 4) + offset as u32) & ADDRESS_MASK
    }

    fn fetch_byte(&mut self, bus: &Bus) -> Result<u8, CpuError> {
        let address = Self::physical_address(self.cs, self.ip);

        let value = bus
            .read_byte(address)
            .ok_or(CpuError::UnmappedMemory(address))?;

        self.ip = self.ip.wrapping_add(1);
        Ok(value)
    }

    fn fetch_word(&mut self, bus: &Bus) -> Result<u16, CpuError> {
        let low = self.fetch_byte(bus)?;
        let high = self.fetch_byte(bus)?;

        Ok(u16::from_le_bytes([low, high]))
    }

    fn set_general_register(&mut self, index: u8, value: u16) {
        match index {
            0 => self.ax = value,
            1 => self.cx = value,
            2 => self.dx = value,
            3 => self.bx = value,
            4 => self.sp = value,
            5 => self.bp = value,
            6 => self.si = value,
            7 => self.di = value,
            _ => unreachable!("Índice de registro de 16 bits inválido"),
        }
    }

    /// Ejecuta una instruccion. Todavía no implementa ciclos de reloj ni nada
    pub fn step(&mut self, bus: &Bus) -> Result<(), CpuError> {
        if self.halted {
            return Ok(());
        }

        let instruction_cs = self.cs;
        let instruction_ip = self.ip;
        let opcode = self.fetch_byte(bus)?;

        match opcode {
            // NOP
            0x90 => {}

            // MOV r16, imm16
            0xB8..=0xBF => {
                let value = self.fetch_word(bus)?;
                self.set_general_register(opcode - 0xB8, value);
            }

            // JMP rel8
            0xEB => {
                let displacement = self.fetch_byte(bus)? as i8;
                self.ip = self.ip.wrapping_add_signed(displacement as i16);
            }

            // JMP rel16
            0xE9 => {
                let displacement = self.fetch_word(bus)? as i16;
                self.ip = self.ip.wrapping_add_signed(displacement);
            }

            // CLI / STI
            0xFA => self.flags &= !FLAG_INTERRUPT,
            0xFB => self.flags |= FLAG_INTERRUPT,

            // CLC / STC
            0xF8 => self.flags &= !FLAG_CARRY,
            0xF9 => self.flags |= FLAG_CARRY,

            // CLD / STD
            0xFC => self.flags &= !FLAG_DIRECTION,
            0xFD => self.flags |= FLAG_DIRECTION,

            // HLT
            0xF4 => self.halted = true,

            _ => {
                return Err(CpuError::UnsupportedOpcode {
                    opcode,
                    cs: instruction_cs,
                    ip: instruction_ip,
                });
            }
        }

        Ok(())
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    fn test_bus() -> Bus {
        let cartridge =
            Cartridge::from_bytes(vec![0; 0x10000]).expect("La ROM de prueba debe ser válida");

        Bus::new(cartridge)
    }

    fn cpu_at_zero() -> Cpu {
        let mut cpu = Cpu::new();
        cpu.cs = 0;
        cpu.ip = 0;
        cpu
    }

    #[test]
    fn reset_state_uses_provisional_x86_reset_address() {
        let cpu = Cpu::new();

        assert_eq!(cpu.cs, 0xFFFF);
        assert_eq!(cpu.ip, 0);
        assert_eq!(Cpu::physical_address(cpu.cs, cpu.ip), 0xFFFF0);
        assert!(!cpu.halted);
    }

    #[test]
    fn physical_address_calculates_segment_and_offset() {
        assert_eq!(Cpu::physical_address(0x1234, 0x5678), 0x179B8);
        assert_eq!(Cpu::physical_address(0xFFFF, 0x0010), 0x00000);
    }

    #[test]
    fn mov_ax_immediate_loads_value() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0xB8);
        bus.write_byte(1, 0x34);
        bus.write_byte(2, 0x12);

        cpu.step(&bus).unwrap();

        assert_eq!(cpu.ax, 0x1234);
        assert_eq!(cpu.ip, 3);
    }

    #[test]
    fn mov_loads_other_general_registers() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0xBB); // MOV BX, imm16
        bus.write_byte(1, 0xCD);
        bus.write_byte(2, 0xAB);

        cpu.step(&bus).unwrap();

        assert_eq!(cpu.bx, 0xABCD);
    }

    #[test]
    fn nop_advances_instruction_pointer() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0x90);

        cpu.step(&bus).unwrap();

        assert_eq!(cpu.ip, 1);
    }

    #[test]
    fn short_jump_changes_instruction_pointer() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0xEB);
        bus.write_byte(1, 0x03);

        cpu.step(&bus).unwrap();

        assert_eq!(cpu.ip, 5);
    }

    #[test]
    fn hlt_stops_subsequent_execution() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0xF4);
        bus.write_byte(1, 0xB8);
        bus.write_byte(2, 0x34);
        bus.write_byte(3, 0x12);

        cpu.step(&bus).unwrap();
        cpu.step(&bus).unwrap();

        assert!(cpu.halted);
        assert_eq!(cpu.ax, 0);
        assert_eq!(cpu.ip, 1);
    }

    #[test]
    fn unsupported_opcode_returns_error() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0x00);

        let result = cpu.step(&bus);

        assert_eq!(
            result,
            Err(CpuError::UnsupportedOpcode {
                opcode: 0x00,
                cs: 0,
                ip: 0,
            })
        );
    }

    #[test]
    fn carry_flag_instructions_work() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_byte(0, 0xF9); // STC
        bus.write_byte(1, 0xF8); // CLC

        cpu.step(&bus).unwrap();
        assert_ne!(cpu.flags & FLAG_CARRY, 0);

        cpu.step(&bus).unwrap();
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
    }
}
