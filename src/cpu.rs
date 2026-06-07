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

#[cfg(test)]
mod tests {
    //! CPU再設計のベースライン安全網。
    //! 現行(static)実装はグローバル状態を共有するため `cargo test -- --test-threads=1` で直列実行する。
    //! 再設計後も同じ期待値が通ることを完了条件とする（サイクル数・フラグ・結果・割り込みタイミング）。
    use super::*;

    const PROG: u16 = 0xC000; // プログラム配置先(WRAM)
    const STACK: u16 = 0xD000; // スタック初期値

    /// プログラムを WRAM に配置し、最初の opcode を fetch 済みにした Cpu/Mmu を返す
    fn setup(program: &[u8]) -> (Cpu, Mmu) {
        let mut mmu = Mmu::new();
        let mut cpu = Cpu::new();
        for (i, &b) in program.iter().enumerate() {
            mmu.write(PROG + i as u16, b);
        }
        cpu.regs.pc = PROG;
        cpu.regs.sp = STACK;
        cpu.fetch(&mmu);
        (cpu, mmu)
    }

    /// 指定 M-cycle 数だけ実行
    fn run(cpu: &mut Cpu, mmu: &mut Mmu, cycles: usize) {
        for _ in 0..cycles {
            cpu.emulate_cycle(mmu);
        }
    }

    // ── LD 8bit ─────────────────────────────
    #[test]
    fn ld_b_c_1cycle() {
        let (mut c, mut m) = setup(&[0x41]); // LD B,C
        c.regs.c = 0x55;
        run(&mut c, &mut m, 1);
        assert_eq!(c.regs.b, 0x55);
    }

    #[test]
    fn ld_a_d8_2cycle() {
        let (mut c, mut m) = setup(&[0x3E, 0x42]); // LD A,d8
        run(&mut c, &mut m, 2);
        assert_eq!(c.regs.a, 0x42);
    }

    #[test]
    fn ld_a_hl_2cycle() {
        let (mut c, mut m) = setup(&[0x7E]); // LD A,(HL)
        c.regs.write_hl(0xC050);
        m.write(0xC050, 0x99);
        run(&mut c, &mut m, 2);
        assert_eq!(c.regs.a, 0x99);
    }

    #[test]
    fn ld_hl_d8_3cycle() {
        let (mut c, mut m) = setup(&[0x36, 0x77]); // LD (HL),d8
        c.regs.write_hl(0xC050);
        run(&mut c, &mut m, 3);
        assert_eq!(m.read(0xC050), 0x77);
    }

    // ── LD 16bit ────────────────────────────
    #[test]
    fn ld_bc_d16_3cycle() {
        let (mut c, mut m) = setup(&[0x01, 0x34, 0x12]); // LD BC,d16
        run(&mut c, &mut m, 3);
        assert_eq!(c.regs.bc(), 0x1234);
    }

    #[test]
    fn ld_a16_sp_5cycle() {
        let (mut c, mut m) = setup(&[0x08, 0x50, 0xC0]); // LD (a16),SP
        c.regs.sp = 0x1234;
        run(&mut c, &mut m, 5);
        assert_eq!(m.read(0xC050), 0x34);
        assert_eq!(m.read(0xC051), 0x12);
    }

    // ── スタック ────────────────────────────
    #[test]
    fn push_bc_4cycle() {
        let (mut c, mut m) = setup(&[0xC5]); // PUSH BC
        c.regs.write_bc(0x1234);
        run(&mut c, &mut m, 4);
        assert_eq!(c.regs.sp, STACK - 2);
        assert_eq!(m.read(STACK - 1), 0x12);
        assert_eq!(m.read(STACK - 2), 0x34);
    }

    #[test]
    fn pop_bc_3cycle() {
        let (mut c, mut m) = setup(&[0xC1]); // POP BC
        c.regs.sp = STACK - 2;
        m.write(STACK - 2, 0x34);
        m.write(STACK - 1, 0x12);
        run(&mut c, &mut m, 3);
        assert_eq!(c.regs.bc(), 0x1234);
        assert_eq!(c.regs.sp, STACK);
    }

    // ── 分岐（オーバーラップfetchで完了時 PC = 宛先+1）───
    #[test]
    fn call_6cycle() {
        let (mut c, mut m) = setup(&[0xCD, 0x00, 0xD0]); // CALL 0xD000
        run(&mut c, &mut m, 6);
        assert_eq!(c.regs.sp, STACK - 2);
        // 戻りアドレス = CALL の次命令 0xC003
        assert_eq!(m.read(STACK - 1), 0xC0);
        assert_eq!(m.read(STACK - 2), 0x03);
        assert_eq!(c.regs.pc, 0xD001); // 0xD000 を fetch 済み
    }

    #[test]
    fn ret_4cycle() {
        let (mut c, mut m) = setup(&[0xC9]); // RET
        c.regs.sp = STACK - 2;
        m.write(STACK - 2, 0x00);
        m.write(STACK - 1, 0xD0); // 戻り先 0xD000
        run(&mut c, &mut m, 4);
        assert_eq!(c.regs.sp, STACK);
        assert_eq!(c.regs.pc, 0xD001);
    }

    #[test]
    fn jr_3cycle() {
        let (mut c, mut m) = setup(&[0x18, 0x05]); // JR +5
        run(&mut c, &mut m, 3);
        // オペランド後 0xC002 + 5 = 0xC007、fetch で +1
        assert_eq!(c.regs.pc, 0xC008);
    }

    #[test]
    fn jr_nz_taken_3cycle() {
        let (mut c, mut m) = setup(&[0x20, 0x05]); // JR NZ,+5
        c.regs.set_zf(false); // NZ 成立
        run(&mut c, &mut m, 3);
        assert_eq!(c.regs.pc, 0xC008);
    }

    #[test]
    fn jr_nz_not_taken_2cycle() {
        let (mut c, mut m) = setup(&[0x20, 0x05]); // JR NZ,+5
        c.regs.set_zf(true); // NZ 不成立
        run(&mut c, &mut m, 2);
        assert_eq!(c.regs.pc, 0xC003); // 分岐せず次命令を fetch
    }

    // ── ALU / フラグ ────────────────────────
    #[test]
    fn add_a_hl_2cycle() {
        let (mut c, mut m) = setup(&[0x86]); // ADD A,(HL)
        c.regs.a = 0x01;
        c.regs.write_hl(0xC050);
        m.write(0xC050, 0x02);
        run(&mut c, &mut m, 2);
        assert_eq!(c.regs.a, 0x03);
    }

    #[test]
    fn inc_hl_mem_3cycle() {
        let (mut c, mut m) = setup(&[0x34]); // INC (HL)
        c.regs.write_hl(0xC050);
        m.write(0xC050, 0x0F);
        run(&mut c, &mut m, 3);
        assert_eq!(m.read(0xC050), 0x10);
        assert!(c.regs.hf()); // ハーフキャリー
        assert!(!c.regs.zf());
    }

    // ── CB ──────────────────────────────────
    #[test]
    fn cb_swap_b_2cycle() {
        let (mut c, mut m) = setup(&[0xCB, 0x30]); // SWAP B
        c.regs.b = 0x12;
        run(&mut c, &mut m, 2);
        assert_eq!(c.regs.b, 0x21);
    }

    #[test]
    fn cb_bit0_b_2cycle() {
        let (mut c, mut m) = setup(&[0xCB, 0x40]); // BIT 0,B
        c.regs.b = 0x01;
        run(&mut c, &mut m, 2);
        assert!(!c.regs.zf()); // bit0=1 → Z=0
    }

    #[test]
    fn cb_res0_hl_4cycle() {
        let (mut c, mut m) = setup(&[0xCB, 0x86]); // RES 0,(HL)
        c.regs.write_hl(0xC050);
        m.write(0xC050, 0xFF);
        run(&mut c, &mut m, 4);
        assert_eq!(m.read(0xC050), 0xFE);
    }

    #[test]
    fn cb_swap_hl_4cycle() {
        let (mut c, mut m) = setup(&[0xCB, 0x36]); // SWAP (HL)
        c.regs.write_hl(0xC050);
        m.write(0xC050, 0x12);
        run(&mut c, &mut m, 4);
        assert_eq!(m.read(0xC050), 0x21);
    }

    // ── 割り込み / 制御 ─────────────────────
    #[test]
    fn interrupt_dispatch() {
        // VBlank 割り込みでベクタ 0x0040 へ。SP に戻りアドレスを積む。
        // 注: 現行実装は割り込みディスパッチを 1 emulate_cycle で完了する（実機は5 M-cycle）。
        //     ベースラインは現行挙動を固定する。正確な5サイクル化は別タスク。
        let (mut c, mut m) = setup(&[0x00]);
        c.ime = true;
        m.ie = 0x01; // VBlank 許可
        m.if_ = 0x01; // VBlank 要求
        // setup 直後が命令境界のため、最初の emulate_cycle で即割り込みディスパッチ。
        // ベクタ先(bootrom)の命令を実行しないよう 1 サイクルのみ回す。
        run(&mut c, &mut m, 1); // 割り込みディスパッチ（現行は1サイクル）
        assert_eq!(c.regs.pc, 0x0041); // 0x0040 を fetch 済み
        assert!(!c.ime); // 割り込みで IME クリア
        assert_eq!(m.if_ & 0x01, 0); // 要求クリア
        assert_eq!(c.regs.sp, STACK - 2);
    }

    #[test]
    fn ei_delayed_one_instruction() {
        // EI の次の1命令を実行してから割り込みが有効になる
        let (mut c, mut m) = setup(&[0xFB, 0x00]); // EI; NOP
        m.ie = 0x01;
        m.if_ = 0x01;
        run(&mut c, &mut m, 1); // EI 完了（この時点では IME まだ無効）
        assert!(!c.ime);
        run(&mut c, &mut m, 1); // NOP 完了。境界で IME 有効化されるが直後割り込みは次境界
        assert!(c.ime);
    }
}
