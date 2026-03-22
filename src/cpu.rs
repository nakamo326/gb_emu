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
    /// 割り込みマスタ有効フラグ
    pub ime: bool,
    /// EI 命令後の1命令遅延フラグ
    ei_delay: bool,
    /// HALT 状態フラグ
    halted: bool,
    /// fetch() が呼ばれた直後（命令境界）かどうか
    at_instruction_start: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            ctx: Ctx::default(),
            ime: false,
            ei_delay: false,
            halted: false,
            at_instruction_start: false,
        }
    }

    pub fn fetch(&mut self, bus: &Mmu) {
        self.ctx.opcode = bus.read(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        self.ctx.cb = false;
        self.at_instruction_start = true;
    }

    pub fn emulate_cycle(&mut self, bus: &mut Mmu) {
        if self.at_instruction_start {
            self.at_instruction_start = false;

            // EI の遅延処理: 前の命令が EI だったら今ここで IME を有効化
            if self.ei_delay {
                self.ei_delay = false;
                self.ime = true;
            }

            let ie = bus.ie;
            let if_ = bus.if_;
            let pending = ie & if_ & 0x1F;

            // HALT から割り込みで復帰
            if self.halted {
                if pending != 0 {
                    self.halted = false;
                } else {
                    // まだ HALT 中: 命令境界フラグを維持して待機
                    self.at_instruction_start = true;
                    return;
                }
            }

            // 割り込みディスパッチ
            if self.ime && pending != 0 {
                for bit in 0..5u8 {
                    if pending & (1 << bit) != 0 {
                        self.ime = false;
                        bus.if_ &= !(1 << bit);
                        let vector = 0x0040u16 + (bit as u16) * 0x08;
                        // PC をスタックに積む
                        self.regs.sp = self.regs.sp.wrapping_sub(1);
                        bus.write(self.regs.sp, (self.regs.pc >> 8) as u8);
                        self.regs.sp = self.regs.sp.wrapping_sub(1);
                        bus.write(self.regs.sp, self.regs.pc as u8);
                        self.regs.pc = vector;
                        self.fetch(bus);
                        return;
                    }
                }
            }
        }

        self.decode(bus);
    }
}
