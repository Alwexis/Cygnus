pub const SCREEN_WIDTH: usize = 224;
pub const SCREEN_HEIGHT: usize = 144;
pub const VIDEO_REGISTER_COUNT: usize = 0x40;
pub const PALETTE_START: u8 = 0x20;
pub const PALETTE_END: u8 = 0x3F;

#[derive(Debug, Clone)]
pub struct Video {
    registers: [u8; VIDEO_REGISTER_COUNT],
    framebuffer: Vec<u8>,
}

impl Video {
    pub fn new() -> Self {
        Self {
            registers: [0; VIDEO_REGISTER_COUNT],
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    pub fn read_port(&self, port: u8) -> u8 {
        self.registers[usize::from(port)]
    }

    pub fn write_port(&mut self, port: u8, value: u8) {
        self.registers[usize::from(port)] = value;
    }

    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    pub fn clear(&mut self, color: u8) {
        self.framebuffer.fill(color & 0x0F);
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: u8) {
        if x < SCREEN_WIDTH && y < SCREEN_HEIGHT {
            self.framebuffer[y * SCREEN_WIDTH + x] = color & 0x0F;
        }
    }

    pub fn pixel(&self, x: usize, y: usize) -> Option<u8> {
        (x < SCREEN_WIDTH && y < SCREEN_HEIGHT).then(|| self.framebuffer[y * SCREEN_WIDTH + x])
    }

    pub fn palette_entry(&self, index: usize) -> Option<u8> {
        (index < 16).then(|| self.registers[usize::from(PALETTE_START) + index])
    }
}

impl Default for Video {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_registers_are_readable_and_writable() {
        let mut video = Video::new();

        video.write_port(0x10, 0xA5);

        assert_eq!(video.read_port(0x10), 0xA5);
    }

    #[test]
    fn framebuffer_has_wonderswan_dimensions() {
        let video = Video::new();

        assert_eq!(video.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn pixels_are_clipped_and_colors_use_four_bits() {
        let mut video = Video::new();

        video.set_pixel(2, 3, 0xFF);
        video.set_pixel(SCREEN_WIDTH, 0, 0x0F);

        assert_eq!(video.pixel(2, 3), Some(0x0F));
        assert_eq!(video.pixel(SCREEN_WIDTH, 0), None);
    }

    #[test]
    fn clear_fills_the_framebuffer() {
        let mut video = Video::new();

        video.clear(0xAB);

        assert!(video.framebuffer().iter().all(|&pixel| pixel == 0x0B));
    }
}
