//! 一時的な調査用ツール: ROM を高速実行し、PC/LCDC/CGB パレット等を定期的にダンプする。
//! GBC ROM が白画面のまま固まる問題の切り分けに使用。
//! 使い方: cargo run --example trace -- <rom_path> [max_frames]

use gb_core::bootrom::Bootrom;
use gb_core::gameboy::GameBoy;
use gb_core::input::NullInput;
use gb_core::mmu::Mmu;
use gb_core::platform::NullAudio;
use std::env;
use std::path::Path;

fn region(pc: u16) -> &'static str {
    match pc {
        0x0000..=0x3FFF => "ROM0",
        0x4000..=0x7FFF => "ROMX",
        0x8000..=0x9FFF => "VRAM",
        0xA000..=0xBFFF => "SRAM",
        0xC000..=0xCFFF => "WRAM0",
        0xD000..=0xDFFF => "WRAMX",
        0xE000..=0xFDFF => "ECHO!",
        0xFE00..=0xFE9F => "OAM!",
        0xFEA0..=0xFEFF => "UNUSED!",
        0xFF00..=0xFF7F => "IO!",
        0xFF80..=0xFFFE => "HRAM",
        0xFFFF => "IE!",
    }
}

struct DumpDisplay;
impl gb_core::platform::Display for DumpDisplay {
    fn draw(&mut self, _pixels: &[u16]) {}
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let rom_path = args.get(1).expect("usage: trace <rom_path> [max_frames|instrs:N]");
    let instr_mode = args.get(2).map(|s| s.as_str()) == Some("instrs");
    let max_frames: u32 = if instr_mode {
        0
    } else {
        args.get(2).and_then(|s| s.parse().ok()).unwrap_or(600)
    };
    let max_instrs: u64 = if instr_mode {
        args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2000)
    } else {
        0
    };

    if Path::new("dmg_bootrom.bin").exists() {
        eprintln!("(dmg_bootrom.bin が存在しますが、trace では常に無効化して CGB init を使います)");
    }
    let bootrom = Bootrom::disabled();

    let cart = gb_host::cartridge::Cartridge::new(rom_path).expect("failed to load ROM");
    println!(
        "cart: title={:?} type={:?} cgb_flag=0x{:02X}",
        cart.header().title,
        cart.header().cartridge_type,
        cart.header().cgb_flag
    );

    let mmu = Mmu::new(bootrom, cart);
    let mut gb = GameBoy::new(mmu, DumpDisplay, NullAudio, NullInput);

    if instr_mode {
        let mut last_pc = u16::MAX;
        let mut count = 0u64;
        loop {
            gb.step();
            let (pc, halted, ime) = gb.debug_cpu();
            if pc != last_pc {
                last_pc = pc;
                count += 1;
                let (a, hl, sp) = gb.debug_regs();
                println!(
                    "#{count:5} pc=0x{pc:04X} a=0x{a:02X} hl=0x{hl:04X} sp=0x{sp:04X} halted={halted} ime={ime} region={}",
                    region(pc)
                );
                if count >= max_instrs {
                    break;
                }
            }
        }
        return;
    }

    let mut frame = 0u32;
    let mut last_lcdc = 0xFFu16;
    loop {
        let r = gb.step();
        if r.frame_ready {
            frame += 1;
            if frame % 60 == 0 || frame <= 5 {
                let (pc, halted, ime) = gb.debug_cpu();
                let lcdc = gb.mmu().ppu.lcdc();
                let pal0 = gb.mmu().ppu.bg_palette_color0(0);
                let key1_dbl = gb.mmu().double_speed();
                println!(
                    "frame={:5} pc=0x{:04X} halted={} ime={} lcdc=0x{:02X} pal0.0=0x{:04X} dbl={}",
                    frame, pc, halted, ime, lcdc, pal0, key1_dbl
                );
                if lcdc as u16 != last_lcdc {
                    last_lcdc = lcdc as u16;
                }
            }
            if frame >= max_frames {
                break;
            }
        }
    }
}
