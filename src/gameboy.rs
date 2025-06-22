use std::time;

use crate::cpu::Cpu;
use crate::mmu::Mmu;
use crate::renderer::Renderer;

pub const CPU_CLOCK_HZ: u128 = 4_194_304;
pub const M_CYCLE_CLOCK: u128 = 4;
const M_CYCLE_NANOS: u128 = M_CYCLE_CLOCK * 1_000_000_000 / CPU_CLOCK_HZ;

pub struct GameBoy {
    cpu: Cpu,
    mmu: Mmu,
    lcd: Box<dyn Renderer>,
}

impl GameBoy {
    pub fn new(lcd: Box<dyn Renderer>) -> Self {
        Self {
            cpu: Cpu::new(),
            mmu: Mmu::new(),
            lcd,
        }
    }

    pub fn run(&mut self) {
        let time = time::Instant::now();
        let mut elapsed = 0;
        loop {
            let e = time.elapsed().as_nanos();
            for _ in 0..(e - elapsed) / M_CYCLE_NANOS {
                self.cpu.emulate_cycle(&mut self.mmu);
                if self.mmu.ppu.emulate_cycle() {
                    self.lcd.draw(self.mmu.ppu.pixel_buffer());
                }
                elapsed += M_CYCLE_NANOS;
            }
            std::thread::sleep(time::Duration::from_nanos(M_CYCLE_NANOS as u64));
        }
    }
}
