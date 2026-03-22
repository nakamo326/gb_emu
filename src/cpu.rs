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
    /// HALT バグ: 次の fetch() で PC をインクリメントしない
    halt_bug: bool,
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
            halt_bug: false,
        }
    }

    /// BootROM をスキップして DMG の初期レジスタ値をセットする
    pub fn apply_dmg_init(&mut self) {
        self.regs.a = 0x01;
        self.regs.f = 0xB0;
        self.regs.b = 0x00;
        self.regs.c = 0x13;
        self.regs.d = 0x00;
        self.regs.e = 0xD8;
        self.regs.h = 0x01;
        self.regs.l = 0x4D;
        self.regs.sp = 0xFFFE;
        self.regs.pc = 0x0100;
    }

    pub fn fetch(&mut self, bus: &Mmu) {
        self.ctx.opcode = bus.read(self.regs.pc);
        if self.halt_bug {
            // HALT バグ: PC をインクリメントしない（次命令の第1オペランドが opcode と同じアドレスになる）
            self.halt_bug = false;
        } else {
            self.regs.pc = self.regs.pc.wrapping_add(1);
        }
        self.ctx.cb = false;
        self.at_instruction_start = true;
    }

    pub fn emulate_cycle(&mut self, bus: &mut Mmu) {
        if self.at_instruction_start {
            self.at_instruction_start = false;

            // EI の遅延処理: 前の命令が EI だったら今ここで IME を有効化
            // ただし EI 直後の1命令は必ず実行してから割り込みをチェックする
            let was_ei_delayed = self.ei_delay;
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

            // 割り込みディスパッチ（EI の直後サイクルはスキップ）
            if self.ime && pending != 0 && !was_ei_delayed {
                for bit in 0..5u8 {
                    if pending & (1 << bit) != 0 {
                        self.ime = false;
                        bus.if_ &= !(1 << bit);
                        let vector = 0x0040u16 + (bit as u16) * 0x08;
                        // PC をスタックに積む
                        // fetch() が opcode 読み取り時に PC を +1 しているため、
                        // 戻るべき命令アドレスは PC - 1（pre-fetch した命令のアドレス）
                        let return_pc = self.regs.pc.wrapping_sub(1);
                        self.regs.sp = self.regs.sp.wrapping_sub(1);
                        bus.write(self.regs.sp, (return_pc >> 8) as u8);
                        self.regs.sp = self.regs.sp.wrapping_sub(1);
                        bus.write(self.regs.sp, return_pc as u8);
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
