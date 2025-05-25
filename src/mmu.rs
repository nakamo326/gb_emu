use std::cell::RefCell;
use std::rc::Rc;

use crate::bootrom::Bootrom;
use crate::hram::HRam;
use crate::ppu::Ppu;
use crate::wram::WRam;

pub struct Mmu {
    pub bootrom: Rc<RefCell<Bootrom>>,
    pub wram: Rc<RefCell<WRam>>,
    pub hram: Rc<RefCell<HRam>>,
    pub ppu: Rc<RefCell<Ppu>>,
}

impl Mmu {
    pub fn new(
        bootrom: Rc<RefCell<Bootrom>>,
        wram: Rc<RefCell<WRam>>,
        hram: Rc<RefCell<HRam>>,
        ppu: Rc<RefCell<Ppu>>,
    ) -> Self {
        Self {
            bootrom,
            wram,
            hram,
            ppu,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF => {
                if self.bootrom.borrow().is_active() {
                    self.bootrom.borrow().read(addr)
                } else {
                    0xFF
                }
            }
            0x8000..=0x9FFF => self.ppu.borrow().read(addr),
            0xFE00..=0xFE9F => self.ppu.borrow().read(addr),
            0xFF40..=0xFF4B => self.ppu.borrow().read(addr),
            0xC000..=0xFDFF => self.wram.borrow().read(addr),
            0xFF80..=0xFFFE => self.hram.borrow().read(addr),
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x8000..=0x9FFF => self.ppu.borrow_mut().write(addr, val),
            0xFE00..=0xFE9F => self.ppu.borrow_mut().write(addr, val),
            0xFF40..=0xFF4B => self.ppu.borrow_mut().write(addr, val),
            0xC000..=0xFDFF => self.wram.borrow_mut().write(addr, val),
            0xFF50 => self.bootrom.borrow_mut().write(addr, val),
            0xFF80..=0xFFFE => self.hram.borrow_mut().write(addr, val),
            _ => (),
        }
    }
}
