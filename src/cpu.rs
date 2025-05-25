mod decode;
mod instructions;
mod operand;
mod registers;

use crate::mmu::Mmu;
use registers::Registers;

#[derive(Default)]
struct Ctx {
    opcode: u8,
    cb: bool,
}

pub struct Cpu {
    regs: Registers,
    ctx: Ctx,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            ctx: Ctx::default(),
        }
    }

    pub fn fetch(&mut self, bus: &Mmu) {
        self.ctx.opcode = bus.read(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        self.ctx.cb = false;
    }

    fn nop(&mut self, bus: &Mmu) {
        self.fetch(bus);
    }

    pub fn emulate_cycle(&mut self, bus: &mut Mmu) {
        self.decode(bus);
    }
}
