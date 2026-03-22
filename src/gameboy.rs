use std::time;

use crate::backend::{Backend, NullBackend};
use crate::cpu::Cpu;
use crate::mmu::Mmu;

pub const CPU_CLOCK_HZ: u128 = 4_194_304;
pub const M_CYCLE_CLOCK: u128 = 4;
const M_CYCLE_NANOS: u128 = M_CYCLE_CLOCK * 1_000_000_000 / CPU_CLOCK_HZ;

pub struct GameBoy {
    cpu: Cpu,
    mmu: Mmu,
    backend: Box<dyn Backend>,
    headless: bool,
}

impl GameBoy {
    pub fn new(backend: Box<dyn Backend>, headless: bool) -> Self {
        let mut cpu = Cpu::new();
        let mut mmu = Mmu::new();
        if !mmu.bootrom.is_active() {
            cpu.apply_dmg_init();
            mmu.apply_dmg_init();
        }
        Self { cpu, mmu, backend, headless }
    }

    pub fn new_headless() -> Self {
        Self::new(Box::new(NullBackend), true)
    }

    pub fn load_cartridge(&mut self, rom_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.mmu.load_cartridge(rom_path)
    }

    pub fn run(&mut self) {
        let time = time::Instant::now();
        let mut elapsed = 0;
        loop {
            let e = time.elapsed().as_nanos();
            for _ in 0..(e - elapsed) / M_CYCLE_NANOS {
                self.cpu.emulate_cycle(&mut self.mmu);

                // タイマー割り込み
                if self.mmu.timer.emulate_cycle() {
                    self.mmu.if_ |= 0x04;
                }

                // APU サンプル生成
                let samples = self.mmu.apu.emulate_cycle();
                if !samples.is_empty() {
                    self.backend.push_audio(&samples);
                }

                // PPU 割り込み
                if self.mmu.ppu.emulate_cycle() {
                    self.backend.draw(self.mmu.ppu.pixel_buffer());
                    // VBlank のタイミングで入力をポーリング
                    let btn = self.backend.poll();
                    if btn.quit {
                        return;
                    }
                    self.mmu.update_joypad(&btn);
                }
                if self.mmu.ppu.vblank_irq {
                    self.mmu.ppu.vblank_irq = false;
                    self.mmu.if_ |= 0x01;
                }
                if self.mmu.ppu.stat_irq {
                    self.mmu.ppu.stat_irq = false;
                    self.mmu.if_ |= 0x02;
                }

                elapsed += M_CYCLE_NANOS;
            }
            if self.headless {
                if self.mmu.test_done {
                    break;
                }
            } else {
                std::thread::sleep(time::Duration::from_nanos(M_CYCLE_NANOS as u64));
            }
        }
    }
}
