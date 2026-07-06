mod gba_run;
mod lcd;
mod renderer;

use gb_host::cartridge;

use gb_core::bootrom::Bootrom;
use gb_core::gameboy::{GameBoy, StepResult};
use gb_core::input::NullInput;
use gb_core::mmu::Mmu;
use gb_core::platform::{CartridgeBus, NullAudio, NullDisplay};
use std::time::{Duration, Instant};

const M_CYCLE_NS: u128 = 4 * 1_000_000_000 / 4_194_304;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let rom_path =
        args.iter().skip(1).find(|a| *a != "--headless").map(|s| s.as_str());

    // .gba は GBA モードで起動（GB とはコア・表示・ループがすべて別）
    if let Some(path) = rom_path.filter(|p| p.ends_with(".gba")) {
        gba_run::run(path);
        return;
    }

    let bootrom = load_bootrom();

    // ROM パス解決: 引数 → test_rom.gb → cpu_instrs.gb の順で探す
    let resolved_path = rom_path.or_else(|| {
        if std::path::Path::new("test_rom.gb").exists() {
            Some("test_rom.gb")
        } else if std::path::Path::new("cpu_instrs.gb").exists() {
            Some("cpu_instrs.gb")
        } else {
            None
        }
    });

    if headless {
        let path = match resolved_path {
            Some(p) => p,
            None => {
                eprintln!("No ROM found for headless mode");
                std::process::exit(1);
            }
        };
        let cart = match cartridge::Cartridge::new(path) {
            Ok(c) => {
                println!("Loaded: {}", path);
                c
            }
            Err(e) => {
                eprintln!("Failed to load '{}': {}", path, e);
                std::process::exit(1);
            }
        };
        let mmu = Mmu::new(bootrom, cart);
        let mut gb = GameBoy::new(mmu, NullDisplay, NullAudio, NullInput);
        run_headless(&mut gb);
    } else {
        let (display, audio, input) = lcd::create_sdl_backends();
        match resolved_path.and_then(|p| {
            cartridge::Cartridge::new(p)
                .map_err(|e| eprintln!("Failed to load '{}': {}", p, e))
                .ok()
                .map(|c| (p, c))
        }) {
            Some((path, cart)) => {
                println!("Loaded: {}", path);
                let mmu = Mmu::new(bootrom, cart);
                let mut gb = GameBoy::new(mmu, display, audio, input);
                run_loop(|| gb.step());
            }
            None => {
                println!("No ROM found, running without cartridge");
                use gb_core::platform::NullCartridge;
                let mmu = Mmu::new(bootrom, NullCartridge);
                let mut gb = GameBoy::new(mmu, display, audio, input);
                run_loop(|| gb.step());
            }
        }
    }
}

fn load_bootrom() -> Bootrom {
    match std::fs::read("dmg_bootrom.bin") {
        Ok(bytes) if bytes.len() >= 0x100 => {
            let mut arr = [0u8; 0x100];
            arr.copy_from_slice(&bytes[..0x100]);
            Bootrom::from_bytes(arr)
        }
        _ => {
            eprintln!("Warning: dmg_bootrom.bin not found, using DMG init values");
            Bootrom::disabled()
        }
    }
}

/// wall-clock catch-up 方式のメインループ。
/// step() を現実時間に追いつくペースで呼び出し、quit が立ったら終了する。
/// CGB ダブルスピード時は M_CYCLE_NS を半分にしてタイミングを調整する。
fn run_loop(mut step: impl FnMut() -> StepResult) {
    let start = Instant::now();
    let mut emulated_ns: u128 = 0;
    let mut cycle_ns = M_CYCLE_NS;

    loop {
        let elapsed = start.elapsed().as_nanos();
        let cycles = (elapsed - emulated_ns) / cycle_ns;
        let mut last = StepResult::default();
        for _ in 0..cycles {
            last = step();
            if last.quit {
                return;
            }
        }
        emulated_ns += cycles * cycle_ns;
        // ダブルスピード切替時にサイクル長を更新
        cycle_ns = if last.double_speed { M_CYCLE_NS / 2 } else { M_CYCLE_NS };
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// テストハーネス付きヘッドレスループ。タイミング制約なしで全力実行する。
/// gb-host は常に gb-core の test-harness フィーチャーを有効化しているため無条件に使用する。
fn run_headless<C: CartridgeBus>(
    gb: &mut GameBoy<C, NullDisplay, NullAudio, NullInput>,
) {
    loop {
        gb.step();
        if gb.mmu().test.test_done {
            let log = &gb.mmu().test.serial_log;
            if !log.is_empty() {
                let text = core::str::from_utf8(log).unwrap_or("(invalid utf8)");
                print!("{}", text);
                if !log.ends_with(b"\n") {
                    println!();
                }
            }
            let ram = &gb.mmu().test.ram_text_buf;
            if !ram.is_empty() {
                let text = core::str::from_utf8(ram).unwrap_or("(invalid utf8)");
                print!("{}", text);
            }
            break;
        }
    }
}
