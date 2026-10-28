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
    DivisionError { opcode: u8, cs: u16, ip: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceEntry {
    pub cs: u16,
    pub ip: u16,
    pub opcode: u8,
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
    segment_override: Option<u16>,
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
            segment_override: None,
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
                let segment = self.segment_override.take().unwrap_or(self.ds);
                return Ok((segment, offset));
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
        let segment =
            self.segment_override
                .take()
                .unwrap_or(if uses_bp { self.ss } else { self.ds });

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

    fn execute_shift_modrm(&mut self, bus: &mut Bus, opcode: u8) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let mode = modrm >> 6;
        let operation = (modrm >> 3) & 7;
        let width = if opcode == 0xD0 || opcode == 0xC0 || opcode == 0xD2 {
            8
        } else {
            16
        };
        if !matches!(operation, 0 | 1 | 4 | 5) {
            return Err(CpuError::UnsupportedOpcode {
                opcode,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }

        let memory_address = if mode == 0b11 {
            None
        } else {
            Some(self.decode_memory_address(bus, modrm)?)
        };
        let count = if opcode == 0xC0 || opcode == 0xC1 {
            self.fetch_byte(bus)? & 0x1F
        } else if opcode == 0xD2 || opcode == 0xD3 {
            self.get_register8(1) & 0x1F
        } else {
            1
        };
        let count = count % width;

        if count == 0 {
            return Ok(());
        }

        let value = if let Some((segment, offset)) = memory_address {
            if width == 8 {
                u16::from(self.read_memory_byte(bus, segment, offset)?)
            } else {
                self.read_memory_word(bus, segment, offset)?
            }
        } else if width == 8 {
            u16::from(self.get_register8(modrm & 7))
        } else {
            self.get_register16(modrm & 7)
        };
        let result = if operation == 4 {
            value << count
        } else if operation == 5 {
            value >> count
        } else if operation == 0 {
            ((value << count) | (value >> (width - count))) & ((1 << width) - 1)
        } else {
            (value >> count) | (value << (width - count))
        };
        let carry_bit = if operation == 4 {
            (value >> (width - count)) & 1
        } else if operation == 5 {
            (value >> (count - 1)) & 1
        } else if operation == 0 {
            result & 1
        } else {
            (result >> (width - 1)) & 1
        };
        if operation == 0 || operation == 1 {
            self.set_flag(FLAG_CARRY, carry_bit != 0);
        } else {
            self.set_flag(FLAG_CARRY, carry_bit != 0);
            self.update_common_flags(result, width);
        }

        if let Some((segment, offset)) = memory_address {
            if width == 8 {
                self.write_memory_byte(bus, segment, offset, result as u8)?;
            } else {
                self.write_memory_word(bus, segment, offset, result)?;
            }
        } else if width == 8 {
            self.set_register8(modrm & 7, result as u8);
        } else {
            self.set_register16(modrm & 7, result);
        }

        Ok(())
    }

    fn execute_immediate_modrm(&mut self, bus: &mut Bus, opcode: u8) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let mode = modrm >> 6;
        let operation = (modrm >> 3) & 7;

        let alu_operation = match operation {
            0 => AluOperation::Add,
            1 => AluOperation::Or,
            4 => AluOperation::And,
            5 => AluOperation::Sub,
            6 => AluOperation::Xor,
            7 => AluOperation::Compare,
            _ => {
                return Err(CpuError::UnsupportedOpcode {
                    opcode,
                    cs: self.cs,
                    ip: self.ip.wrapping_sub(2),
                });
            }
        };
        let width = if opcode == 0x80 { 8 } else { 16 };
        let address = if mode == 0b11 {
            None
        } else {
            Some(self.decode_memory_address(bus, modrm)?)
        };
        let left = if let Some((segment, offset)) = address {
            if width == 8 {
                u16::from(self.read_memory_byte(bus, segment, offset)?)
            } else {
                self.read_memory_word(bus, segment, offset)?
            }
        } else {
            if width == 8 {
                u16::from(self.get_register8(modrm & 7))
            } else {
                self.get_register16(modrm & 7)
            }
        };
        let right = if opcode == 0x83 {
            i16::from(self.fetch_byte(bus)? as i8) as u16
        } else if width == 8 {
            u16::from(self.fetch_byte(bus)?)
        } else {
            self.fetch_word(bus)?
        };
        let result = self.execute_alu(alu_operation, left, right, width);

        if alu_operation != AluOperation::Compare {
            if let Some((segment, offset)) = address {
                if width == 8 {
                    self.write_memory_byte(bus, segment, offset, result as u8)?;
                } else {
                    self.write_memory_word(bus, segment, offset, result)?;
                }
            } else {
                if width == 8 {
                    self.set_register8(modrm & 7, result as u8);
                } else {
                    self.set_register16(modrm & 7, result);
                }
            }
        }

        Ok(())
    }

    fn execute_ff_modrm(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let operation = (modrm >> 3) & 7;
        if !matches!(operation, 0 | 1 | 2 | 3 | 4 | 5 | 6) {
            return Err(CpuError::UnsupportedOpcode {
                opcode: 0xFF,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }

        if operation == 0 || operation == 1 {
            let carry = self.flags & FLAG_CARRY;
            let memory_address = if modrm >> 6 == 0b11 {
                None
            } else {
                Some(self.decode_memory_address(bus, modrm)?)
            };
            let value = if let Some((segment, offset)) = memory_address {
                self.read_memory_word(bus, segment, offset)?
            } else {
                self.get_register16(modrm & 7)
            };
            let result = if operation == 0 {
                self.execute_alu(AluOperation::Add, value, 1, 16)
            } else {
                self.execute_alu(AluOperation::Sub, value, 1, 16)
            };
            self.flags = (self.flags & !FLAG_CARRY) | carry | FLAG_RESERVED;
            if let Some((segment, offset)) = memory_address {
                self.write_memory_word(bus, segment, offset, result)?;
            } else {
                self.set_register16(modrm & 7, result);
            }
            return Ok(());
        }

        let memory_address = if modrm >> 6 == 0b11 {
            None
        } else {
            Some(self.decode_memory_address(bus, modrm)?)
        };
        let target_offset = if let Some((segment, offset)) = memory_address {
            self.read_memory_word(bus, segment, offset)?
        } else {
            self.get_register16(modrm & 7)
        };

        match operation {
            2 => {
                self.push_word(bus, self.ip)?;
                self.ip = target_offset;
            }
            4 => {
                self.ip = target_offset;
            }
            6 => {
                self.push_word(bus, target_offset)?;
            }
            3 | 5 => {
                let (segment, offset) = memory_address.ok_or(CpuError::UnsupportedOpcode {
                    opcode: 0xFF,
                    cs: self.cs,
                    ip: self.ip.wrapping_sub(2),
                })?;
                let target_segment = self.read_memory_word(bus, segment, offset.wrapping_add(2))?;
                if operation == 3 {
                    self.push_word(bus, self.cs)?;
                    self.push_word(bus, self.ip)?;
                }
                self.ip = target_offset;
                self.cs = target_segment;
            }
            _ => unreachable!(),
        }

        if matches!(operation, 2 | 4 | 6) {
            return Ok(());
        }

        if memory_address.is_none() {
            return Err(CpuError::UnsupportedOpcode {
                opcode: 0xFF,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }
        Ok(())
    }

    fn execute_imul_immediate(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let mode = modrm >> 6;
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;
        let source = if mode == 0b11 {
            self.get_register16(rm)
        } else {
            let (segment, offset) = self.decode_memory_address(bus, modrm)?;
            self.read_memory_word(bus, segment, offset)?
        };
        let immediate = self.fetch_byte(bus)? as i8 as i16;
        let result = i32::from(source as i16) * i32::from(immediate);
        let value = result as i16 as u16;
        self.set_register16(reg, value);
        self.set_flag(FLAG_CARRY, !(-32768..=32767).contains(&result));
        self.set_flag(FLAG_OVERFLOW, !(-32768..=32767).contains(&result));
        Ok(())
    }

    fn execute_inc_dec_byte(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let operation = (modrm >> 3) & 7;
        if operation > 1 {
            return Err(CpuError::UnsupportedOpcode {
                opcode: 0xFE,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }

        let carry = self.flags & FLAG_CARRY;
        let mode = modrm >> 6;
        let memory_address = if mode == 0b11 {
            None
        } else {
            Some(self.decode_memory_address(bus, modrm)?)
        };
        let value = if let Some((segment, offset)) = memory_address {
            u16::from(self.read_memory_byte(bus, segment, offset)?)
        } else {
            u16::from(self.get_register8(modrm & 7))
        };
        let alu_operation = if operation == 0 {
            AluOperation::Add
        } else {
            AluOperation::Sub
        };
        let result = self.execute_alu(alu_operation, value, 1, 8) as u8;
        self.flags = (self.flags & !FLAG_CARRY) | carry | FLAG_RESERVED;

        if memory_address.is_none() {
            self.set_register8(modrm & 7, result);
        } else {
            let (segment, offset) = memory_address.expect("memory address was decoded");
            self.write_memory_byte(bus, segment, offset, result)?;
        }
        Ok(())
    }

    fn push_word(&mut self, bus: &mut Bus, value: u16) -> Result<(), CpuError> {
        self.sp = self.sp.wrapping_sub(2);
        self.write_memory_word(bus, self.ss, self.sp, value)
    }

    fn pop_word(&mut self, bus: &Bus) -> Result<u16, CpuError> {
        let value = self.read_memory_word(bus, self.ss, self.sp)?;
        self.sp = self.sp.wrapping_add(2);
        Ok(value)
    }

    fn enter_interrupt(&mut self, bus: &mut Bus, vector: u8) -> Result<(), CpuError> {
        let vector_address = u32::from(vector) * 4;
        let offset = u16::from_le_bytes([
            bus.read_byte(vector_address)
                .ok_or(CpuError::UnmappedMemory(vector_address))?,
            bus.read_byte(vector_address + 1)
                .ok_or(CpuError::UnmappedMemory(vector_address + 1))?,
        ]);
        let segment = u16::from_le_bytes([
            bus.read_byte(vector_address + 2)
                .ok_or(CpuError::UnmappedMemory(vector_address + 2))?,
            bus.read_byte(vector_address + 3)
                .ok_or(CpuError::UnmappedMemory(vector_address + 3))?,
        ]);

        self.push_word(bus, self.flags)?;
        self.push_word(bus, self.cs)?;
        self.push_word(bus, self.ip)?;
        self.flags &= !(FLAG_INTERRUPT | FLAG_TRAP);
        self.cs = segment;
        self.ip = offset;
        self.halted = false;
        Ok(())
    }

    fn service_pending_interrupt(&mut self, bus: &mut Bus) -> Result<bool, CpuError> {
        if self.flags & FLAG_INTERRUPT == 0 {
            return Ok(false);
        }

        let Some(vector) = bus.take_pending_interrupt() else {
            return Ok(false);
        };
        self.enter_interrupt(bus, vector)?;
        Ok(true)
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

    fn get_segment_register(&self, index: u8) -> u16 {
        match index {
            0 => self.es,
            1 => self.cs,
            2 => self.ss,
            3 => self.ds,
            _ => unreachable!("Índice de segmento inválido"),
        }
    }

    fn set_segment_register(&mut self, index: u8, value: u16) {
        match index {
            0 => self.es = value,
            1 => self.cs = value,
            2 => self.ss = value,
            3 => self.ds = value,
            _ => unreachable!("Índice de segmento inválido"),
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

        // CMP r8/r16,r/m8/r/m16.
        if opcode == 0x3A || opcode == 0x3B {
            let left = if width == 8 {
                u16::from(self.get_register8(reg))
            } else {
                self.get_register16(reg)
            };
            let right = if mode == 0b11 {
                if width == 8 {
                    u16::from(self.get_register8(rm))
                } else {
                    self.get_register16(rm)
                }
            } else {
                let (segment, offset) = self.decode_memory_address(bus, modrm)?;

                if width == 8 {
                    u16::from(self.read_memory_byte(bus, segment, offset)?)
                } else {
                    self.read_memory_word(bus, segment, offset)?
                }
            };

            self.execute_alu(AluOperation::Compare, left, right, width);
            return Ok(());
        }

        // TEST r/m8,r8; TEST r/m16,r16.
        if opcode == 0x84 || opcode == 0x85 {
            let left = if mode == 0b11 {
                if width == 8 {
                    u16::from(self.get_register8(rm))
                } else {
                    self.get_register16(rm)
                }
            } else {
                let (segment, offset) = self.decode_memory_address(bus, modrm)?;

                if width == 8 {
                    u16::from(self.read_memory_byte(bus, segment, offset)?)
                } else {
                    self.read_memory_word(bus, segment, offset)?
                }
            };
            let right = if width == 8 {
                u16::from(self.get_register8(reg))
            } else {
                self.get_register16(reg)
            };

            self.execute_alu(AluOperation::And, left, right, width);
            return Ok(());
        }

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

        // MOV Sreg,r/m16 y MOV r/m16,Sreg.
        if (0x8C..=0x8E).contains(&opcode) {
            if opcode == 0x8D {
                return Err(CpuError::UnsupportedOpcode {
                    opcode,
                    cs: self.cs,
                    ip: self.ip.wrapping_sub(2),
                });
            }

            if mode == 0b11 {
                if opcode == 0x8E {
                    self.set_segment_register(reg, self.get_register16(rm));
                } else {
                    self.set_register16(rm, self.get_segment_register(reg));
                }
            } else {
                let (segment, offset) = self.decode_memory_address(bus, modrm)?;

                if opcode == 0x8E {
                    self.set_segment_register(reg, self.read_memory_word(bus, segment, offset)?);
                } else {
                    self.write_memory_word(bus, segment, offset, self.get_segment_register(reg))?;
                }
            }

            return Ok(());
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

        let memory_address = if mode == 0b11 {
            None
        } else {
            Some(self.decode_memory_address(bus, modrm)?)
        };
        let memory_value = if let Some((segment, offset)) = memory_address {
            if width == 8 {
                u16::from(self.read_memory_byte(bus, segment, offset)?)
            } else {
                self.read_memory_word(bus, segment, offset)?
            }
        } else {
            0
        };
        let (destination, source) = if width == 8 {
            let left = if direction_to_reg {
                u16::from(self.get_register8(reg))
            } else if memory_address.is_some() {
                memory_value
            } else {
                u16::from(self.get_register8(rm))
            };

            let right = if direction_to_reg {
                if memory_address.is_some() {
                    memory_value
                } else {
                    u16::from(self.get_register8(rm))
                }
            } else {
                u16::from(self.get_register8(reg))
            };

            (left, right)
        } else {
            let left = if direction_to_reg {
                self.get_register16(reg)
            } else if memory_address.is_some() {
                memory_value
            } else {
                self.get_register16(rm)
            };
            let right = if direction_to_reg {
                if memory_address.is_some() {
                    memory_value
                } else {
                    self.get_register16(rm)
                }
            } else {
                self.get_register16(reg)
            };

            (left, right)
        };

        let result = self.execute_alu(operation, destination, source, width);

        if operation != AluOperation::Compare {
            if let Some((segment, offset)) = memory_address {
                if !direction_to_reg {
                    if width == 8 {
                        self.write_memory_byte(bus, segment, offset, result as u8)?;
                    } else {
                        self.write_memory_word(bus, segment, offset, result)?;
                    }
                }
            } else if width == 8 {
                self.set_register8(if direction_to_reg { reg } else { rm }, result as u8);
            } else {
                self.set_register16(if direction_to_reg { reg } else { rm }, result);
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

    fn execute_test_accumulator(&mut self, bus: &mut Bus, opcode: u8) -> Result<(), CpuError> {
        let width = if opcode == 0xA8 { 8 } else { 16 };
        let immediate = if width == 8 {
            u16::from(self.fetch_byte(bus)?)
        } else {
            self.fetch_word(bus)?
        };
        let accumulator = if width == 8 {
            u16::from(self.get_register8(0))
        } else {
            self.ax
        };

        self.execute_alu(AluOperation::And, accumulator, immediate, width);
        Ok(())
    }

    fn execute_move_accumulator_moffs(
        &mut self,
        bus: &mut Bus,
        opcode: u8,
    ) -> Result<(), CpuError> {
        let offset = self.fetch_word(bus)?;
        let segment = self.segment_override.take().unwrap_or(self.ds);

        match opcode {
            0xA0 => {
                let value = self.read_memory_byte(bus, segment, offset)?;
                self.set_register8(0, value);
            }
            0xA1 => {
                let value = self.read_memory_word(bus, segment, offset)?;
                self.ax = value;
            }
            0xA2 => {
                let value = self.get_register8(0);
                self.write_memory_byte(bus, segment, offset, value)?;
            }
            0xA3 => {
                self.write_memory_word(bus, segment, offset, self.ax)?;
            }
            _ => unreachable!("Opcode moffs inválido"),
        }

        Ok(())
    }

    fn execute_move_immediate_to_memory(
        &mut self,
        bus: &mut Bus,
        opcode: u8,
    ) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let mode = modrm >> 6;
        let reg = (modrm >> 3) & 7;
        let width = if opcode == 0xC6 { 8 } else { 16 };

        if reg != 0 {
            return Err(CpuError::UnsupportedOpcode {
                opcode,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }

        if mode == 0b11 {
            let value = if width == 8 {
                u16::from(self.fetch_byte(bus)?)
            } else {
                self.fetch_word(bus)?
            };

            if width == 8 {
                self.set_register8(modrm & 7, value as u8);
            } else {
                self.set_register16(modrm & 7, value);
            }
        } else {
            let (segment, offset) = self.decode_memory_address(bus, modrm)?;
            let value = if width == 8 {
                u16::from(self.fetch_byte(bus)?)
            } else {
                self.fetch_word(bus)?
            };

            if width == 8 {
                self.write_memory_byte(bus, segment, offset, value as u8)?;
            } else {
                self.write_memory_word(bus, segment, offset, value)?;
            }
        }

        Ok(())
    }

    fn execute_test_modrm(&mut self, bus: &mut Bus, opcode: u8) -> Result<(), CpuError> {
        let modrm = self.fetch_byte(bus)?;
        let mode = modrm >> 6;
        let reg = (modrm >> 3) & 7;
        let width = if opcode == 0xF6 { 8 } else { 16 };

        if reg == 2 {
            if mode == 0b11 {
                if width == 8 {
                    self.set_register8(modrm & 7, !self.get_register8(modrm & 7));
                } else {
                    self.set_register16(modrm & 7, !self.get_register16(modrm & 7));
                }
            } else {
                let (segment, offset) = self.decode_memory_address(bus, modrm)?;
                if width == 8 {
                    let value = self.read_memory_byte(bus, segment, offset)?;
                    self.write_memory_byte(bus, segment, offset, !value)?;
                } else {
                    let value = self.read_memory_word(bus, segment, offset)?;
                    self.write_memory_word(bus, segment, offset, !value)?;
                }
            }
            return Ok(());
        }

        if reg == 4 {
            let operand = if mode == 0b11 {
                if width == 8 {
                    u16::from(self.get_register8(modrm & 7))
                } else {
                    self.get_register16(modrm & 7)
                }
            } else {
                let (segment, offset) = self.decode_memory_address(bus, modrm)?;
                if width == 8 {
                    u16::from(self.read_memory_byte(bus, segment, offset)?)
                } else {
                    self.read_memory_word(bus, segment, offset)?
                }
            };

            if width == 8 {
                let result = u16::from(self.get_register8(0)) * operand;
                self.ax = result;
            } else {
                let result = u32::from(self.ax) * u32::from(operand);
                self.ax = result as u16;
                self.dx = (result >> 16) as u16;
            }
            return Ok(());
        }

        if reg == 3 {
            let memory_address = if mode == 0b11 {
                None
            } else {
                Some(self.decode_memory_address(bus, modrm)?)
            };
            let value = if let Some((segment, offset)) = memory_address {
                if opcode == 0xF6 {
                    u16::from(self.read_memory_byte(bus, segment, offset)?)
                } else {
                    self.read_memory_word(bus, segment, offset)?
                }
            } else {
                if opcode == 0xF6 {
                    u16::from(self.get_register8(modrm & 7))
                } else {
                    self.get_register16(modrm & 7)
                }
            };
            let result = self.execute_alu(
                AluOperation::Sub,
                0,
                value,
                if opcode == 0xF6 { 8 } else { 16 },
            );
            if let Some((segment, offset)) = memory_address {
                if opcode == 0xF6 {
                    self.write_memory_byte(bus, segment, offset, result as u8)?;
                } else {
                    self.write_memory_word(bus, segment, offset, result)?;
                }
            } else {
                if opcode == 0xF6 {
                    self.set_register8(modrm & 7, result as u8);
                } else {
                    self.set_register16(modrm & 7, result);
                }
            }
            return Ok(());
        }

        if reg == 6 {
            let divisor = if mode == 0b11 {
                if width == 8 {
                    u16::from(self.get_register8(modrm & 7))
                } else {
                    self.get_register16(modrm & 7)
                }
            } else {
                let (segment, offset) = self.decode_memory_address(bus, modrm)?;
                if width == 8 {
                    u16::from(self.read_memory_byte(bus, segment, offset)?)
                } else {
                    self.read_memory_word(bus, segment, offset)?
                }
            };
            let error = || CpuError::DivisionError {
                opcode,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            };

            if divisor == 0 {
                return Err(error());
            }

            if width == 8 {
                let dividend = u16::from(self.ax);
                let quotient = dividend / divisor;
                if quotient > u16::from(u8::MAX) {
                    return Err(error());
                }
                self.set_register8(0, quotient as u8);
                self.set_register8(4, (dividend % divisor) as u8);
            } else {
                let dividend = (u32::from(self.dx) << 16) | u32::from(self.ax);
                let quotient = dividend / u32::from(divisor);
                if quotient > u32::from(u16::MAX) {
                    return Err(error());
                }
                self.ax = quotient as u16;
                self.dx = (dividend % u32::from(divisor)) as u16;
            }
            return Ok(());
        }

        if reg != 0 {
            return Err(CpuError::UnsupportedOpcode {
                opcode,
                cs: self.cs,
                ip: self.ip.wrapping_sub(2),
            });
        }

        let operand = if mode == 0b11 {
            if width == 8 {
                u16::from(self.get_register8(modrm & 7))
            } else {
                self.get_register16(modrm & 7)
            }
        } else {
            let (segment, offset) = self.decode_memory_address(bus, modrm)?;

            if width == 8 {
                u16::from(self.read_memory_byte(bus, segment, offset)?)
            } else {
                self.read_memory_word(bus, segment, offset)?
            }
        };
        let immediate = if width == 8 {
            u16::from(self.fetch_byte(bus)?)
        } else {
            self.fetch_word(bus)?
        };

        self.execute_alu(AluOperation::And, operand, immediate, width);
        Ok(())
    }

    fn execute_stos_word(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        self.write_memory_word(bus, self.es, self.di, self.ax)?;

        if self.flags & FLAG_DIRECTION == 0 {
            self.di = self.di.wrapping_add(2);
        } else {
            self.di = self.di.wrapping_sub(2);
        }

        Ok(())
    }

    fn execute_movs_word(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        let value = self.read_memory_word(bus, self.ds, self.si)?;
        self.write_memory_word(bus, self.es, self.di, value)?;

        if self.flags & FLAG_DIRECTION == 0 {
            self.si = self.si.wrapping_add(2);
            self.di = self.di.wrapping_add(2);
        } else {
            self.si = self.si.wrapping_sub(2);
            self.di = self.di.wrapping_sub(2);
        }

        Ok(())
    }

    fn execute_outs_byte(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        let value = self.read_memory_byte(bus, self.ds, self.si)?;
        bus.write_port(self.dx as u8, value);
        if self.flags & FLAG_DIRECTION == 0 {
            self.si = self.si.wrapping_add(1);
        } else {
            self.si = self.si.wrapping_sub(1);
        }
        Ok(())
    }

    fn execute_outs_word(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        let value = self.read_memory_word(bus, self.ds, self.si)?;
        let [low, high] = value.to_le_bytes();
        let port = self.dx as u8;
        bus.write_port(port, low);
        bus.write_port(port.wrapping_add(1), high);
        if self.flags & FLAG_DIRECTION == 0 {
            self.si = self.si.wrapping_add(2);
        } else {
            self.si = self.si.wrapping_sub(2);
        }
        Ok(())
    }

    pub fn step(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        if self.service_pending_interrupt(bus)? {
            bus.tick(1);
            return Ok(());
        }

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
            // MOV Sreg,r/m16; MOV r/m16,Sreg
            0x88..=0x8E => self.decode_register_modrm(bus, opcode)?,

            // TEST r/m8,r8; TEST r/m16,r16
            0x84 | 0x85 => self.decode_register_modrm(bus, opcode)?,

            // Operaciones ALU inmediatas sobre r/m.
            0x80 | 0x81 | 0x83 => self.execute_immediate_modrm(bus, opcode)?,

            // ADD, OR, AND, SUB, XOR, CMP entre registros
            0x00..=0x03 | 0x08..=0x0B | 0x20..=0x23 | 0x28..=0x2B | 0x30..=0x33 | 0x38..=0x3B => {
                self.decode_register_modrm(bus, opcode)?;
            }

            // Operaciones inmediatas contra AL o AX
            0x04 | 0x05 | 0x0C | 0x0D | 0x24 | 0x25 | 0x2C | 0x2D | 0x34 | 0x35 | 0x3C | 0x3D => {
                self.execute_accumulator_immediate(bus, opcode)?;
            }

            // TEST AL/AX, imm
            0xA8 | 0xA9 => self.execute_test_accumulator(bus, opcode)?,

            // MOV AL/AX,moffs16; MOV moffs16,AL/AX
            0xA0..=0xA3 => self.execute_move_accumulator_moffs(bus, opcode)?,

            // Segment override prefixes.
            0x26 => {
                self.segment_override = Some(self.es);
                self.step(bus)?;
                self.segment_override = None;
            }
            0x2E => {
                self.segment_override = Some(self.cs);
                self.step(bus)?;
                self.segment_override = None;
            }
            0x36 => {
                self.segment_override = Some(self.ss);
                self.step(bus)?;
                self.segment_override = None;
            }
            0x3E => {
                self.segment_override = Some(self.ds);
                self.step(bus)?;
                self.segment_override = None;
            }

            // MOV r/m8,imm8; MOV r/m16,imm16
            0xC6 | 0xC7 => self.execute_move_immediate_to_memory(bus, opcode)?,

            // TEST r/m8,imm8; TEST r/m16,imm16
            0xF6 | 0xF7 => self.execute_test_modrm(bus, opcode)?,

            // SHR r/m8,1; SHR r/m16,1
            0xC0 | 0xC1 | 0xD0 | 0xD1 | 0xD2 | 0xD3 => self.execute_shift_modrm(bus, opcode)?,

            // LOOP rel8
            0xE2 => {
                let displacement = self.fetch_byte(bus)? as i8;
                self.cx = self.cx.wrapping_sub(1);
                if self.cx != 0 {
                    self.ip = self.ip.wrapping_add_signed(displacement as i16);
                }
            }

            // PUSH r16
            0x50..=0x57 => {
                self.push_word(bus, self.get_register16(opcode - 0x50))?;
            }

            // PUSHA
            0x60 => {
                let original_sp = self.sp;
                self.push_word(bus, self.ax)?;
                self.push_word(bus, self.cx)?;
                self.push_word(bus, self.dx)?;
                self.push_word(bus, self.bx)?;
                self.push_word(bus, original_sp)?;
                self.push_word(bus, self.bp)?;
                self.push_word(bus, self.si)?;
                self.push_word(bus, self.di)?;
            }

            // INC r16
            0x40..=0x47 => {
                let carry = self.flags & FLAG_CARRY;
                let value = self.get_register16(opcode - 0x40);
                let result = self.execute_alu(AluOperation::Add, value, 1, 16);
                self.set_register16(opcode - 0x40, result);
                self.flags = (self.flags & !FLAG_CARRY) | carry | FLAG_RESERVED;
            }

            // DEC r16
            0x48..=0x4F => {
                let carry = self.flags & FLAG_CARRY;
                let value = self.get_register16(opcode - 0x48);
                let result = self.execute_alu(AluOperation::Sub, value, 1, 16);
                self.set_register16(opcode - 0x48, result);
                self.flags = (self.flags & !FLAG_CARRY) | carry | FLAG_RESERVED;
            }

            // POP r16
            0x58..=0x5F => {
                let value = self.pop_word(bus)?;
                self.set_register16(opcode - 0x58, value);
            }

            // POPA
            0x61 => {
                self.di = self.pop_word(bus)?;
                self.si = self.pop_word(bus)?;
                self.bp = self.pop_word(bus)?;
                let _ = self.pop_word(bus)?;
                self.bx = self.pop_word(bus)?;
                self.dx = self.pop_word(bus)?;
                self.cx = self.pop_word(bus)?;
                self.ax = self.pop_word(bus)?;
            }

            // PUSH/POP segmento
            0x06 => self.push_word(bus, self.es)?,
            0x0E => self.push_word(bus, self.cs)?,
            0x16 => self.push_word(bus, self.ss)?,
            0x1E => self.push_word(bus, self.ds)?,
            0x07 => self.es = self.pop_word(bus)?,
            0x17 => self.ss = self.pop_word(bus)?,
            0x1F => self.ds = self.pop_word(bus)?,

            // PUSH imm8 sign-extended
            0x6A => {
                let value = self.fetch_byte(bus)? as i8 as i16 as u16;
                self.push_word(bus, value)?;
            }

            // IMUL r16,r/m16,imm8 sign-extended
            0x6B => self.execute_imul_immediate(bus)?,

            // INC/DEC r/m8
            0xFE => self.execute_inc_dec_byte(bus)?,

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

            // JMP FAR ptr16:16
            0xEA => {
                let offset = self.fetch_word(bus)?;
                let segment = self.fetch_word(bus)?;
                self.ip = offset;
                self.cs = segment;
            }

            // CALL FAR ptr16:16
            0x9A => {
                let offset = self.fetch_word(bus)?;
                let segment = self.fetch_word(bus)?;
                self.push_word(bus, self.cs)?;
                self.push_word(bus, self.ip)?;
                self.ip = offset;
                self.cs = segment;
            }

            // CALL FAR m16:16
            0xFF => self.execute_ff_modrm(bus)?,

            // INT imm8 / INT3
            0xCD => {
                let vector = self.fetch_byte(bus)?;
                self.enter_interrupt(bus, vector)?;
            }
            0xCC => self.enter_interrupt(bus, 3)?,

            // IRET
            0xCF => {
                self.ip = self.pop_word(bus)?;
                self.cs = self.pop_word(bus)?;
                self.flags = self.pop_word(bus)? | FLAG_RESERVED;
            }

            // PUSHF / POPF
            0x9C => self.push_word(bus, self.flags)?,
            0x9D => self.flags = self.pop_word(bus)? | FLAG_RESERVED,

            // RET near
            0xC3 => {
                self.ip = self.pop_word(bus)?;
            }

            // RET FAR
            0xCB => {
                self.ip = self.pop_word(bus)?;
                self.cs = self.pop_word(bus)?;
            }

            // IN AL/AX, imm8
            0xE4 | 0xE5 => {
                let port = self.fetch_byte(bus);
                let port = port?;

                if opcode == 0xE4 {
                    self.set_register8(0, bus.read_port(port));
                } else {
                    let low = bus.read_port(port);
                    let high = bus.read_port(port.wrapping_add(1));
                    self.ax = u16::from_le_bytes([low, high]);
                }
            }

            // IN AL/AX, DX
            0xEC | 0xED => {
                let port = self.dx as u8;

                if opcode == 0xEC {
                    self.set_register8(0, bus.read_port(port));
                } else {
                    let low = bus.read_port(port);
                    let high = bus.read_port(port.wrapping_add(1));
                    self.ax = u16::from_le_bytes([low, high]);
                }
            }

            // OUT imm8, AL/AX
            0xE6 | 0xE7 => {
                let port = self.fetch_byte(bus);
                let port = port?;

                if opcode == 0xE6 {
                    bus.write_port(port, self.get_register8(0));
                } else {
                    let [low, high] = self.ax.to_le_bytes();
                    bus.write_port(port, low);
                    bus.write_port(port.wrapping_add(1), high);
                }
            }

            // OUT DX, AL/AX
            0xEE | 0xEF => {
                let port = self.dx as u8;

                if opcode == 0xEE {
                    bus.write_port(port, self.get_register8(0));
                } else {
                    let [low, high] = self.ax.to_le_bytes();
                    bus.write_port(port, low);
                    bus.write_port(port.wrapping_add(1), high);
                }
            }

            // MOVSW
            0xA5 => self.execute_movs_word(bus)?,

            // STOSW
            0xAB => self.execute_stos_word(bus)?,

            // OUTSB / OUTSW
            0x6E => self.execute_outs_byte(bus)?,
            0x6F => self.execute_outs_word(bus)?,

            // REP MOVSW / REP STOSW / REP OUTSB / REP OUTSW
            0xF3 => {
                let repeated_opcode = self.fetch_byte(bus)?;

                if !matches!(repeated_opcode, 0x6E | 0x6F | 0xA5 | 0xAB) {
                    return Err(CpuError::UnsupportedOpcode {
                        opcode: repeated_opcode,
                        cs: self.cs,
                        ip: self.ip.wrapping_sub(1),
                    });
                }

                while self.cx != 0 {
                    if repeated_opcode == 0xA5 {
                        self.execute_movs_word(bus)?;
                    } else if repeated_opcode == 0xAB {
                        self.execute_stos_word(bus)?;
                    } else if repeated_opcode == 0x6E {
                        self.execute_outs_byte(bus)?;
                    } else {
                        self.execute_outs_word(bus)?;
                    }
                    self.cx = self.cx.wrapping_sub(1);
                }
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

            // JB / JC rel8
            0x72 => {
                let displacement = self.fetch_byte(bus)? as i8;
                if self.flags & FLAG_CARRY != 0 {
                    self.ip = self.ip.wrapping_add_signed(displacement as i16);
                }
            }

            // JAE / JNC rel8
            0x73 => {
                let displacement = self.fetch_byte(bus)? as i8;
                if self.flags & FLAG_CARRY == 0 {
                    self.ip = self.ip.wrapping_add_signed(displacement as i16);
                }
            }

            // JBE / JNA rel8
            0x76 => {
                let displacement = self.fetch_byte(bus)? as i8;
                if self.flags & (FLAG_CARRY | FLAG_ZERO) != 0 {
                    self.ip = self.ip.wrapping_add_signed(displacement as i16);
                }
            }

            // JA / JNBE rel8
            0x77 => {
                let displacement = self.fetch_byte(bus)? as i8;
                if self.flags & (FLAG_CARRY | FLAG_ZERO) == 0 {
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

        bus.tick(1);
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

    pub fn run_steps_with_trace(
        &mut self,
        bus: &mut Bus,
        limit: usize,
        trace_limit: usize,
    ) -> Result<(usize, Vec<TraceEntry>), CpuError> {
        let mut executed = 0;
        let mut trace = Vec::with_capacity(trace_limit.min(limit));

        while executed < limit && !self.halted {
            let entry = self.trace_step(bus)?;

            if trace_limit != 0 {
                if trace.len() == trace_limit {
                    trace.remove(0);
                }
                trace.push(entry);
            }

            executed += 1;
        }

        Ok((executed, trace))
    }

    pub fn trace_step(&mut self, bus: &mut Bus) -> Result<TraceEntry, CpuError> {
        let address = Self::physical_address(self.cs, self.ip);
        let entry = TraceEntry {
            cs: self.cs,
            ip: self.ip,
            opcode: bus
                .read_byte(address)
                .ok_or(CpuError::UnmappedMemory(address))?,
        };
        self.step(bus)?;
        Ok(entry)
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

    fn write_bytes(bus: &mut Bus, address: u32, bytes: &[u8]) {
        for (offset, value) in bytes.iter().copied().enumerate() {
            assert!(bus.write_byte(address + offset as u32, value));
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
        write_program(&mut bus, &[0x0F]);

        let result = cpu.step(&mut bus);

        assert_eq!(
            result,
            Err(CpuError::UnsupportedOpcode {
                opcode: 0x0F,
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

    #[test]
    fn far_jump_loads_code_segment_and_instruction_pointer() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        write_program(&mut bus, &[0xEA, 0x34, 0x12, 0x78, 0x56]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 0x1234);
        assert_eq!(cpu.cs, 0x5678);
    }

    #[test]
    fn in_reads_byte_from_immediate_port() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        bus.write_port(0xC2, 0x35);
        write_program(&mut bus, &[0xE4, 0xC2]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax as u8, 0x35);
    }

    #[test]
    fn out_writes_byte_to_immediate_port() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x0035;
        write_program(&mut bus, &[0xE6, 0xC2]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_port(0xC2), 0x35);
    }

    #[test]
    fn mov_register_to_segment_register_works() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x1234;
        write_program(&mut bus, &[0x8E, 0xD8]); // MOV DS, AX

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ds, 0x1234);
    }

    #[test]
    fn stosw_writes_ax_to_es_di_and_advances_di() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x1234;
        cpu.es = 0;
        cpu.di = 0x0200;
        write_program(&mut bus, &[0xAB]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0200), Some(0x34));
        assert_eq!(bus.read_byte(0x0201), Some(0x12));
        assert_eq!(cpu.di, 0x0202);
    }

    #[test]
    fn rep_stosw_repeats_cx_times_and_clears_cx() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0xBEEF;
        cpu.es = 0;
        cpu.di = 0x0200;
        cpu.cx = 2;
        write_program(&mut bus, &[0xF3, 0xAB]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0200), Some(0xEF));
        assert_eq!(bus.read_byte(0x0201), Some(0xBE));
        assert_eq!(bus.read_byte(0x0202), Some(0xEF));
        assert_eq!(bus.read_byte(0x0203), Some(0xBE));
        assert_eq!(cpu.cx, 0);
        assert_eq!(cpu.di, 0x0204);
    }

    #[test]
    fn test_accumulator_updates_flags_without_modifying_accumulator() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x0002;
        write_program(&mut bus, &[0xA8, 0x02]); // TEST AL, 02h

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x0002);
        assert_eq!(cpu.flags & FLAG_ZERO, 0);
    }

    #[test]
    fn rep_movsw_copies_words_and_updates_indices() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ds = 0;
        cpu.es = 0;
        cpu.si = 0x0200;
        cpu.di = 0x0300;
        cpu.cx = 2;
        assert!(bus.write_byte(0x0200, 0x34));
        assert!(bus.write_byte(0x0201, 0x12));
        assert!(bus.write_byte(0x0202, 0x78));
        assert!(bus.write_byte(0x0203, 0x56));
        write_program(&mut bus, &[0xF3, 0xA5]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0300), Some(0x34));
        assert_eq!(bus.read_byte(0x0301), Some(0x12));
        assert_eq!(bus.read_byte(0x0302), Some(0x78));
        assert_eq!(bus.read_byte(0x0303), Some(0x56));
        assert_eq!(cpu.si, 0x0204);
        assert_eq!(cpu.di, 0x0304);
        assert_eq!(cpu.cx, 0);
    }

    #[test]
    fn rep_outsw_writes_words_to_video_ports_and_updates_si() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ds = 0;
        cpu.si = 0x0200;
        cpu.cx = 2;
        cpu.dx = 0x20;
        assert!(bus.write_byte(0x0200, 0x34));
        assert!(bus.write_byte(0x0201, 0x12));
        assert!(bus.write_byte(0x0202, 0x78));
        assert!(bus.write_byte(0x0203, 0x56));
        write_program(&mut bus, &[0xF3, 0x6F]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_port(0x20), 0x78);
        assert_eq!(bus.read_port(0x21), 0x56);
        assert_eq!(bus.read_port(0x22), 0);
        assert_eq!(bus.read_port(0x23), 0);
        assert_eq!(cpu.si, 0x0204);
        assert_eq!(cpu.cx, 0);
    }

    #[test]
    fn segment_override_moffs_writes_accumulator_to_selected_segment() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x005A;
        cpu.ss = 0x0010;
        write_program(&mut bus, &[0x36, 0xA2, 0x00, 0x02]); // MOV SS:[0200h], AL

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0300), Some(0x5A));
        assert_eq!(bus.read_byte(0x0200), Some(0));
    }

    #[test]
    fn far_call_and_return_preserve_code_location() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ss = 0;
        cpu.sp = 0x0200;
        write_program(&mut bus, &[0x9A, 0x00, 0x01, 0x00, 0x00]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 0x0100);
        assert_eq!(cpu.cs, 0);
        assert_eq!(cpu.sp, 0x01FC);

        assert!(bus.write_byte(0x0100, 0xCB));
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 5);
        assert_eq!(cpu.cs, 0);
        assert_eq!(cpu.sp, 0x0200);
    }

    #[test]
    fn test_modrm_memory_updates_flags_without_writing_memory() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        assert!(bus.write_byte(0x0200, 0x00));
        assert!(bus.write_byte(0x0201, 0x00));
        write_program(&mut bus, &[0xF7, 0x06, 0x00, 0x02, 0x00, 0x80]);

        cpu.step(&mut bus).unwrap();

        assert_ne!(cpu.flags & FLAG_ZERO, 0);
        assert_eq!(bus.read_byte(0x0200), Some(0x00));
        assert_eq!(bus.read_byte(0x0201), Some(0x00));
    }

    #[test]
    fn push_and_pop_general_register_preserve_value() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.sp = 0x0200;
        cpu.ax = 0x1234;
        write_program(&mut bus, &[0x50, 0x58]); // PUSH AX; POP AX

        cpu.step(&mut bus).unwrap();
        cpu.ax = 0;
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x1234);
        assert_eq!(cpu.sp, 0x0200);
    }

    #[test]
    fn cmp_register_against_memory_updates_flags() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.cs = 0;
        cpu.bx = 0x0200;
        cpu.ax = 0x0042;
        assert!(bus.write_byte(0x0200, 0x42));
        write_program(&mut bus, &[0x3A, 0x07]); // CMP AL,[BX]

        cpu.step(&mut bus).unwrap();

        assert_ne!(cpu.flags & FLAG_ZERO, 0);
        assert_eq!(cpu.ax, 0x0042);
    }

    #[test]
    fn add_immediate_byte_to_memory() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        assert!(bus.write_byte(0x0200, 0x01));
        write_program(&mut bus, &[0x80, 0x06, 0x00, 0x02, 0x08]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0200), Some(0x09));
    }

    #[test]
    fn shift_left_register_by_immediate_count() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.bx = 0x0003;
        write_program(&mut bus, &[0xC1, 0xE3, 0x03]); // SHL BX,3

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.bx, 0x0018);
    }

    #[test]
    fn far_call_indirect_reads_pointer_from_memory() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.sp = 0x0200;
        assert!(bus.write_byte(0x0300, 0x00));
        assert!(bus.write_byte(0x0301, 0x01));
        assert!(bus.write_byte(0x0302, 0x00));
        assert!(bus.write_byte(0x0303, 0x10));
        write_program(&mut bus, &[0xFF, 0x1E, 0x00, 0x03]); // CALL FAR [0300]

        cpu.step(&mut bus).unwrap();

        assert_eq!((cpu.cs, cpu.ip), (0x1000, 0x0100));
    }

    #[test]
    fn push_immediate_byte_sign_extends() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.sp = 0x0200;
        write_program(&mut bus, &[0x6A, 0x80]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.sp, 0x01FE);
        assert_eq!(bus.read_byte(0x01FE), Some(0x80));
        assert_eq!(bus.read_byte(0x01FF), Some(0xFF));
    }

    #[test]
    fn and_register_into_memory_updates_memory() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x00F0;
        assert!(bus.write_byte(0x0200, 0xFF));
        assert!(bus.write_byte(0x0201, 0x00));
        write_program(&mut bus, &[0x21, 0x06, 0x00, 0x02]); // AND [0200],AX

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0200), Some(0xF0));
        assert_eq!(bus.read_byte(0x0201), Some(0x00));
    }

    #[test]
    fn carry_jump_takes_branch_when_carry_is_set() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.flags |= FLAG_CARRY;
        write_program(&mut bus, &[0x72, 0x02, 0xB8, 0x34, 0x12]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 4);
    }

    #[test]
    fn imul_register_by_signed_immediate_writes_destination_register() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 4;
        write_program(&mut bus, &[0x6B, 0xD8, 0xFE]); // IMUL BX,AX,-2

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.bx, 0xFFF8);
    }

    #[test]
    fn dec_byte_register_preserves_carry() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.set_register8(1, 2);
        cpu.flags |= FLAG_CARRY;
        write_program(&mut bus, &[0xFE, 0xC9]); // DEC CL

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.get_register8(1), 1);
        assert_ne!(cpu.flags & FLAG_CARRY, 0);
    }

    #[test]
    fn no_carry_jump_takes_branch_when_carry_is_clear() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        write_program(&mut bus, &[0x73, 0x02, 0xB8, 0x34, 0x12]); // JAE +2

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 4);
    }

    #[test]
    fn below_or_equal_jump_takes_branch_for_zero() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.flags |= FLAG_ZERO;
        write_program(&mut bus, &[0x76, 0x02, 0xB8, 0x34, 0x12]); // JBE +2

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 4);
    }

    #[test]
    fn shift_left_register_by_cl_count() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 1;
        cpu.cx = 3;
        write_program(&mut bus, &[0xD3, 0xE0]); // SHL AX,CL

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 8);
    }

    #[test]
    fn shift_by_zero_leaves_register_and_flags_unchanged() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x1234;
        cpu.cx = 0;
        cpu.flags = FLAG_CARRY | FLAG_ZERO;
        write_program(&mut bus, &[0xD3, 0xE0]); // SHL AX,CL

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0x1234);
        assert_eq!(cpu.flags, FLAG_CARRY | FLAG_ZERO);
    }

    #[test]
    fn rotate_left_byte_in_memory_by_immediate_count() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        assert!(bus.write_byte(0x0200, 0x81));
        write_program(&mut bus, &[0xC0, 0x06, 0x00, 0x02, 0x01]); // ROL [0200],1

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0200), Some(0x03));
    }

    #[test]
    fn divide_word_uses_dx_ax_and_writes_quotient_and_remainder() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 10;
        cpu.dx = 0;
        cpu.bx = 3;
        write_program(&mut bus, &[0xF7, 0xF3]); // DIV BX

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 3);
        assert_eq!(cpu.dx, 1);
    }

    #[test]
    fn not_word_register_inverts_all_bits() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ax = 0x0F0F;
        write_program(&mut bus, &[0xF7, 0xD0]); // NOT AX

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ax, 0xF0F0);
    }

    #[test]
    fn inc_word_in_memory_preserves_carry() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        assert!(bus.write_byte(0x0200, 0xFF));
        assert!(bus.write_byte(0x0201, 0x00));
        cpu.flags = FLAG_CARRY;
        write_program(&mut bus, &[0xFF, 0x06, 0x00, 0x02]); // INC word [0200]

        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_byte(0x0200), Some(0x00));
        assert_eq!(bus.read_byte(0x0201), Some(0x01));
        assert_ne!(cpu.flags & FLAG_CARRY, 0);
    }

    #[test]
    fn software_interrupt_and_iret_restore_execution_state() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.ss = 0;
        cpu.sp = 0x0200;
        cpu.flags = FLAG_INTERRUPT | FLAG_CARRY | FLAG_RESERVED;
        write_bytes(&mut bus, 0x20 * 4, &[0x00, 0x01, 0x00, 0x00]);
        write_program(&mut bus, &[0xCD, 0x20]);
        write_bytes(&mut bus, 0x0100, &[0xCF]);

        cpu.step(&mut bus).unwrap();
        assert_eq!((cpu.cs, cpu.ip), (0, 0x0100));
        assert_eq!(cpu.flags & FLAG_INTERRUPT, 0);

        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.ip, 2);
        assert_ne!(cpu.flags & FLAG_INTERRUPT, 0);
        assert_ne!(cpu.flags & FLAG_CARRY, 0);
    }

    #[test]
    fn pending_interrupt_wakes_halted_cpu() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.flags = FLAG_INTERRUPT | FLAG_RESERVED;
        cpu.sp = 0x0200;
        write_program(&mut bus, &[0xF4]);
        write_bytes(&mut bus, 0x08 * 4, &[0x00, 0x01, 0x00, 0x00]);
        write_bytes(&mut bus, 0x0100, &[0xF4]);

        cpu.step(&mut bus).unwrap();
        assert!(cpu.halted);
        bus.request_interrupt(0x08);
        cpu.step(&mut bus).unwrap();

        assert!(!cpu.halted);
        assert_eq!(cpu.ip, 0x0100);
    }

    #[test]
    fn timer_interrupt_enters_configured_vector_after_a_cpu_step() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        cpu.flags = FLAG_INTERRUPT | FLAG_RESERVED;
        cpu.sp = 0x0200;
        write_program(&mut bus, &[0x90]);
        write_bytes(&mut bus, 0x08 * 4, &[0x00, 0x01, 0x00, 0x00]);
        bus.configure_timer(1, 0x08);

        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.ip, 1);
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.ip, 0x0100);
        assert_eq!(cpu.cs, 0);
    }

    #[test]
    fn traced_execution_keeps_only_the_requested_tail() {
        let mut bus = test_bus();
        let mut cpu = cpu_at_zero();

        write_program(&mut bus, &[0x90, 0x90, 0xF4]);

        let result = cpu.run_steps_with_trace(&mut bus, 10, 2).unwrap();

        assert_eq!(result.0, 3);
        assert_eq!(
            result.1,
            vec![
                TraceEntry {
                    cs: 0,
                    ip: 1,
                    opcode: 0x90
                },
                TraceEntry {
                    cs: 0,
                    ip: 2,
                    opcode: 0xF4
                }
            ]
        );
    }
}
