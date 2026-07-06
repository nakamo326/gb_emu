//! GBA のホスト実行ループ。SDL2 での表示・入力と、フレーム単位のペーシングを行う。
//!
//! GB 側の M-cycle 追従ループとは違い、`run_frame()` で 1 フレーム分を
//! 一気に実行してから描画・待機する方式（GBA はフレーム内のリアルタイム性を
//! ホスト側で保つ必要がないため単純な方を選んだ）。

use gba_core::gba::Gba;
use gba_core::ppu::{HEIGHT, WIDTH};
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::pixels::PixelFormatEnum;
use std::time::{Duration, Instant};

const SCALE: u32 = 3;
/// 59.7275 Hz
const FRAME_NS: u64 = 16_742_706;

pub fn run(rom_path: &str) {
    let rom = match std::fs::read(rom_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to load '{}': {}", rom_path, e);
            std::process::exit(1);
        }
    };
    println!("Loaded: {}", rom_path);

    let bios = std::fs::read("gba_bios.bin").ok().filter(|b| b.len() == 0x4000);
    if bios.is_some() {
        println!("Using gba_bios.bin");
    } else {
        println!("gba_bios.bin not found, using HLE BIOS");
    }
    let mut gba = Gba::new(rom, bios);

    // SRAM セーブのロード（<rom>.sav）
    let sav_path = std::path::Path::new(rom_path).with_extension("sav");
    if let Ok(data) = std::fs::read(&sav_path) {
        let n = data.len().min(gba.bus.sram.len());
        gba.bus.sram[..n].copy_from_slice(&data[..n]);
        println!("Loaded save: {}", sav_path.display());
    }

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let mut event_pump = sdl.event_pump().unwrap();
    let window = video
        .window("GBA Emulator", WIDTH as u32 * SCALE, HEIGHT as u32 * SCALE)
        .position_centered()
        .resizable()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().accelerated().present_vsync().build().unwrap();
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGB24, WIDTH as u32, HEIGHT as u32)
        .unwrap();

    let start = Instant::now();
    let mut next_frame = Duration::ZERO;
    let mut rgb = vec![0u8; WIDTH * HEIGHT * 3];
    'main: loop {
        gba.run_frame();

        for (i, &px) in gba.framebuffer().iter().enumerate() {
            rgb[i * 3] = ((px & 0x1F) << 3) as u8;
            rgb[i * 3 + 1] = ((px >> 5 & 0x1F) << 3) as u8;
            rgb[i * 3 + 2] = ((px >> 10 & 0x1F) << 3) as u8;
        }
        texture.update(None, &rgb, WIDTH * 3).unwrap();
        canvas.clear();
        canvas.copy(&texture, None, None).unwrap();
        canvas.present();

        for event in event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                break 'main;
            }
        }
        let kb = event_pump.keyboard_state();
        let mut keys = 0u16;
        for (bit, sc) in [
            (0, Scancode::Z),         // A
            (1, Scancode::X),         // B
            (2, Scancode::RShift),    // Select
            (3, Scancode::Return),    // Start
            (4, Scancode::Right),
            (5, Scancode::Left),
            (6, Scancode::Up),
            (7, Scancode::Down),
            (8, Scancode::S),         // R
            (9, Scancode::A),         // L
        ] {
            if kb.is_scancode_pressed(sc) {
                keys |= 1 << bit;
            }
        }
        gba.set_keys(keys);

        // フレームペーシング。大きく遅れたら追いつきを諦めて基準を取り直す
        next_frame += Duration::from_nanos(FRAME_NS);
        let elapsed = start.elapsed();
        if let Some(sleep) = next_frame.checked_sub(elapsed) {
            std::thread::sleep(sleep);
        } else if elapsed > next_frame + Duration::from_millis(500) {
            next_frame = elapsed;
        }
    }

    if gba.bus.sram_dirty {
        match std::fs::write(&sav_path, &gba.bus.sram) {
            Ok(_) => println!("Saved: {}", sav_path.display()),
            Err(e) => eprintln!("Failed to save '{}': {}", sav_path.display(), e),
        }
    }
}
