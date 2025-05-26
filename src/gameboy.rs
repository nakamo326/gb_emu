use std::cell::RefCell;
use std::rc::Rc;
use std::time;

use crate::bootrom::Bootrom;
use crate::cpu::Cpu;
use crate::hram::HRam;
use crate::lcd::Lcd;
use crate::mmu::Mmu;
use crate::ppu::Ppu;
use crate::wram::WRam;

pub const CPU_CLOCK_HZ: u128 = 4_194_304;
pub const M_CYCLE_CLOCK: u128 = 4;
const M_CYCLE_NANOS: u128 = M_CYCLE_CLOCK * 1_000_000_000 / CPU_CLOCK_HZ;

pub struct GameBoy {
    cpu: Cpu,
    bootrom: Rc<RefCell<Bootrom>>,
    wram: Rc<RefCell<WRam>>,
    hram: Rc<RefCell<HRam>>,
    ppu: Rc<RefCell<Ppu>>,
    mmu: Mmu,
    lcd: Lcd,
}

impl GameBoy {
    pub fn new() -> Self {
        let bootrom = Rc::new(RefCell::new(Bootrom::new("dmg_bootrom.bin")));
        let wram = Rc::new(RefCell::new(WRam::new()));
        let hram = Rc::new(RefCell::new(HRam::new()));
        let ppu = Rc::new(RefCell::new(Ppu::new()));
        let mmu = Mmu {
            bootrom: bootrom.clone(),
            wram: wram.clone(),
            hram: hram.clone(),
            ppu: ppu.clone(),
        };

        Self {
            cpu: Cpu::new(),
            bootrom,
            wram,
            hram,
            ppu,
            mmu,
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
                if self.mmu.ppu.borrow_mut().emulate_cycle() {
                    self.lcd.draw(self.mmu.ppu.borrow().pixel_buffer());
                }
                elapsed += M_CYCLE_NANOS;
            }
            std::thread::sleep(time::Duration::from_nanos(M_CYCLE_NANOS as u64));
        }
    }
}
