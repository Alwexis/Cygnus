use crate::bus::Bus;

const ADDRESS_MASK: u32 = 0xFFFFF;

const FLAG_CARRY: u16 = 1 << 0;
const FLAG_PARITY: u16 = 1 << 2;
const FLAG_AUXILIARY: u16 = 1 << 4;
const FLAG_ZERO: u16 = 1 << 6;
const FLAG_SIGN: u16 = 1 << 7;
const FLAG_TRAP: u16 = 1 << 8;
const FLAG_INTERRUPT: u16 = 1 << 9;
const FLAG_DIRECTION: u16 = 1 << 10;
const FLAG_OVERFLOW: u16 = 1 << 11;
const FLAG_RESERVED: u16 = 1 << 1;

const ARITHMETIC_FLAGS: u16 =
    FLAG_CARRY | FLAG_PARITY | FLAG_AUXILIARY | FLAG_ZERO | FLAG_SIGN | FLAG_OVERFLOW;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AluOperation {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Compare,
}

impl Cpu {
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

    fn decode_memory_address(&mut self, bus: &Bus, modrm: u8) -> Result<(u16, u16), CpuError> {
        let mode = modrm >> 6;
        let rm = modrm & 7;

        let displacement: i16 = match mode {
            0 if rm == 6 => {
                // mod=00, r/m=110: dirección directa disp16.
                let offset = self.fetch_word(bus)?;
                return Ok((self.ds, offset));
            }
            0 => 0,
            1 => self.fetch_byte(bus)? as i8 as i16,
            2 => self.fetch_word(bus)? as i16,
            _ => unreachable!("El modo registro no tiene dirección de memoria"),
        };

        let (base, uses_bp) = match rm {
            0 => (self.bx.wrapping_add(self.si), false), // BX + SI
            1 => (self.bx.wrapping_add(self.di), false), // BX + DI
            2 => (self.bp.wrapping_add(self.si), true),  // BP + SI
            3 => (self.bp.wrapping_add(self.di), true),  // BP + DI
            4 => (self.si, false),
            5 => (self.di, false),
            6 => (self.bp, true),
            7 => (self.bx, false),
            _ => unreachable!(),
        };

        let offset = base.wrapping_add(displacement as u16);
        let segment = if uses_bp { self.ss } else { self.ds };

        Ok((segment, offset))
    }

    fn read_memory_byte(&self, bus: &Bus, segment: u16, offset: u16) -> Result<u8, CpuError> {
        let address = Self::physical_address(segment, offset);

        bus.read_byte(address)
            .ok_or(CpuError::UnmappedMemory(address))
    }

    fn read_memory_word(&self, bus: &Bus, segment: u16, offset: u16) -> Result<u16, CpuError> {
        let low = self.read_memory_byte(bus, segment, offset)?;
        let high = self.read_memory_byte(bus, segment, offset.wrapping_add(1))?;

        Ok(u16::from_le_bytes([low, high]))
    }

    fn write_memory_byte(
        &self,
        bus: &mut Bus,
        segment: u16,
        offset: u16,
        value: u8,
    ) -> Result<(), CpuError> {
        let address = Self::physical_address(segment, offset);

        if bus.write_byte(address, value) {
            Ok(())
        } else {
            Err(CpuError::UnmappedMemory(address))
        }
    }

    fn write_memory_word(
        &self,
        bus: &mut Bus,
        segment: u16,
        offset: u16,
        value: u16,
    ) -> Result<(), CpuError> {
        let [low, high] = value.to_le_bytes();

        self.write_memory_byte(bus, segment, offset, low)?;
        self.write_memory_byte(bus, segment, offset.wrapping_add(1), high)?;

        Ok(())
    }

    fn get_register16(&self, index: u8) -> u16 {
        match index {
            0 => self.ax,
            1 => self.cx,
            2 => self.dx,
            3 => self.bx,
            4 => self.sp,
            5 => self.bp,
            6 => self.si,
            7 => self.di,
            _ => unreachable!("Índice de registro inválido"),
        }
    }

    fn set_register16(&mut self, index: u8, value: u16) {
        match index {
            0 => self.ax = value,
            1 => self.cx = value,
            2 => self.dx = value,
            3 => self.bx = value,
            4 => self.sp = value,
            5 => self.bp = value,
            6 => self.si = value,
            7 => self.di = value,
            _ => unreachable!("Índice de registro inválido"),
        }
    }

    /// Índices de registros de 8 bits:
    /// 0=AL, 1=CL, 2=DL, 3=BL, 4=AH, 5=CH, 6=DH, 7=BH.
    fn get_register8(&self, index: u8) -> u8 {
        match index {
            0 => self.ax as u8,
            1 => self.cx as u8,
            2 => self.dx as u8,
            3 => self.bx as u8,
            4 => (self.ax >> 8) as u8,
            5 => (self.cx >> 8) as u8,
            6 => (self.dx >> 8) as u8,
            7 => (self.bx >> 8) as u8,
            _ => unreachable!("Índice de registro inválido"),
        }
    }

    fn set_register8(&mut self, index: u8, value: u8) {
        match index {
            0 => self.ax = (self.ax & 0xFF00) | u16::from(value),
            1 => self.cx = (self.cx & 0xFF00) | u16::from(value),
            2 => self.dx = (self.dx & 0xFF00) | u16::from(value),
            3 => self.bx = (self.bx & 0xFF00) | u16::from(value),
            4 => self.ax = (self.ax & 0x00FF) | (u16::from(value) << 8),
            5 => self.cx = (self.cx & 0x00FF) | (u16::from(value) << 8),
            6 => self.dx = (self.dx & 0x00FF) | (u16::from(value) << 8),
            7 => self.bx = (self.bx & 0x00FF) | (u16::from(value) << 8),
            _ => unreachable!("Índice de registro inválido"),
        }
    }

    fn set_flag(&mut self, flag: u16, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
        self.flags |= FLAG_RESERVED;
    }

    fn update_common_flags(&mut self, result: u16, width: u8) {
        let mask = if width == 8 { 0x00FF } else { 0xFFFF };
        let sign_bit = if width == 8 { 0x0080 } else { 0x8000 };
        let value = result & mask;

        self.set_flag(FLAG_ZERO, value == 0);
        self.set_flag(FLAG_SIGN, value & sign_bit != 0);
        self.set_flag(FLAG_PARITY, (value as u8).count_ones().is_multiple_of(2));
    }

    fn execute_alu(&mut self, operation: AluOperation, left: u16, right: u16, width: u8) -> u16 {
        let mask: u32 = if width == 8 { 0xFF } else { 0xFFFF };
        let sign_bit: u32 = if width == 8 { 0x80 } else { 0x8000 };
        let lhs = u32::from(left) & mask;
        let rhs = u32::from(right) & mask;

        let (result, carry, overflow, auxiliary) = match operation {
            AluOperation::Add => {
                let full = lhs + rhs;
                let result = full & mask;
                let overflow = (!(lhs ^ rhs) & (lhs ^ result) & sign_bit) != 0;
                (
                    result,
                    full > mask,
                    overflow,
                    ((lhs ^ rhs ^ result) & 0x10) != 0,
                )
            }
            AluOperation::Sub | AluOperation::Compare => {
                let result = lhs.wrapping_sub(rhs) & mask;
                let overflow = ((lhs ^ rhs) & (lhs ^ result) & sign_bit) != 0;
                (
                    result,
                    lhs < rhs,
                    overflow,
                    ((lhs ^ rhs ^ result) & 0x10) != 0,
                )
            }
            AluOperation::And | AluOperation::Or | AluOperation::Xor => {
                let result = match operation {
                    AluOperation::And => lhs & rhs,
                    AluOperation::Or => lhs | rhs,
                    AluOperation::Xor => lhs ^ rhs,
                    _ => unreachable!(),
                };
                (result, false, false, false)
            }
        };

        self.flags &= !ARITHMETIC_FLAGS;
        self.set_flag(FLAG_CARRY, carry);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.set_flag(FLAG_AUXILIARY, auxiliary);
        self.update_common_flags(result as u16, width);

        result as u16
    }

    fn decode_register_modrm(&mut self, bus: &mut Bus, opcode: u8) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let mode = modrm >> 6;
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;

        let width = if opcode & 1 == 0 { 8 } else { 16 };
        let direction_to_reg = opcode & 2 != 0;

        // MOV r/m,reg y MOV reg,r/m.
        if (0x88..=0x8B).contains(&opcode) {
            if mode == 0b11 {
                if width == 8 {
                    let source = if direction_to_reg {
                        self.get_register8(rm)
                    } else {
                        self.get_register8(reg)
                    };

                    let destination = if direction_to_reg { reg } else { rm };
                    self.set_register8(destination, source);
                } else {
                    let source = if direction_to_reg {
                        self.get_register16(rm)
                    } else {
                        self.get_register16(reg)
                    };

                    let destination = if direction_to_reg { reg } else { rm };
                    self.set_register16(destination, source);
                }

                return Ok(());
            }

            let (segment, offset) = self.decode_memory_address(bus, modrm)?;

            if direction_to_reg {
                if width == 8 {
                    let value = self.read_memory_byte(bus, segment, offset)?;
                    self.set_register8(reg, value);
                } else {
                    let value = self.read_memory_word(bus, segment, offset)?;
                    self.set_register16(reg, value);
                }
            } else if width == 8 {
                let value = self.get_register8(reg);
                self.write_memory_byte(bus, segment, offset, value)?;
            } else {
                let value = self.get_register16(reg);
                self.write_memory_word(bus, segment, offset, value)?;
            }

            return Ok(());
        }

        // Las operaciones ALU siguen soportadas en modo registro-registro.
        if mode != 0b11 {
            return Err(CpuError::UnsupportedOpcode {
                opcode,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }

        let operation = match opcode & 0xFC {
            0x00 => AluOperation::Add,
            0x08 => AluOperation::Or,
            0x20 => AluOperation::And,
            0x28 => AluOperation::Sub,
            0x30 => AluOperation::Xor,
            0x38 => AluOperation::Compare,
            _ => {
                return Err(CpuError::UnsupportedOpcode {
                    opcode,
                    cs: self.cs,
                    ip: self.ip.wrapping_sub(2),
                });
            }
        };

        let (destination, source) = if width == 8 {
            let left = if direction_to_reg {
                self.get_register8(reg)
            } else {
                self.get_register8(rm)
            };

            let right = if direction_to_reg {
                self.get_register8(rm)
            } else {
                self.get_register8(reg)
            };

            (u16::from(left), u16::from(right))
        } else {
            let left = if direction_to_reg {
                self.get_register16(reg)
            } else {
                self.get_register16(rm)
            };

            let right = if direction_to_reg {
                self.get_register16(rm)
            } else {
                self.get_register16(reg)
            };

            (left, right)
        };

        let result = self.execute_alu(operation, destination, source, width);

        if operation != AluOperation::Compare {
            let destination_register = if direction_to_reg { reg } else { rm };

            if width == 8 {
                self.set_register8(destination_register, result as u8);
            } else {
                self.set_register16(destination_register, result);
            }
        }

        Ok(())
    }

    fn execute_accumulator_immediate(&mut self, bus: &Bus, opcode: u8) -> Result<(), CpuError> {
        let width = if opcode & 1 == 0 { 8 } else { 16 };
        let immediate = if width == 8 {
            u16::from(self.fetch_byte(bus)?)
        } else {
            self.fetch_word(bus)?
        };

        let operation = match opcode & 0xFC {
            0x04 => AluOperation::Add,
            0x0C => AluOperation::Or,
            0x24 => AluOperation::And,
            0x2C => AluOperation::Sub,
            0x34 => AluOperation::Xor,
            0x3C => AluOperation::Compare,
            _ => {
                return Err(CpuError::UnsupportedOpcode {
                    opcode,
                    cs: self.cs,
                    ip: self.ip,
                });
            }
        };

        let accumulator = if width == 8 {
            u16::from(self.get_register8(0))
        } else {
            self.ax
        };

        let result = self.execute_alu(operation, accumulator, immediate, width);

        if operation != AluOperation::Compare {
            if width == 8 {
                self.set_register8(0, result as u8);
            } else {
                self.ax = result;
            }
        }

        Ok(())
    }

    pub fn step(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        if self.halted {
            return Ok(());
        }

        let instruction_cs = self.cs;
        let instruction_ip = self.ip;
        let opcode = self.fetch_byte(bus)?;

        match opcode {
            // NOP
            0x90 => {}

            // MOV r8, imm8
            0xB0..=0xB7 => {
                let value = self.fetch_byte(bus)?;
                self.set_register8(opcode - 0xB0, value);
            }

            // MOV r16, imm16
            0xB8..=0xBF => {
                let value = self.fetch_word(bus)?;
                self.set_register16(opcode - 0xB8, value);
            }

            // MOV r/m8,r8; MOV r/m16,r16; MOV r8,r/m8; MOV r16,r/m16
            0x88..=0x8B => self.decode_register_modrm(bus, opcode)?,

            // ADD, OR, AND, SUB, XOR, CMP entre registros
            0x00..=0x03 | 0x08..=0x0B | 0x20..=0x23 | 0x28..=0x2B | 0x30..=0x33 | 0x38..=0x3B => {
                self.decode_register_modrm(bus, opcode)?;
            }

            // Operaciones inmediatas contra AL o AX
            0x04 | 0x05 | 0x0C | 0x0D | 0x24 | 0x25 | 0x2C | 0x2D | 0x34 | 0x35 | 0x3C | 0x3D => {
                self.execute_accumulator_immediate(bus, opcode)?;
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

            // JZ / JE rel8
            0x74 => {
                let displacement = self.fetch_byte(bus)? as i8;
                if self.flags & FLAG_ZERO != 0 {
                    self.ip = self.ip.wrapping_add_signed(displacement as i16);
                }
            }

            // JNZ / JNE rel8
            0x75 => {
                let displacement = self.fetch_byte(bus)? as i8;
                if self.flags & FLAG_ZERO == 0 {
                    self.ip = self.ip.wrapping_add_signed(displacement as i16);
                }
            }

            // CLI / STI
            0xFA => self.flags &= !FLAG_INTERRUPT,
            0xFB => self.flags |= FLAG_INTERRUPT,

            // CLC / STC
            0xF8 => self.set_flag(FLAG_CARRY, false),
            0xF9 => self.set_flag(FLAG_CARRY, true),

            // CLD / STD
            0xFC => self.set_flag(FLAG_DIRECTION, false),
            0xFD => self.set_flag(FLAG_DIRECTION, true),

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

    pub fn run_steps(&mut self, bus: &mut Bus, limit: usize) -> Result<usize, CpuError> {
        let mut executed = 0;

        while executed < limit && !self.halted {
            self.step(bus)?;
            executed += 1;
        }

        Ok(executed)
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

    fn write_program(bus: &mut Bus, bytes: &[u8]) {
        for (address, value) in bytes.iter().copied().enumerate() {
            assert!(bus.write_byte(address as u32, value));
        }
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
        write_program(&mut bus, &[0xB8, 0x34, 0x12]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x1234);
        assert_eq!(cpu.ip, 3);
    }

    #[test]
    fn mov_loads_other_general_registers() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0xBB, 0xCD, 0xAB]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.bx, 0xABCD);
    }

    #[test]
    fn mov_al_and_ah_share_ax_storage() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0xB0, 0x34, 0xB4, 0x12]);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x1234);
    }

    #[test]
    fn mov_register_to_register_works() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x1234;
        write_program(&mut bus, &[0x89, 0xC3]); // MOV BX, AX

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.bx, 0x1234);
    }

    #[test]
    fn mov_byte_register_to_register_works() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x1234;
        write_program(&mut bus, &[0x88, 0xC3]); // MOV BL, AL

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.bx as u8, 0x34);
        assert_eq!(cpu.ax, 0x1234);
    }

    #[test]
    fn add_immediate_updates_accumulator_and_carry() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x00FF;
        write_program(&mut bus, &[0x04, 0x01]); // ADD AL, 1

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0);
        assert_ne!(cpu.flags & FLAG_CARRY, 0);
        assert_ne!(cpu.flags & FLAG_ZERO, 0);
    }

    #[test]
    fn sub_immediate_updates_zero_flag() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 5;
        write_program(&mut bus, &[0x2C, 0x05]); // SUB AL, 5

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0);
        assert_ne!(cpu.flags & FLAG_ZERO, 0);
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
    }

    #[test]
    fn cmp_does_not_modify_accumulator() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 7;
        write_program(&mut bus, &[0x3C, 0x07]); // CMP AL, 7

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 7);
        assert_ne!(cpu.flags & FLAG_ZERO, 0);
    }

    #[test]
    fn xor_register_with_itself_sets_zero_flag() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x55AA;
        write_program(&mut bus, &[0x31, 0xC0]); // XOR AX, AX

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0);
        assert_ne!(cpu.flags & FLAG_ZERO, 0);
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
    }

    #[test]
    fn conditional_jump_is_taken_when_zero_flag_is_set() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.flags |= FLAG_ZERO;
        write_program(&mut bus, &[0x74, 0x03]); // JZ +3

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 5);
    }

    #[test]
    fn conditional_jump_is_not_taken_when_zero_flag_is_clear() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.flags &= !FLAG_ZERO;
        write_program(&mut bus, &[0x74, 0x03]); // JZ +3

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 2);
    }

    #[test]
    fn nop_advances_instruction_pointer() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0x90]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 1);
    }

    #[test]
    fn short_jump_changes_instruction_pointer() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0xEB, 0x03]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 5);
    }

    #[test]
    fn hlt_stops_subsequent_execution() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0xF4, 0xB8, 0x34, 0x12]);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert!(cpu.halted);
        assert_eq!(cpu.ax, 0);
        assert_eq!(cpu.ip, 1);
    }

    #[test]
    fn unsupported_opcode_returns_error() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0x06]);

        let result = cpu.step(&mut bus);

        assert_eq!(
            result,
            Err(CpuError::UnsupportedOpcode {
                opcode: 0x06,
                cs: 0,
                ip: 0,
            })
        );
    }

    #[test]
    fn carry_flag_instructions_work() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        write_program(&mut bus, &[0xF9, 0xF8]);

        cpu.step(&mut bus).unwrap();
        assert_ne!(cpu.flags & FLAG_CARRY, 0);

        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
    }

    #[test]
    fn add_byte_detects_signed_overflow_and_auxiliary_carry() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x007F;

        write_program(&mut bus, &[0x04, 0x01]); // ADD AL, 1
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0x80);
        assert_ne!(cpu.flags & FLAG_OVERFLOW, 0);
        assert_ne!(cpu.flags & FLAG_SIGN, 0);
        assert_ne!(cpu.flags & FLAG_AUXILIARY, 0);
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
        assert_eq!(cpu.flags & FLAG_ZERO, 0);
    }

    #[test]
    fn add_word_detects_unsigned_overflow() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0xFFFF;

        write_program(&mut bus, &[0x05, 0x01, 0x00]); // ADD AX, 1
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0);
        assert_ne!(cpu.flags & FLAG_CARRY, 0);
        assert_ne!(cpu.flags & FLAG_ZERO, 0);
        assert_eq!(cpu.flags & FLAG_OVERFLOW, 0);
        assert_ne!(cpu.flags & FLAG_AUXILIARY, 0);
    }

    #[test]
    fn subtract_byte_detects_borrow_and_sign() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0;

        write_program(&mut bus, &[0x2C, 0x01]); // SUB AL, 1
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0xFF);
        assert_ne!(cpu.flags & FLAG_CARRY, 0);
        assert_ne!(cpu.flags & FLAG_SIGN, 0);
        assert_ne!(cpu.flags & FLAG_AUXILIARY, 0);
        assert_eq!(cpu.flags & FLAG_ZERO, 0);
    }

    #[test]
    fn subtract_byte_detects_signed_overflow() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x0080;

        write_program(&mut bus, &[0x2C, 0x01]); // SUB AL, 1
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0x7F);
        assert_ne!(cpu.flags & FLAG_OVERFLOW, 0);
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
        assert_eq!(cpu.flags & FLAG_SIGN, 0);
    }

    #[test]
    fn mov_immediate_preserves_flags() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.flags = FLAG_RESERVED
            | FLAG_CARRY
            | FLAG_PARITY
            | FLAG_AUXILIARY
            | FLAG_ZERO
            | FLAG_SIGN
            | FLAG_TRAP
            | FLAG_INTERRUPT
            | FLAG_DIRECTION
            | FLAG_OVERFLOW;

        let original_flags = cpu.flags;
        write_program(&mut bus, &[0xB8, 0x34, 0x12]); // MOV AX, 1234h
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x1234);
        assert_eq!(cpu.flags, original_flags);
    }

    #[test]
    fn logical_operation_clears_carry_and_overflow() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();
        cpu.ax = 0x00F0;
        cpu.flags |= FLAG_CARRY | FLAG_OVERFLOW;

        write_program(&mut bus, &[0x24, 0x0F]); // AND AL, 0Fh
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0);
        assert_eq!(cpu.flags & FLAG_CARRY, 0);
        assert_eq!(cpu.flags & FLAG_OVERFLOW, 0);
        assert_ne!(cpu.flags & FLAG_ZERO, 0);
        assert_ne!(cpu.flags & FLAG_PARITY, 0);
    }

    #[test]
    fn mov_word_between_register_and_memory() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        // MOV [0200h], AX
        // MOV BX, [0200h]
        write_program(&mut bus, &[0x89, 0x06, 0x00, 0x02, 0x8B, 0x1E, 0x00, 0x02]);

        cpu.ax = 0x1234;

        cpu.step(&mut bus).unwrap();
        assert_eq!(bus.read_byte(0x0200), Some(0x34));
        assert_eq!(bus.read_byte(0x0201), Some(0x12));

        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.bx, 0x1234);
    }

    #[test]
    fn mov_byte_with_bx_plus_si_addressing() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        // MOV [BX+SI], AL
        write_program(&mut bus, &[0x88, 0x00]);

        cpu.bx = 0x0200;
        cpu.si = 0x0010;
        cpu.ax = 0x005A;

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0210), Some(0x5A));
    }

    #[test]
    fn mov_uses_ss_for_bp_based_addressing() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        // MOV AX, [BP+00h]
        write_program(&mut bus, &[0x8B, 0x46, 0x00]);

        cpu.bp = 0x0200;
        cpu.ds = 0x0000;
        cpu.ss = 0x0010;

        // SS:0200h -> dirección física 0300h.
        assert!(bus.write_byte(0x0300, 0x78));
        assert!(bus.write_byte(0x0301, 0x56));

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x5678);
    }

    #[test]
    fn run_steps_stops_at_instruction_limit() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        write_program(&mut bus, &[0x90, 0x90, 0x90]);

        assert_eq!(cpu.run_steps(&mut bus, 3), Ok(3));
        assert_eq!(cpu.ip, 3);
    }

    #[test]
    fn run_steps_stops_when_cpu_is_halted() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        write_program(&mut bus, &[0xF4]);

        assert_eq!(cpu.run_steps(&mut bus, 3), Ok(1));
        assert!(cpu.halted);
        assert_eq!(cpu.ip, 1);
    }
}
