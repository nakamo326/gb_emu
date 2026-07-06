//! ARM7TDMI と周辺（DMA・タイマー・PPU・HLE BIOS）の動作テスト。
//! 命令は手アセンブルしたオペコードを IWRAM に置いて実行する。

use gba_core::cpu::{FLAG_C, FLAG_N, FLAG_T, FLAG_V, FLAG_Z, MODE_IRQ, MODE_SYS};
use gba_core::gba::Gba;

const IWRAM: u32 = 0x0300_0000;

/// ARM 命令列を IWRAM に置いた状態の Gba を作る（PC は先頭を指す）
fn setup(instrs: &[u32]) -> Gba {
    let mut gba = Gba::new(vec![], None);
    for (i, &op) in instrs.iter().enumerate() {
        gba.bus.write32(IWRAM + i as u32 * 4, op);
    }
    gba.cpu.regs[15] = IWRAM;
    gba
}

fn step(gba: &mut Gba, n: usize) {
    for _ in 0..n {
        gba.cpu.step(&mut gba.bus);
    }
}

#[test]
fn arm_mov_imm_flags() {
    // MOVS r0, #0 / MOV r1, #0xFF000000 (0xFF ror 8)
    let mut gba = setup(&[0xE3B0_0000, 0xE3A0_14FF]);
    step(&mut gba, 2);
    assert_eq!(gba.cpu.regs[0], 0);
    assert_ne!(gba.cpu.cpsr & FLAG_Z, 0);
    assert_eq!(gba.cpu.regs[1], 0xFF00_0000);
}

#[test]
fn arm_add_sub_flags() {
    // ADDS r1, r0, r0 (r0=0xFF000000): C=1, N=1, V=0
    let mut gba = setup(&[0xE090_1000, 0xE150_0000]); // ADDS / CMP r0, r0
    gba.cpu.regs[0] = 0xFF00_0000;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[1], 0xFE00_0000);
    assert_ne!(gba.cpu.cpsr & FLAG_C, 0);
    assert_ne!(gba.cpu.cpsr & FLAG_N, 0);
    assert_eq!(gba.cpu.cpsr & FLAG_V, 0);
    step(&mut gba, 1); // CMP r0, r0: Z=1, C=1 (ボローなし)
    assert_ne!(gba.cpu.cpsr & FLAG_Z, 0);
    assert_ne!(gba.cpu.cpsr & FLAG_C, 0);
}

#[test]
fn arm_overflow_flag() {
    // ADDS r2, r0, r1 (0x7FFFFFFF + 1): V=1
    let mut gba = setup(&[0xE090_2001]);
    gba.cpu.regs[0] = 0x7FFF_FFFF;
    gba.cpu.regs[1] = 1;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[2], 0x8000_0000);
    assert_ne!(gba.cpu.cpsr & FLAG_V, 0);
    assert_eq!(gba.cpu.cpsr & FLAG_C, 0);
}

#[test]
fn arm_shifter_lsr32() {
    // MOVS r1, r0, LSR #0 は LSR #32: 結果 0、C = bit31
    let mut gba = setup(&[0xE1B0_1020]);
    gba.cpu.regs[0] = 0x8000_0000;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[1], 0);
    assert_ne!(gba.cpu.cpsr & FLAG_Z, 0);
    assert_ne!(gba.cpu.cpsr & FLAG_C, 0);
}

#[test]
fn arm_ldr_unaligned_rotate() {
    // LDR r0, [r1] (r1 = +1 の非アライン): ワードが右ローテートされて読める
    let mut gba = setup(&[0xE591_0000]);
    gba.bus.write32(IWRAM + 0x100, 0x1122_3344);
    gba.cpu.regs[1] = IWRAM + 0x101;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[0], 0x4411_2233);
}

#[test]
fn arm_str_ldrb() {
    // STR r0, [r1, #4]! / LDRB r2, [r1], #1
    let mut gba = setup(&[0xE5A1_0004, 0xE4D1_2001]);
    gba.cpu.regs[0] = 0xAABB_CCDD;
    gba.cpu.regs[1] = IWRAM + 0x200;
    step(&mut gba, 2);
    assert_eq!(gba.bus.read32(IWRAM + 0x204), 0xAABB_CCDD);
    assert_eq!(gba.cpu.regs[2], 0xDD);
    assert_eq!(gba.cpu.regs[1], IWRAM + 0x205); // ポストインクリメント
}

#[test]
fn arm_ldrh_ldrsb() {
    // LDRH r0, [r1] / LDRSB r2, [r1]
    let mut gba = setup(&[0xE1D1_00B0, 0xE1D1_20D0]);
    gba.bus.write16(IWRAM + 0x300, 0x80FE);
    gba.cpu.regs[1] = IWRAM + 0x300;
    step(&mut gba, 2);
    assert_eq!(gba.cpu.regs[0], 0x80FE);
    assert_eq!(gba.cpu.regs[2], 0xFFFF_FFFE); // 0xFE の符号拡張
}

#[test]
fn arm_stm_ldm() {
    // STMIA r4!, {r0-r3} → LDMDB r4!, {r5-r8}
    let mut gba = setup(&[0xE8A4_000F, 0xE934_01E0]);
    for r in 0..4 {
        gba.cpu.regs[r] = 0x10 + r as u32;
    }
    gba.cpu.regs[4] = IWRAM + 0x400;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[4], IWRAM + 0x410);
    assert_eq!(gba.bus.read32(IWRAM + 0x400), 0x10);
    assert_eq!(gba.bus.read32(IWRAM + 0x40C), 0x13);
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[4], IWRAM + 0x400); // ライトバック
    assert_eq!(gba.cpu.regs[5], 0x10);
    assert_eq!(gba.cpu.regs[8], 0x13);
}

#[test]
fn arm_branch_link() {
    // BL +4 (0x03000000 → 0x0300000C)
    let mut gba = setup(&[0xEB00_0001]);
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[15], IWRAM + 0xC);
    assert_eq!(gba.cpu.regs[14], IWRAM + 4);
}

#[test]
fn arm_mul_umull() {
    // MUL r0, r1, r2 / UMULL r3, r4, r1, r2
    let mut gba = setup(&[0xE000_0291, 0xE084_3291]);
    gba.cpu.regs[1] = 0xFFFF_FFFF;
    gba.cpu.regs[2] = 2;
    step(&mut gba, 2);
    assert_eq!(gba.cpu.regs[0], 0xFFFF_FFFE);
    assert_eq!(gba.cpu.regs[3], 0xFFFF_FFFE); // 下位
    assert_eq!(gba.cpu.regs[4], 1); // 上位
}

#[test]
fn arm_msr_mode_switch() {
    // MSR cpsr_c, #0x12: IRQ モードへ切り替え → r13 が IRQ バンクに変わる
    let mut gba = setup(&[0xE321_F012]);
    let sys_sp = gba.cpu.regs[13];
    step(&mut gba, 1);
    assert_eq!(gba.cpu.cpsr & 0x1F, MODE_IRQ);
    assert_eq!(gba.cpu.regs[13], 0x0300_7FA0); // HLE BIOS 初期化の sp_irq
    assert_ne!(gba.cpu.regs[13], sys_sp);
}

#[test]
fn bx_to_thumb_and_back() {
    // BX r1 (bit0=1) で Thumb へ。Thumb 側: MOV r0, #42 / BX r2 で ARM へ復帰
    let mut gba = setup(&[0xE12F_FF11]);
    gba.bus.write16(IWRAM + 0x100, 0x202A); // MOV r0, #42
    gba.bus.write16(IWRAM + 0x102, 0x4710); // BX r2
    gba.cpu.regs[1] = IWRAM + 0x101;
    gba.cpu.regs[2] = IWRAM + 0x10;
    step(&mut gba, 1);
    assert_ne!(gba.cpu.cpsr & FLAG_T, 0);
    assert_eq!(gba.cpu.regs[15], IWRAM + 0x100);
    step(&mut gba, 2);
    assert_eq!(gba.cpu.regs[0], 42);
    assert_eq!(gba.cpu.cpsr & FLAG_T, 0);
    assert_eq!(gba.cpu.regs[15], IWRAM + 0x10);
}

#[test]
fn thumb_push_pop() {
    let mut gba = setup(&[]);
    gba.bus.write16(IWRAM, 0xB501); // PUSH {r0, lr}
    gba.bus.write16(IWRAM + 2, 0xBD02); // POP {r1, pc}
    gba.cpu.regs[15] = IWRAM;
    gba.cpu.cpsr |= FLAG_T;
    gba.cpu.regs[0] = 0x1234;
    gba.cpu.regs[14] = IWRAM + 0x21; // Thumb の戻り先
    let sp = gba.cpu.regs[13];
    step(&mut gba, 2);
    assert_eq!(gba.cpu.regs[1], 0x1234);
    assert_eq!(gba.cpu.regs[15], IWRAM + 0x20);
    assert_eq!(gba.cpu.regs[13], sp);
}

#[test]
fn thumb_bl_pair() {
    let mut gba = setup(&[]);
    // BL +0x10: 前半 0xF000, 後半 0xF806 (offset 0x10 → 前半0, 後半 (0x10-4)/2 +? )
    // target = (pc+4) + 0 + (imm11 << 1)。pc=IWRAM: target = IWRAM+4+0xC = IWRAM+0x10
    gba.bus.write16(IWRAM, 0xF000);
    gba.bus.write16(IWRAM + 2, 0xF806);
    gba.cpu.regs[15] = IWRAM;
    gba.cpu.cpsr |= FLAG_T;
    step(&mut gba, 2);
    assert_eq!(gba.cpu.regs[15], IWRAM + 0x10);
    assert_eq!(gba.cpu.regs[14], (IWRAM + 4) | 1);
}

#[test]
fn hle_swi_div() {
    // SWI 0x06: r0/r1
    let mut gba = setup(&[0xEF06_0000]);
    gba.cpu.regs[0] = 0xFFFF_FFF9; // -7
    gba.cpu.regs[1] = 2;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[0] as i32, -3);
    assert_eq!(gba.cpu.regs[1] as i32, -1);
    assert_eq!(gba.cpu.regs[3], 3);
}

#[test]
fn hle_swi_sqrt_cpuset() {
    let mut gba = setup(&[0xEF08_0000, 0xEF0B_0000]);
    gba.cpu.regs[0] = 1000000;
    step(&mut gba, 1);
    assert_eq!(gba.cpu.regs[0], 1000);
    // CpuSet: fill モードで 4 ワード
    gba.bus.write32(IWRAM + 0x500, 0xDEAD_BEEF);
    gba.cpu.regs[0] = IWRAM + 0x500;
    gba.cpu.regs[1] = IWRAM + 0x600;
    gba.cpu.regs[2] = 4 | 1 << 24 | 1 << 26;
    step(&mut gba, 1);
    for i in 0..4 {
        assert_eq!(gba.bus.read32(IWRAM + 0x600 + i * 4), 0xDEAD_BEEF);
    }
}

#[test]
fn hle_irq_dispatch_and_return() {
    // IRQ 発生 → HLE で [0x03007FFC] のハンドラへ、復帰マジックで元の位置へ戻る
    let mut gba = setup(&[0xE1A0_0000, 0xE1A0_0000]); // NOP; NOP
    gba.bus.write32(0x0300_7FFC, IWRAM + 0x100); // ユーザーハンドラ
    gba.bus.write16(0x0400_0200, 0x0001); // IE: VBlank
    gba.bus.write16(0x0400_0208, 1); // IME
    gba.bus.if_ |= 1; // VBlank IRQ 要求
    let step_result = gba.step(); // IRQ ディスパッチ + ハンドラ 1 命令目
    assert!(!step_result);
    assert_eq!(gba.cpu.cpsr & 0x1F, MODE_IRQ);
    // ハンドラ先頭 (IWRAM+0x100) の命令が実行された後の PC を確認
    assert_eq!(gba.cpu.regs[15], IWRAM + 0x104);
    // ハンドラが IF を ack して復帰する状況を再現
    gba.bus.write16(0x0400_0202, 0x0001);
    gba.cpu.regs[15] = gba.cpu.regs[14]; // BX lr 相当（マジックアドレスへ）
    gba.step();
    assert_eq!(gba.cpu.cpsr & 0x1F, MODE_SYS);
    assert_eq!(gba.cpu.regs[15], IWRAM); // 中断された NOP へ復帰
}

#[test]
fn keypad_input() {
    let mut gba = setup(&[]);
    gba.set_keys(0x0001); // A 押下
    assert_eq!(gba.bus.read16(0x0400_0130), 0x3FE);
}

#[test]
fn dma_immediate_transfer() {
    let mut gba = setup(&[0xE1A0_0000]); // NOP
    for i in 0..4u32 {
        gba.bus.write32(IWRAM + 0x700 + i * 4, 0x1111_0000 + i);
    }
    // DMA3: src=IWRAM+0x700, dst=IWRAM+0x800, 4 ワード, 32bit, 即時
    gba.bus.write32(0x0400_00D4, IWRAM + 0x700);
    gba.bus.write32(0x0400_00D8, IWRAM + 0x800);
    gba.bus.write16(0x0400_00DC, 4);
    gba.bus.write16(0x0400_00DE, 0x8400);
    gba.step();
    for i in 0..4u32 {
        assert_eq!(gba.bus.read32(IWRAM + 0x800 + i * 4), 0x1111_0000 + i);
    }
}

#[test]
fn timer_overflow_irq() {
    let mut gba = setup(&[0xE1A0_0000; 64]);
    // タイマー0: リロード 0xFFF0、プリスケーラ 1、IRQ 有効
    gba.bus.write16(0x0400_0100, 0xFFF0);
    gba.bus.write16(0x0400_0102, 0x00C0);
    // 0x10 = 16 サイクルでオーバーフロー
    for _ in 0..16 {
        gba.step();
    }
    assert_ne!(gba.bus.if_ & 0x08, 0);
}

#[test]
fn ppu_mode3_renders_pixel() {
    let mut gba = setup(&[0xEAFF_FFFE]); // B self (自己ループ)
    // モード 3 + BG2
    gba.bus.write16(0x0400_0000, 0x0403);
    // (x=10, y=5) に赤
    gba.bus.write16(0x0600_0000 + (5 * 240 + 10) * 2, 0x001F);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[5 * 240 + 10], 0x001F);
    assert_eq!(gba.framebuffer()[0], 0);
}

#[test]
fn ppu_vblank_irq_and_intr_wait() {
    // VBlankIntrWait がハンドラのフラグ書き込みで解除されるか
    let mut gba = setup(&[]);
    // メイン: SWI 5 (VBlankIntrWait) → 次命令で r5 = 1
    gba.bus.write32(IWRAM, 0xEF05_0000);
    gba.bus.write32(IWRAM + 4, 0xE3A0_5001); // MOV r5, #1
    gba.bus.write32(IWRAM + 8, 0xEAFF_FFFE); // B self
    // ハンドラ: BIOS フラグに VBlank を立て、IF を ack して復帰
    let handler = IWRAM + 0x100;
    gba.bus.write32(0x0300_7FFC, handler);
    // MOV r1, #0x04000000 / ADD r1, r1, #0x200 / MOV r2, #1 / STRH r2, [r1, #2]
    gba.bus.write32(handler, 0xE3A0_1301);
    gba.bus.write32(handler + 4, 0xE281_1C02);
    gba.bus.write32(handler + 8, 0xE3A0_2001);
    gba.bus.write32(handler + 12, 0xE1C1_20B2);
    // MOV r3, #0x03000000 / ORR r3, r3, #0x7F00 / ORR r3, r3, #0xF8 / STRH r2, [r3]
    gba.bus.write32(handler + 16, 0xE3A0_3403);
    gba.bus.write32(handler + 20, 0xE383_3C7F);
    gba.bus.write32(handler + 24, 0xE383_30F8);
    gba.bus.write32(handler + 28, 0xE1C3_20B0);
    gba.bus.write32(handler + 32, 0xE12F_FF1E); // BX lr
    // IE: VBlank, IME on, DISPSTAT: VBlank IRQ 有効
    gba.bus.write16(0x0400_0200, 0x0001);
    gba.bus.write16(0x0400_0208, 1);
    gba.bus.write16(0x0400_0004, 0x0008);

    gba.run_frame();
    // フレーム末尾の VBlank でハンドラが走り、IntrWait が解除されて r5 = 1
    for _ in 0..100 {
        gba.step();
    }
    assert_eq!(gba.cpu.regs[5], 1);
}
