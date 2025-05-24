use sdl2::Sdl;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::ppu::{LCD_HEIGHT, LCD_WIDTH};

pub struct LCD(Canvas<Window>);
impl LCD {
    pub fn new(sdl: &Sdl, scale: u32) -> Self {
        let window = sdl
            .video()
            .unwrap()
            .window(
                "Game Boy Emulator",
                LCD_WIDTH as u32 * scale,
                LCD_HEIGHT as u32 * scale,
            )
            .position_centered()
            .build()
            .unwrap();

        let canvas = window.into_canvas().build().unwrap();

        Self(canvas)
    }

    pub fn draw(&mut self, pixels: Box<[u8]>) {
        let texture_creator = self.0.texture_creator();
        let mut texture = texture_creator
            .create_texture_target(PixelFormatEnum::RGB24, LCD_WIDTH as u32, LCD_HEIGHT as u32)
            .unwrap();

        texture.update(None, &pixels, LCD_WIDTH * 3).unwrap();
        self.0.clear();
        self.0.copy(&texture, None, None).unwrap();
        self.0.present();
    }
}
