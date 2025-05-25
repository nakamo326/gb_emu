use std::time;

use crate::bootrom::Bootrom;
use crate::cpu::Cpu;
use crate::lcd::LCD;
use crate::peripherals::Peripherals;

pub const CPU_CLOCK_HZ: u128 = 4_194_304;
pub const M_CYCLE_CLOCK: u128 = 4;
const M_CYCLE_NANOS: u128 = M_CYCLE_CLOCK * 1_000_000_000 / CPU_CLOCK_HZ;

pub struct GameBoy {
    cpu: Cpu,
    peripherals: Peripherals,
    lcd: LCD,
}

impl GameBoy {
    pub fn new(bootrom: Bootrom) -> Self {
        let sdl = sdl2::init().unwrap();

        Self {
            cpu: Cpu::new(),
            peripherals: Peripherals::new(bootrom),
            lcd: LCD::new(&sdl, 4),
        }
    }

    pub fn run(&mut self) {
        let time = time::Instant::now();
        let mut elapsed = 0;
        loop {
            let e = time.elapsed().as_nanos();
            for _ in 0..(e - elapsed) / M_CYCLE_NANOS {
                self.cpu.emulate_cycle(&mut self.peripherals);
                if self.peripherals.ppu.emulate_cycle() {
                    self.lcd.draw(self.peripherals.ppu.pixel_buffer());
                }
                elapsed += M_CYCLE_NANOS;
            }
            std::thread::sleep(time::Duration::from_nanos(M_CYCLE_NANOS as u64));
        }
    }
}
