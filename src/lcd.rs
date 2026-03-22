use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;
use sdl2::Sdl;

use crate::backend::Backend;
use crate::input::ButtonState;
use crate::ppu::{LCD_HEIGHT, LCD_WIDTH};

const SCALE: u32 = 4;

pub struct Lcd {
    canvas: Canvas<Window>,
    #[allow(dead_code)]
    sdl_context: Sdl,
    event_pump: EventPump,
}

impl Lcd {
    pub fn new() -> Self {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();
        let event_pump = sdl_context.event_pump().unwrap();

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

        Self { canvas, sdl_context, event_pump }
    }
}

impl Backend for Lcd {
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

    fn poll(&mut self) -> ButtonState {
        let mut state = ButtonState::default();
        for event in self.event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                state.quit = true;
            }
        }
        let keys: std::collections::HashSet<Keycode> = self
            .event_pump
            .keyboard_state()
            .pressed_scancodes()
            .filter_map(Keycode::from_scancode)
            .collect();

        state.a = keys.contains(&Keycode::Z);
        state.b = keys.contains(&Keycode::X);
        state.start = keys.contains(&Keycode::Return);
        state.select = keys.contains(&Keycode::RShift);
        state.up = keys.contains(&Keycode::Up);
        state.down = keys.contains(&Keycode::Down);
        state.left = keys.contains(&Keycode::Left);
        state.right = keys.contains(&Keycode::Right);
        state
    }
}
