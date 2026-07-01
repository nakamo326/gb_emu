use gb_core::input::{ButtonState, InputSource};
use gb_core::platform::{AudioSink, Display};
use gb_core::ppu::{LCD_HEIGHT, LCD_WIDTH};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;
use sdl2::Sdl;

const SCALE: u32 = 4;
// AudioQueue のバッファ上限: 約 100ms 分（44100 * 2ch * 4bytes * 0.1sec）
const AUDIO_QUEUE_MAX_BYTES: u32 = 35280;

pub struct SdlDisplay {
    canvas: Canvas<Window>,
    #[allow(dead_code)]
    sdl_context: Sdl,
}

pub struct SdlAudio {
    audio_queue: Option<AudioQueue<f32>>,
}

pub struct SdlInput {
    event_pump: EventPump,
}

pub fn create_sdl_backends() -> (SdlDisplay, SdlAudio, SdlInput) {
    let sdl_context = sdl2::init().unwrap();
    let video = sdl_context.video().unwrap();
    let event_pump = sdl_context.event_pump().unwrap();

    let window = video
        .window(
            "Game Boy Emulator",
            LCD_WIDTH as u32 * SCALE,
            LCD_HEIGHT as u32 * SCALE,
        )
        .position_centered()
        .resizable()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().accelerated().present_vsync().build().unwrap();
    canvas.set_draw_color(sdl2::pixels::Color::RGB(0x0E, 0x18, 0x20));
    canvas.clear();
    canvas.present();

    let audio_queue = sdl_context.audio().ok().and_then(|audio| {
        let desired_spec = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(2),
            samples: None,
        };
        audio.open_queue::<f32, _>(None, &desired_spec).ok().map(|q| {
            q.resume();
            q
        })
    });
    if audio_queue.is_none() {
        eprintln!("Warning: audio device unavailable, running without sound");
    }

    (
        SdlDisplay { canvas, sdl_context },
        SdlAudio { audio_queue },
        SdlInput { event_pump },
    )
}

impl Display for SdlDisplay {
    fn draw(&mut self, buffer: &[u16]) {
        // RGB555 (bits 0-4=R, 5-9=G, 10-14=B) → RGB24
        let rgb_buffer: Vec<u8> = buffer
            .iter()
            .flat_map(|&px| {
                let r = ((px & 0x1F) as u8) << 3;
                let g = (((px >> 5) & 0x1F) as u8) << 3;
                let b = (((px >> 10) & 0x1F) as u8) << 3;
                [r, g, b]
            })
            .collect();

        let texture_creator = self.canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGB24,
                LCD_WIDTH as u32,
                LCD_HEIGHT as u32,
            )
            .unwrap();
        texture.update(None, &rgb_buffer, LCD_WIDTH * 3).unwrap();
        self.canvas.clear();
        self.canvas.copy(&texture, None, None).unwrap();
        self.canvas.present();
    }
}

impl AudioSink for SdlAudio {
    fn push(&mut self, left: f32, right: f32) {
        if let Some(q) = &mut self.audio_queue {
            if q.size() < AUDIO_QUEUE_MAX_BYTES {
                let _ = q.queue_audio(&[left, right]);
            }
        }
    }
}

impl InputSource for SdlInput {
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
