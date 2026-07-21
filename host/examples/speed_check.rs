//! CGB ダブルスピードモードの検証用ヘッドレス実行。
//!
//! 使い方: cargo run -p gb-host --example speed_check <rom.gb> [emulated_seconds]
//!
//! 指定秒数ぶんエミュレートし、KEY1 によるダブルスピード切替の発生タイミングと、
//! フレームあたりの step 数 (通常速度=17556 / ダブルスピード=35112 が期待値) を出力する。

use gb_core::bootrom::Bootrom;
use gb_core::gameboy::GameBoy;
use gb_core::mmu::Mmu;
use gb_core::input::NullInput;
use gb_core::platform::{NullAudio, NullDisplay};
use gb_host::cartridge::Cartridge;

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args.next().expect("usage: speed_check <rom.gb> [seconds]");
    let seconds: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(30);

    let cart = Cartridge::new(&rom_path).expect("failed to load ROM");
    let mmu = Mmu::new(Bootrom::disabled(), cart);
    let mut gb = GameBoy::new(mmu, NullDisplay, NullAudio, NullInput);

    let total_steps = 1_048_576 * seconds;
    let mut prev_double = false;
    let mut frames: u64 = 0;
    let mut steps_at_last_frame: u64 = 0;
    let mut frame_gap_hist: std::collections::BTreeMap<u64, u64> = Default::default();

    for step in 0..total_steps {
        let r = gb.step();
        if r.double_speed != prev_double {
            println!(
                "step {:>10}: double_speed {} -> {}",
                step, prev_double, r.double_speed
            );
            prev_double = r.double_speed;
        }
        if r.frame_ready {
            frames += 1;
            let gap = step - steps_at_last_frame;
            steps_at_last_frame = step;
            *frame_gap_hist.entry(gap).or_default() += 1;
        }
    }

    println!("frames: {frames} in {seconds}s of emulated M-cycles");
    println!("frame gap histogram (steps between frame_ready):");
    for (gap, count) in frame_gap_hist {
        println!("  {gap:>7} steps x {count}");
    }
}
