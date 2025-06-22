use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::ppu::{LCD_HEIGHT, LCD_WIDTH};
use crate::renderer::Renderer;

const SCALE: u32 = 4;

pub struct Lcd(Canvas<Window>);
impl Lcd {
    pub fn new() -> Self {
        let window = sdl2::init()
            .unwrap()
            .video()
            .unwrap()
            .window(
                "Game Boy Emulator",
                LCD_WIDTH as u32 * SCALE,
                LCD_HEIGHT as u32 * SCALE,
            )
            .position_centered()
            .build()
            .unwrap();

        let canvas = window.into_canvas().build().unwrap();

        Self(canvas)
    }
}

impl Renderer for Lcd {
    fn draw(&mut self, buffer: &[u8]) {
        let rgb_buffer: Vec<u8> = buffer
            .iter()
            .flat_map(|&palette_idx| {
                let rgb = match palette_idx {
                    0 => [0xE0, 0xF8, 0xD0],
                    1 => [0x88, 0xC0, 0x70],
                    2 => [0x34, 0x68, 0x56],
                    _ => [0x0E, 0x18, 0x20],
                };
                rgb.into_iter()
            })
            .collect();

        let texture_creator = self.0.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGB24, LCD_WIDTH as u32, LCD_HEIGHT as u32)
            .unwrap();

        texture.update(None, &rgb_buffer, LCD_WIDTH * 3).unwrap();
        self.0.clear();
        self.0.copy(&texture, None, None).unwrap();
        self.0.present();
    }
}
