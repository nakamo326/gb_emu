use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;

use crate::ppu::{LCD_HEIGHT, LCD_WIDTH};
use crate::renderer::Renderer;

const SCALE: u32 = 4;

pub struct Lcd {
    canvas: Canvas<Window>,
    #[allow(dead_code)]
    sdl_context: Sdl,
}

impl Lcd {
    pub fn new() -> Self {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();
        
        let window = video_subsystem
            .window(
                "Game Boy Emulator",
                LCD_WIDTH as u32 * SCALE,
                LCD_HEIGHT as u32 * SCALE,
            )
            .position_centered()
            .resizable()
            .build()
            .unwrap();

        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .unwrap();
            
        canvas.set_draw_color(sdl2::pixels::Color::RGB(0x0E, 0x18, 0x20));
        canvas.clear();
        canvas.present();

        Self { canvas, sdl_context }
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

        let texture_creator = self.canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGB24, LCD_WIDTH as u32, LCD_HEIGHT as u32)
            .unwrap();

        texture.update(None, &rgb_buffer, LCD_WIDTH * 3).unwrap();
        self.canvas.clear();
        self.canvas.copy(&texture, None, None).unwrap();
        self.canvas.present();
    }

}
