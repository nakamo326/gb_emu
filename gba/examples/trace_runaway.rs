//! デバッグ用: PC が ROM/RAM/BIOS 外へ飛んだ時点で直前の実行履歴を出力する。

use gba_core::gba::Gba;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: trace_runaway <rom.gba>");
    let rom = std::fs::read(path).unwrap();
    let mut gba = Gba::new(rom, None);

    let mut history: Vec<(u32, bool, u32, u32)> = Vec::new(); // (pc, thumb, opcode, r14)
    for i in 0u64..300_000_000 {
        let pc = gba.cpu.regs[15];
        let region = pc >> 24;
        let ok = matches!(region, 0x0 | 0x2 | 0x3 | 0x8..=0xD);
        if !ok {
            println!("runaway at step {}: pc={:08X}", i, pc);
            for (p, t, op, lr) in history.iter().rev().take(60).rev() {
                println!("  {:08X} {} {:08X} lr={:08X}", p, if *t { "T" } else { "A" }, op, lr);
            }
            return;
        }
        if !gba.cpu.halted {
            let op = if gba.cpu.thumb() {
                gba.bus.read16(pc) as u32
            } else {
                gba.bus.read32(pc)
            };
            history.push((pc, gba.cpu.thumb(), op, gba.cpu.regs[14]));
            if history.len() > 100 {
                history.remove(0);
            }
        }
        gba.step();
    }
    println!("no runaway detected");
}
