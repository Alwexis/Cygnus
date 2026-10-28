use crate::keyboard::Keyboard;
use crate::keypad::Keypad;

const KEYPAD_PORT: u8 = 0xB5;

const ROM_LINEAR_BANK_PORT: u8 = 0xC0;
const RAM_BANK_PORT: u8 = 0xC1;
const ROM_BANK0_PORT: u8 = 0xC2;
const ROM_BANK1_PORT: u8 = 0xC3;
const TIMER_CONTROL_PORT: u8 = 0xA2;
const HORIZONTAL_TIMER_PORT: u8 = 0xA4;
const VERTICAL_TIMER_PORT: u8 = 0xA6;
const IRQ_BASE_PORT: u8 = 0xB0;
const IRQ_ENABLE_PORT: u8 = 0xB2;
const IRQ_ACK_PORT: u8 = 0xB6;
const SCANLINE_PORT: u8 = 0x02;
const SCANLINE_CONTROL_PORT: u8 = 0x03;
const HORIZONTAL_COUNTER_PORT: u8 = 0xA8;
const HORIZONTAL_COUNTER_HIGH_PORT: u8 = 0xA9;
const VERTICAL_COUNTER_PORT: u8 = 0xAA;
const VERTICAL_COUNTER_MIDDLE_PORT: u8 = 0xAB;
const VERTICAL_COUNTER_HIGH_PORT: u8 = 0xAC;

const VERTICAL_TIMER_IRQ: u8 = 0x20;
const HORIZONTAL_TIMER_IRQ: u8 = 0x80;
const CYCLES_PER_SCANLINE: u32 = 256;
const VISIBLE_SCANLINES: u16 = 144;
const TOTAL_SCANLINES: u16 = 159;

pub struct IO {
    keyboard: Keyboard,
    keypad: Keypad,
    timer: Timer,
    pending_interrupt: Option<u8>,
    timer_control: u8,
    horizontal_timer: u16,
    vertical_timer: u16,
    irq_base: u8,
    irq_enable: u8,
    irq_ack: u8,
    direct_timer: bool,
    video_cycles: u32,
    scanline: u16,
    scanline_control: u8,
    horizontal_counter: u16,
    vertical_counter: u32,

    rom_linear_bank: u8,
    ram_bank: u8,
    rom_bank0: u8,
    rom_bank1: u8,
}

#[derive(Debug, Clone, Copy)]
struct Timer {
    period: u32,
    counter: u32,
    vector: u8,
    enabled: bool,
}

impl Timer {
    fn new() -> Self {
        Self {
            period: 0,
            counter: 0,
            vector: 0,
            enabled: false,
        }
    }

    fn configure(&mut self, period: u32, vector: u8) {
        self.period = period;
        self.counter = period;
        self.vector = vector;
        self.enabled = period != 0;
    }

    fn tick(&mut self, cycles: u32) -> Option<u8> {
        if !self.enabled {
            return None;
        }

        let mut remaining = cycles;
        while remaining >= self.counter {
            remaining -= self.counter;
            self.counter = self.period;
            if remaining == 0 {
                return Some(self.vector);
            }
        }
        self.counter -= remaining;
        None
    }
}

impl IO {
    pub fn new() -> Self {
        Self::new_with_linear_bank(0)
    }

    pub fn new_with_linear_bank(linear_bank: u8) -> Self {
        Self {
            keyboard: Keyboard::new(),
            keypad: Keypad::new(),
            timer: Timer::new(),
            pending_interrupt: None,
            timer_control: 0,
            horizontal_timer: 0,
            vertical_timer: 0,
            irq_base: 0,
            irq_enable: 0,
            irq_ack: 0,
            direct_timer: false,
            video_cycles: 0,
            scanline: 0,
            scanline_control: 0,
            horizontal_counter: 0,
            vertical_counter: 0,
            rom_linear_bank: linear_bank,
            ram_bank: 0,
            rom_bank0: 0,
            rom_bank1: 1,
        }
    }

    pub fn keyboard_mut(&mut self) -> &mut Keyboard {
        &mut self.keyboard
    }

    pub fn configure_timer(&mut self, period: u32, vector: u8) {
        self.direct_timer = true;
        self.timer.configure(period, vector);
    }

    pub fn tick(&mut self, cycles: u32) {
        if let Some(vector) = self.timer.tick(cycles) {
            if self.direct_timer {
                self.pending_interrupt = Some(vector);
            } else {
                self.raise_interrupt(vector);
            }
        }

        self.video_cycles += cycles;
        while self.video_cycles >= CYCLES_PER_SCANLINE {
            self.video_cycles -= CYCLES_PER_SCANLINE;
            self.horizontal_counter = self.horizontal_counter.wrapping_add(1);
            self.scanline = self.scanline.wrapping_add(1);
            self.vertical_counter = self.vertical_counter.wrapping_add(1);

            if self.irq_enable & HORIZONTAL_TIMER_IRQ != 0 {
                self.raise_interrupt(HORIZONTAL_TIMER_IRQ);
            }
            if self.scanline == VISIBLE_SCANLINES && self.irq_enable & VERTICAL_TIMER_IRQ != 0 {
                self.raise_interrupt(VERTICAL_TIMER_IRQ);
            }
            if self.scanline >= TOTAL_SCANLINES {
                self.scanline = 0;
            }
        }
    }

    pub fn request_interrupt(&mut self, vector: u8) {
        self.pending_interrupt = Some(vector);
    }

    fn raise_interrupt(&mut self, source: u8) {
        if self.irq_enable & source == 0 {
            return;
        }
        self.irq_ack |= source;
        self.pending_interrupt = Some(self.irq_base.wrapping_add(source.trailing_zeros() as u8));
    }

    pub fn take_pending_interrupt(&mut self) -> Option<u8> {
        self.pending_interrupt.take()
    }

    pub fn rom_linear_bank(&self) -> u8 {
        self.rom_linear_bank
    }

    pub fn ram_bank(&self) -> u8 {
        self.ram_bank
    }

    pub fn rom_bank0(&self) -> u8 {
        self.rom_bank0
    }

    pub fn rom_bank1(&self) -> u8 {
        self.rom_bank1
    }

    pub fn read_port(&self, port: u8) -> u8 {
        match port {
            KEYPAD_PORT => (self.keypad.read_inputs(&self.keyboard) >> 8) as u8,
            ROM_LINEAR_BANK_PORT => self.rom_linear_bank,
            RAM_BANK_PORT => self.ram_bank,
            ROM_BANK0_PORT => self.rom_bank0,
            ROM_BANK1_PORT => self.rom_bank1,
            TIMER_CONTROL_PORT => self.timer_control,
            HORIZONTAL_TIMER_PORT..=0xA5 => {
                (self.horizontal_timer >> ((port - HORIZONTAL_TIMER_PORT) * 8)) as u8
            }
            VERTICAL_TIMER_PORT..=0xA7 => {
                (self.vertical_timer >> ((port - VERTICAL_TIMER_PORT) * 8)) as u8
            }
            IRQ_BASE_PORT => self.irq_base,
            IRQ_ENABLE_PORT => self.irq_enable,
            IRQ_ACK_PORT => self.irq_ack,
            SCANLINE_PORT => self.scanline as u8,
            SCANLINE_CONTROL_PORT => self.scanline_control,
            HORIZONTAL_COUNTER_PORT => self.horizontal_counter as u8,
            HORIZONTAL_COUNTER_HIGH_PORT => (self.horizontal_counter >> 8) as u8,
            VERTICAL_COUNTER_PORT => self.vertical_counter as u8,
            VERTICAL_COUNTER_MIDDLE_PORT => (self.vertical_counter >> 8) as u8,
            VERTICAL_COUNTER_HIGH_PORT => (self.vertical_counter >> 16) as u8,
            _ => 0xFF,
        }
    }

    pub fn write_port(&mut self, port: u8, value: u8) {
        match port {
            KEYPAD_PORT => {
                self.keypad.write_selection((value as u16) << 8);
            }
            ROM_LINEAR_BANK_PORT => self.rom_linear_bank = value,
            RAM_BANK_PORT => self.ram_bank = value,
            ROM_BANK0_PORT => self.rom_bank0 = value,
            ROM_BANK1_PORT => self.rom_bank1 = value,
            TIMER_CONTROL_PORT => {
                self.timer_control = value;
                let (period, source) = if value & 0x04 != 0 {
                    (self.vertical_timer, VERTICAL_TIMER_IRQ)
                } else if value & 0x01 != 0 {
                    (self.horizontal_timer, HORIZONTAL_TIMER_IRQ)
                } else {
                    (0, 0)
                };
                if source != 0 {
                    self.direct_timer = false;
                    let period = u32::from(period.max(1));
                    self.timer.configure(period, source);
                }
            }
            0xA5 => {
                self.horizontal_timer = (self.horizontal_timer & 0x00FF) | (u16::from(value) << 8);
                if self.timer_control & 0x01 != 0 {
                    self.timer.configure(
                        u32::from(self.horizontal_timer.max(1)),
                        HORIZONTAL_TIMER_IRQ,
                    );
                }
            }
            HORIZONTAL_TIMER_PORT => {
                self.horizontal_timer = (self.horizontal_timer & 0xFF00) | u16::from(value);
            }
            0xA7 => {
                self.vertical_timer = (self.vertical_timer & 0x00FF) | (u16::from(value) << 8);
                if self.timer_control & 0x04 != 0 {
                    self.timer
                        .configure(u32::from(self.vertical_timer.max(1)), VERTICAL_TIMER_IRQ);
                }
            }
            VERTICAL_TIMER_PORT => {
                self.vertical_timer = (self.vertical_timer & 0xFF00) | u16::from(value);
            }
            IRQ_BASE_PORT => self.irq_base = value,
            IRQ_ENABLE_PORT => self.irq_enable = value,
            IRQ_ACK_PORT => self.irq_ack &= !value,
            SCANLINE_PORT => self.scanline = u16::from(value) % TOTAL_SCANLINES,
            SCANLINE_CONTROL_PORT => self.scanline_control = value,
            HORIZONTAL_COUNTER_PORT => {
                self.horizontal_counter = (self.horizontal_counter & 0xFF00) | u16::from(value);
            }
            HORIZONTAL_COUNTER_HIGH_PORT => {
                self.horizontal_counter =
                    (self.horizontal_counter & 0x00FF) | (u16::from(value) << 8);
            }
            VERTICAL_COUNTER_PORT => {
                self.vertical_counter = (self.vertical_counter & 0xFFFF00) | u32::from(value);
            }
            VERTICAL_COUNTER_MIDDLE_PORT => {
                self.vertical_counter =
                    (self.vertical_counter & 0xFF00FF) | (u32::from(value) << 8);
            }
            VERTICAL_COUNTER_HIGH_PORT => {
                self.vertical_counter =
                    (self.vertical_counter & 0x00FFFF) | (u32::from(value) << 16);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rom_bank_registers_can_be_written_and_read() {
        let mut io = IO::new();

        io.write_port(0xC0, 3);
        io.write_port(0xC2, 5);
        io.write_port(0xC3, 7);

        assert_eq!(io.read_port(0xC0), 3);
        assert_eq!(io.read_port(0xC2), 5);
        assert_eq!(io.read_port(0xC3), 7);
    }

    #[test]
    fn test_ram_bank_register_can_be_written_and_read() {
        let mut io = IO::new();

        io.write_port(0xC1, 9);

        assert_eq!(io.read_port(0xC1), 9);
        assert_eq!(io.ram_bank(), 9);
    }

    #[test]
    fn test_unknown_port_reads_ff() {
        let io = IO::new();

        assert_eq!(io.read_port(0x00), 0xFF);
    }

    #[test]
    fn test_keypad_port_still_works() {
        let mut io = IO::new();

        io.keyboard_mut().press(crate::keyboard::Button::A);
        io.write_port(0xB5, 0x40);

        assert_eq!(io.read_port(0xB5), 0x44);
    }

    #[test]
    fn timer_requests_periodic_interrupts() {
        let mut io = IO::new();
        io.configure_timer(3, 0x08);

        io.tick(2);
        assert_eq!(io.take_pending_interrupt(), None);
        io.tick(1);
        assert_eq!(io.take_pending_interrupt(), Some(0x08));
        io.tick(3);
        assert_eq!(io.take_pending_interrupt(), Some(0x08));
    }

    #[test]
    fn wonder_swan_timer_registers_raise_enabled_irq_source() {
        let mut io = IO::new();

        io.write_port(IRQ_BASE_PORT, 0x10);
        io.write_port(IRQ_ENABLE_PORT, VERTICAL_TIMER_IRQ);
        io.write_port(VERTICAL_TIMER_PORT, 3);
        io.write_port(VERTICAL_TIMER_PORT + 1, 0);
        io.write_port(TIMER_CONTROL_PORT, 0x04);

        io.tick(3);

        assert_eq!(io.read_port(IRQ_ACK_PORT), VERTICAL_TIMER_IRQ);
        assert_eq!(io.take_pending_interrupt(), Some(0x15));
    }

    #[test]
    fn writing_irq_ack_clears_only_selected_sources() {
        let mut io = IO::new();

        io.write_port(IRQ_ENABLE_PORT, VERTICAL_TIMER_IRQ | HORIZONTAL_TIMER_IRQ);
        io.request_interrupt(0x15);
        io.irq_ack = VERTICAL_TIMER_IRQ | HORIZONTAL_TIMER_IRQ;
        io.write_port(IRQ_ACK_PORT, VERTICAL_TIMER_IRQ);

        assert_eq!(io.read_port(IRQ_ACK_PORT), HORIZONTAL_TIMER_IRQ);
    }

    #[test]
    fn video_clock_advances_scanlines_and_raises_vblank_irq() {
        let mut io = IO::new();

        io.write_port(IRQ_BASE_PORT, 0x10);
        io.write_port(IRQ_ENABLE_PORT, VERTICAL_TIMER_IRQ);
        io.tick(CYCLES_PER_SCANLINE * u32::from(VISIBLE_SCANLINES));

        assert_eq!(io.read_port(SCANLINE_PORT), VISIBLE_SCANLINES as u8);
        assert_eq!(io.read_port(VERTICAL_COUNTER_PORT), VISIBLE_SCANLINES as u8);
        assert_eq!(io.take_pending_interrupt(), Some(0x15));
        assert_eq!(io.read_port(IRQ_ACK_PORT), VERTICAL_TIMER_IRQ);
    }

    #[test]
    fn video_clock_wraps_after_total_scanlines() {
        let mut io = IO::new();

        io.tick(CYCLES_PER_SCANLINE * u32::from(TOTAL_SCANLINES));

        assert_eq!(io.read_port(SCANLINE_PORT), 0);
    }
}
