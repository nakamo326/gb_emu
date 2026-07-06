//! ヘッドレス実行ツール: ROM を指定フレーム数実行し、フレームバッファの
//! 統計を出力して最終フレームを PPM 画像で保存する（描画確認・デバッグ用）。
//!
//! 使い方: cargo run -p gba-core --release --example headless <rom.gba> [frames] [out.ppm]

use gba_core::gba::Gba;
use gba_core::ppu::{HEIGHT, WIDTH};
use std::collections::HashSet;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: headless <rom.gba> [frames] [out.ppm]");
    let frames: u32 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(300);
    let out = args.get(3).map(String::as_str).unwrap_or("gba_frame.ppm");

    let rom = std::fs::read(path).unwrap();
    let bios = std::fs::read("gba_bios.bin").ok().filter(|b| b.len() == 0x4000);
    println!("bios: {}", if bios.is_some() { "real" } else { "HLE" });
    let mut gba = Gba::new(rom, bios);

    for f in 0..frames {
        gba.run_frame();
        if (f + 1) % 60 == 0 {
            let fb = gba.framebuffer();
            let nonzero = fb.iter().filter(|&&p| p != 0).count();
            let uniq: HashSet<u16> = fb.iter().copied().collect();
            println!("frame {:4}: nonzero={:5} unique_colors={}", f + 1, nonzero, uniq.len());
        }
    }

    println!(
        "cpu: pc={:08X} thumb={} halted={} ie={:04X} if={:04X} ime={}",
        gba.cpu.regs[15],
        gba.cpu.thumb(),
        gba.cpu.halted,
        gba.bus.ie,
        gba.bus.if_,
        gba.bus.ime,
    );

    let mut ppm = format!("P6\n{} {}\n255\n", WIDTH, HEIGHT).into_bytes();
    for &px in gba.framebuffer() {
        ppm.push(((px & 0x1F) << 3) as u8);
        ppm.push(((px >> 5 & 0x1F) << 3) as u8);
        ppm.push(((px >> 10 & 0x1F) << 3) as u8);
    }
    std::fs::write(out, ppm).unwrap();
    println!("wrote {}", out);
}
