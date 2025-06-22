use std::time;

use crate::cpu::Cpu;
use crate::lcd::Lcd;
use crate::mmu::Mmu;

pub const CPU_CLOCK_HZ: u128 = 4_194_304;
pub const M_CYCLE_CLOCK: u128 = 4;
const M_CYCLE_NANOS: u128 = M_CYCLE_CLOCK * 1_000_000_000 / CPU_CLOCK_HZ;

pub struct GameBoy {
    cpu: Cpu,
    mmu: Mmu,
    lcd: Lcd,
}

impl GameBoy {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            mmu: Mmu::new(),
            lcd: Lcd::new(),
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
