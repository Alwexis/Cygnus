pub const SCREEN_WIDTH: usize = 224;
pub const SCREEN_HEIGHT: usize = 144;
pub const VIDEO_REGISTER_COUNT: usize = 0x40;
pub const PALETTE_START: u8 = 0x20;
pub const PALETTE_END: u8 = 0x3F;
pub const DISPLAY_CONTROL_PORT: u8 = 0x00;
pub const BORDER_COLOR_PORT: u8 = 0x01;
pub const TILE_MAP_PORT: u8 = 0x07;
pub const BG1_SCROLL_X_PORT: u8 = 0x10;
pub const BG1_SCROLL_Y_PORT: u8 = 0x11;
pub const BG2_SCROLL_X_PORT: u8 = 0x12;
pub const BG2_SCROLL_Y_PORT: u8 = 0x13;
pub const LCD_CONTROL_PORT: u8 = 0x14;
pub const COLOR_CONTROL_PORT: u8 = 0x60;
pub const SPRITE_TABLE_PORT: u8 = 0x04;
pub const SPRITE_TILE_BASE_PORT: u8 = 0x05;
pub const SPRITE_COUNT_PORT: u8 = 0x06;

#[derive(Debug, Clone)]
pub struct Video {
    registers: [u8; VIDEO_REGISTER_COUNT],
    color_control: u8,
    framebuffer: Vec<u8>,
}

impl Video {
    pub fn new() -> Self {
        Self {
            registers: [0; VIDEO_REGISTER_COUNT],
            color_control: 0,
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    pub fn read_port(&self, port: u8) -> u8 {
        if port == COLOR_CONTROL_PORT {
            self.color_control
        } else {
            self.registers[usize::from(port)]
        }
    }

    pub fn write_port(&mut self, port: u8, value: u8) {
        if port == COLOR_CONTROL_PORT {
            self.color_control = value;
        } else if usize::from(port) < VIDEO_REGISTER_COUNT {
            self.registers[usize::from(port)] = value;
        }
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

    pub fn render_background(&mut self, ram: &[u8]) {
        self.clear(self.registers[usize::from(BORDER_COLOR_PORT)] & 0x0F);
        let display_control = self.registers[usize::from(DISPLAY_CONTROL_PORT)];
        if display_control & 0x07 == 0 {
            return;
        }

        if display_control & 0x01 != 0 {
            self.render_layer(
                ram,
                usize::from(self.registers[usize::from(TILE_MAP_PORT)] & 0x0F) << 11,
                BG1_SCROLL_X_PORT,
                BG1_SCROLL_Y_PORT,
                false,
            );
        }
        if display_control & 0x02 != 0 {
            self.render_layer(
                ram,
                usize::from(self.registers[usize::from(TILE_MAP_PORT)] >> 4) << 11,
                BG2_SCROLL_X_PORT,
                BG2_SCROLL_Y_PORT,
                true,
            );
        }
        if display_control & 0x04 != 0 {
            self.render_sprites(ram);
        }
    }

    fn render_layer(
        &mut self,
        ram: &[u8],
        map_base: usize,
        scroll_x_port: u8,
        scroll_y_port: u8,
        transparent_zero: bool,
    ) {
        let scroll_x = usize::from(self.registers[usize::from(scroll_x_port)]);
        let scroll_y = usize::from(self.registers[usize::from(scroll_y_port)]);
        let tile_base = if self.color_control & 0x40 != 0 {
            0x4000
        } else {
            0x2000
        };
        let tile_size = if self.color_control & 0x40 != 0 {
            32
        } else {
            16
        };

        for y in 0..SCREEN_HEIGHT {
            let source_y = y + scroll_y;
            let tile_row = (source_y / 8) & 0x1F;
            let pixel_y = source_y & 7;
            for x in 0..SCREEN_WIDTH {
                let source_x = x + scroll_x;
                let tile_col = (source_x / 8) & 0x3F;
                let pixel_x = source_x & 7;
                let map_offset = map_base + (tile_row * 64 + tile_col) * 2;
                let Some(&low) = ram.get(map_offset) else {
                    continue;
                };
                let Some(&high) = ram.get(map_offset + 1) else {
                    continue;
                };
                let tile_map = u16::from_le_bytes([low, high]);
                let tile = usize::from(tile_map & 0x01FF);
                let flipped_x = tile_map & 0x4000 != 0;
                let flipped_y = tile_map & 0x8000 != 0;
                let source_pixel_x = if flipped_x { 7 - pixel_x } else { pixel_x };
                let source_pixel_y = if flipped_y { 7 - pixel_y } else { pixel_y };
                let tile_offset = tile_base + tile * tile_size;
                let Some(&packed) =
                    ram.get(tile_offset + source_pixel_y * (tile_size / 8) + source_pixel_x / 2)
                else {
                    continue;
                };
                let color = if source_pixel_x & 1 == 0 {
                    packed >> 4
                } else {
                    packed & 0x0F
                };
                if !transparent_zero || color != 0 {
                    self.framebuffer[y * SCREEN_WIDTH + x] = color;
                }
            }
        }
    }

    fn render_sprites(&mut self, ram: &[u8]) {
        let table_base = usize::from(self.registers[usize::from(SPRITE_TABLE_PORT)] & 0x1F) << 9;
        let tile_base =
            0x4000 + (usize::from(self.registers[usize::from(SPRITE_TILE_BASE_PORT)]) << 2);
        let count = usize::from(self.registers[usize::from(SPRITE_COUNT_PORT)]);

        for index in (0..count.min(128)).rev() {
            let entry = table_base + index * 4;
            let Some(&tile_low) = ram.get(entry) else {
                continue;
            };
            let Some(&tile_high) = ram.get(entry + 1) else {
                continue;
            };
            let tile_map = u16::from_le_bytes([tile_low, tile_high]);
            let x = i16::from(ram.get(entry + 3).copied().unwrap_or(0) as i8);
            let y = i16::from(ram.get(entry + 2).copied().unwrap_or(0) as i8);
            let tile = usize::from(tile_map & 0x01FF);
            let flipped_x = tile_map & 0x4000 != 0;
            let flipped_y = tile_map & 0x8000 != 0;
            let tile_offset = tile_base + tile * 32;

            for pixel_y in 0usize..8 {
                let screen_y = y + pixel_y as i16;
                if !(0..SCREEN_HEIGHT as i16).contains(&screen_y) {
                    continue;
                }
                let source_y = if flipped_y { 7 - pixel_y } else { pixel_y };
                let Some(&row0) = ram.get(tile_offset + source_y * 4) else {
                    continue;
                };
                let Some(&row1) = ram.get(tile_offset + source_y * 4 + 1) else {
                    continue;
                };
                let Some(&row2) = ram.get(tile_offset + source_y * 4 + 2) else {
                    continue;
                };
                let Some(&row3) = ram.get(tile_offset + source_y * 4 + 3) else {
                    continue;
                };
                for pixel_x in 0..8 {
                    let source_x = if flipped_x { 7 - pixel_x } else { pixel_x };
                    let bit = 7 - source_x;
                    let color = ((row0 >> bit) & 1)
                        | (((row1 >> bit) & 1) << 1)
                        | (((row2 >> bit) & 1) << 2)
                        | (((row3 >> bit) & 1) << 3);
                    if color == 0 {
                        continue;
                    }
                    let screen_x = x + pixel_x as i16;
                    if (0..SCREEN_WIDTH as i16).contains(&screen_x) {
                        self.framebuffer[screen_y as usize * SCREEN_WIDTH + screen_x as usize] =
                            color;
                    }
                }
            }
        }
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

    #[test]
    fn renders_a_packed_four_bit_background_tile() {
        let mut video = Video::new();
        let mut ram = vec![0; 0x10000];

        video.write_port(DISPLAY_CONTROL_PORT, 0x01);
        video.write_port(COLOR_CONTROL_PORT, 0x40);
        ram[0] = 0;
        ram[1] = 0;
        ram[0x4000] = 0x12;

        video.render_background(&ram);

        assert_eq!(video.pixel(0, 0), Some(1));
        assert_eq!(video.pixel(1, 0), Some(2));
    }

    #[test]
    fn second_background_overlays_nonzero_pixels() {
        let mut video = Video::new();
        let mut ram = vec![0; 0x10000];

        video.write_port(DISPLAY_CONTROL_PORT, 0x03);
        video.write_port(COLOR_CONTROL_PORT, 0x40);
        video.write_port(TILE_MAP_PORT, 0x10);
        ram[0] = 0;
        ram[1] = 0;
        ram[0x800] = 1;
        ram[0x801] = 0;
        ram[0x4000] = 0x12;
        ram[0x4020] = 0x30;

        video.render_background(&ram);

        assert_eq!(video.pixel(0, 0), Some(3));
        assert_eq!(video.pixel(1, 0), Some(2));
    }

    #[test]
    fn renders_a_four_bit_sprite_over_the_background() {
        let mut video = Video::new();
        let mut ram = vec![0; 0x10000];

        video.write_port(DISPLAY_CONTROL_PORT, 0x04);
        video.write_port(COLOR_CONTROL_PORT, 0x40);
        video.write_port(SPRITE_COUNT_PORT, 1);
        ram[2] = 0;
        ram[3] = 0;
        ram[0x4000] = 0x80;
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x00;
        ram[0x4003] = 0x00;

        video.render_background(&ram);

        assert_eq!(video.pixel(0, 0), Some(1));
    }
}
