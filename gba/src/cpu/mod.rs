//! ARM7TDMI。ARM / Thumb 両命令セット、モード別バンクレジスタ、例外を実装する。
//!
//! パイプラインは実体を持たず、「実行中命令のアドレス + 8（Thumb は +4）を
//! r15 が指す」規約で近似する。サイクル数は命令クラス単位の概算
//! （S/N サイクルやウェイトステートは区別しない）。

mod arm;
pub(crate) mod swi;
mod thumb;

use crate::bus::Bus;

pub const MODE_USR: u32 = 0x10;
pub const MODE_FIQ: u32 = 0x11;
pub const MODE_IRQ: u32 = 0x12;
pub const MODE_SVC: u32 = 0x13;
pub const MODE_ABT: u32 = 0x17;
pub const MODE_UND: u32 = 0x1B;
pub const MODE_SYS: u32 = 0x1F;

pub const FLAG_N: u32 = 1 << 31;
pub const FLAG_Z: u32 = 1 << 30;
pub const FLAG_C: u32 = 1 << 29;
pub const FLAG_V: u32 = 1 << 28;
pub const FLAG_I: u32 = 1 << 7;
pub const FLAG_T: u32 = 1 << 5;

/// HLE BIOS の IRQ ハンドラ復帰先。BIOS 領域内の実在しないアドレスを
/// マジック値として使い、フェッチされたら復帰処理を行う
const HLE_IRQ_RETURN: u32 = 0x0000_0138;

pub struct Cpu {
    pub regs: [u32; 16],
    pub cpsr: u32,
    /// r8-r12 の退避（FIQ 以外のモード用と FIQ 用）
    bank_r8_12: [u32; 5],
    bank_r8_12_fiq: [u32; 5],
    /// r13-r14 のモード別バンク。添字は [`bank_index`]
    bank_r13_14: [[u32; 2]; 6],
    spsr: [u32; 6],
    pub halted: bool,
    /// HLE IntrWait 待機中の割り込みマスク（BIOS フラグ 0x03007FF8 と照合）
    pub intr_wait_mask: Option<u16>,
    /// 実 BIOS 非搭載時に SWI と IRQ ディスパッチを HLE する
    pub hle_bios: bool,
    /// この命令で r15 が書き換えられた（次命令アドレスの自動加算を抑止）
    branched: bool,
    /// ロード/ストア等が加算する追加サイクル
    extra_cycles: u32,
}

/// モード → バンク添字 (0:usr/sys 1:fiq 2:irq 3:svc 4:abt 5:und)
fn bank_index(mode: u32) -> usize {
    match mode {
        MODE_FIQ => 1,
        MODE_IRQ => 2,
        MODE_SVC => 3,
        MODE_ABT => 4,
        MODE_UND => 5,
        _ => 0,
    }
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            regs: [0; 16],
            cpsr: MODE_SVC | FLAG_I | FLAG_F_INIT,
            bank_r8_12: [0; 5],
            bank_r8_12_fiq: [0; 5],
            bank_r13_14: [[0; 2]; 6],
            spsr: [0; 6],
            halted: false,
            intr_wait_mask: None,
            hle_bios: false,
            branched: false,
            extra_cycles: 0,
        }
    }

    /// BIOS 起動完了直後の状態を再現する（HLE BIOS 用）。
    pub fn apply_post_bios_state(&mut self) {
        self.bank_r13_14[bank_index(MODE_IRQ)][0] = 0x0300_7FA0;
        self.bank_r13_14[bank_index(MODE_SVC)][0] = 0x0300_7FE0;
        self.regs[13] = 0x0300_7F00;
        self.cpsr = MODE_SYS;
        self.regs[15] = 0x0800_0000;
    }

    pub fn thumb(&self) -> bool {
        self.cpsr & FLAG_T != 0
    }

    fn mode(&self) -> u32 {
        self.cpsr & 0x1F
    }

    /// モード変更に伴い r8-r14 のバンクを入れ替える。cpsr のモードビットも更新する。
    pub fn set_mode(&mut self, new_mode: u32) {
        let old = self.mode();
        if old != new_mode {
            let (oi, ni) = (bank_index(old), bank_index(new_mode));
            if oi != ni {
                self.bank_r13_14[oi] = [self.regs[13], self.regs[14]];
                self.regs[13] = self.bank_r13_14[ni][0];
                self.regs[14] = self.bank_r13_14[ni][1];
                // r8-r12 は FIQ だけが独自バンクを持つ
                if old == MODE_FIQ {
                    self.bank_r8_12_fiq.copy_from_slice(&self.regs[8..13]);
                    self.regs[8..13].copy_from_slice(&self.bank_r8_12);
                } else if new_mode == MODE_FIQ {
                    self.bank_r8_12.copy_from_slice(&self.regs[8..13]);
                    self.regs[8..13].copy_from_slice(&self.bank_r8_12_fiq);
                }
            }
        }
        self.cpsr = self.cpsr & !0x1F | new_mode;
    }

    /// LDM/STM の S ビット用: ユーザーバンクのレジスタへアクセスする。
    fn user_reg(&self, r: usize) -> u32 {
        let mode = self.mode();
        if r >= 13 && !matches!(mode, MODE_USR | MODE_SYS) {
            self.bank_r13_14[0][r - 13]
        } else if (8..13).contains(&r) && mode == MODE_FIQ {
            self.bank_r8_12[r - 8]
        } else {
            self.regs[r]
        }
    }

    fn set_user_reg(&mut self, r: usize, val: u32) {
        let mode = self.mode();
        if r >= 13 && !matches!(mode, MODE_USR | MODE_SYS) {
            self.bank_r13_14[0][r - 13] = val;
        } else if (8..13).contains(&r) && mode == MODE_FIQ {
            self.bank_r8_12[r - 8] = val;
        } else {
            self.regs[r] = val;
        }
    }

    fn spsr(&self) -> u32 {
        let i = bank_index(self.mode());
        if i == 0 { self.cpsr } else { self.spsr[i] }
    }

    fn set_spsr(&mut self, val: u32) {
        let i = bank_index(self.mode());
        if i != 0 {
            self.spsr[i] = val;
        }
    }

    /// SPSR を CPSR へ復帰する（例外からの復帰）。モードバンクも切り替わる。
    fn restore_cpsr(&mut self) {
        let spsr = self.spsr();
        self.set_mode(spsr & 0x1F);
        self.cpsr = spsr;
    }

    fn exception(&mut self, mode: u32, vector: u32, lr: u32) {
        let old_cpsr = self.cpsr;
        self.set_mode(mode);
        self.set_spsr(old_cpsr);
        self.regs[14] = lr;
        self.cpsr = self.cpsr & !FLAG_T | FLAG_I;
        self.regs[15] = vector;
        self.branched = true;
    }

    /// IRQ 例外へ遷移する。呼び出し時点で regs[15] は次に実行する命令を指すこと。
    /// I フラグが立っていれば何もしない。
    pub fn irq(&mut self, bus: &mut Bus) {
        if self.cpsr & FLAG_I != 0 {
            return;
        }
        // ハンドラは SUBS PC, LR, #4 で復帰するため LR = 次命令 + 4
        let lr = self.regs[15].wrapping_add(4);
        self.exception(MODE_IRQ, 0x18, lr);
        if self.hle_bios {
            self.hle_irq_dispatch(bus);
        }
    }

    /// BIOS の IRQ スタブ相当: r0-r3,r12,lr を退避し [0x03007FFC] のハンドラへ。
    /// 復帰先には HLE_IRQ_RETURN を渡し、フェッチ時に復帰処理を行う。
    fn hle_irq_dispatch(&mut self, bus: &mut Bus) {
        let mut sp = self.regs[13];
        for r in [14usize, 12, 3, 2, 1, 0] {
            sp = sp.wrapping_sub(4);
            bus.write32(sp, self.regs[r]);
        }
        self.regs[13] = sp;
        self.regs[0] = 0x0400_0000;
        self.regs[14] = HLE_IRQ_RETURN;
        self.regs[15] = bus.read32(0x0300_7FFC) & !3;
    }

    /// HLE IRQ ハンドラからの復帰（レジスタ復元 + SUBS PC, LR, #4 相当）。
    fn hle_irq_return(&mut self, bus: &mut Bus) {
        let mut sp = self.regs[13];
        for r in [0usize, 1, 2, 3, 12, 14] {
            self.regs[r] = bus.read32(sp);
            sp = sp.wrapping_add(4);
        }
        self.regs[13] = sp;
        self.regs[15] = self.regs[14].wrapping_sub(4);
        self.restore_cpsr();
        // IntrWait 待機が未解決のまま戻ってきたら再度 halt する
        // （待機対象でない割り込みで起こされたケース）
        if self.intr_wait_mask.is_some() {
            self.halted = true;
        }
    }

    /// 1 命令実行し、消費サイクル（概算）を返す。
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        if self.hle_bios && self.regs[15] == HLE_IRQ_RETURN {
            self.hle_irq_return(bus);
            return 4;
        }
        self.branched = false;
        self.extra_cycles = 0;
        if self.thumb() {
            let addr = self.regs[15] & !1;
            let op = bus.read16(addr);
            self.regs[15] = addr.wrapping_add(4);
            self.exec_thumb(bus, op);
            if !self.branched {
                self.regs[15] = addr.wrapping_add(2);
            } else if !self.thumb() {
                self.regs[15] &= !3;
            } else {
                self.regs[15] &= !1;
            }
        } else {
            let addr = self.regs[15] & !3;
            let op = bus.read32(addr);
            self.regs[15] = addr.wrapping_add(8);
            if self.check_cond(op >> 28) {
                self.exec_arm(bus, op);
            }
            if !self.branched {
                self.regs[15] = addr.wrapping_add(4);
            } else if self.thumb() {
                self.regs[15] &= !1;
            } else {
                self.regs[15] &= !3;
            }
        }
        // 基本 3 サイクル + メモリアクセス分。実機の S/N/ウェイトの平均を狙った概算
        3 + self.extra_cycles
    }

    pub(crate) fn check_cond(&self, cond: u32) -> bool {
        let n = self.cpsr & FLAG_N != 0;
        let z = self.cpsr & FLAG_Z != 0;
        let c = self.cpsr & FLAG_C != 0;
        let v = self.cpsr & FLAG_V != 0;
        match cond & 0xF {
            0x0 => z,
            0x1 => !z,
            0x2 => c,
            0x3 => !c,
            0x4 => n,
            0x5 => !n,
            0x6 => v,
            0x7 => !v,
            0x8 => c && !z,
            0x9 => !c || z,
            0xA => n == v,
            0xB => n != v,
            0xC => !z && n == v,
            0xD => z || n != v,
            0xE => true,
            _ => false,
        }
    }

    // ---- フラグ計算ヘルパー（ARM / Thumb 共用） ----

    fn set_nz(&mut self, v: u32) {
        self.cpsr = self.cpsr & !(FLAG_N | FLAG_Z)
            | if v == 0 { FLAG_Z } else { 0 }
            | v & FLAG_N;
    }

    fn set_c(&mut self, c: bool) {
        self.cpsr = self.cpsr & !FLAG_C | if c { FLAG_C } else { 0 };
    }

    fn add_flags(&mut self, a: u32, b: u32, set: bool) -> u32 {
        let (r, c) = a.overflowing_add(b);
        if set {
            self.set_nz(r);
            self.set_c(c);
            let v = (!(a ^ b) & (a ^ r)) & 0x8000_0000 != 0;
            self.cpsr = self.cpsr & !FLAG_V | if v { FLAG_V } else { 0 };
        }
        r
    }

    fn sub_flags(&mut self, a: u32, b: u32, set: bool) -> u32 {
        let r = a.wrapping_sub(b);
        if set {
            self.set_nz(r);
            self.set_c(a >= b); // C = ボローなし
            let v = ((a ^ b) & (a ^ r)) & 0x8000_0000 != 0;
            self.cpsr = self.cpsr & !FLAG_V | if v { FLAG_V } else { 0 };
        }
        r
    }

    fn adc_flags(&mut self, a: u32, b: u32, set: bool) -> u32 {
        let carry = (self.cpsr & FLAG_C != 0) as u32;
        let r = a.wrapping_add(b).wrapping_add(carry);
        if set {
            self.set_nz(r);
            let c = (a as u64 + b as u64 + carry as u64) > 0xFFFF_FFFF;
            self.set_c(c);
            let v = (!(a ^ b) & (a ^ r)) & 0x8000_0000 != 0;
            self.cpsr = self.cpsr & !FLAG_V | if v { FLAG_V } else { 0 };
        }
        r
    }

    fn sbc_flags(&mut self, a: u32, b: u32, set: bool) -> u32 {
        let borrow = (self.cpsr & FLAG_C == 0) as u32;
        let r = a.wrapping_sub(b).wrapping_sub(borrow);
        if set {
            self.set_nz(r);
            self.set_c(a as u64 >= b as u64 + borrow as u64);
            let v = ((a ^ b) & (a ^ r)) & 0x8000_0000 != 0;
            self.cpsr = self.cpsr & !FLAG_V | if v { FLAG_V } else { 0 };
        }
        r
    }
}

/// リセット時は FIQ 無効。GBA では FIQ は使われないため以後固定
const FLAG_F_INIT: u32 = 1 << 6;

// ---- バレルシフタ（ARM オペランド 2 / Thumb シフト命令共用） ----

/// 即値シフト。amount==0 は各タイプ特殊（LSR/ASR は 32、ROR は RRX）
pub(crate) fn shift_by_immediate(val: u32, ty: u32, amount: u32, carry: &mut bool) -> u32 {
    match (ty, amount) {
        (0, 0) => val,
        (0, n) => {
            *carry = val & (1 << (32 - n)) != 0;
            val << n
        }
        (1, 0) => {
            *carry = val & 0x8000_0000 != 0;
            0
        }
        (1, n) => {
            *carry = val & (1 << (n - 1)) != 0;
            val >> n
        }
        (2, 0) => {
            *carry = val & 0x8000_0000 != 0;
            ((val as i32) >> 31) as u32
        }
        (2, n) => {
            *carry = val & (1 << (n - 1)) != 0;
            ((val as i32) >> n) as u32
        }
        (_, 0) => {
            // RRX
            let old_c = *carry as u32;
            *carry = val & 1 != 0;
            val >> 1 | old_c << 31
        }
        (_, n) => {
            *carry = val & (1 << (n - 1)) != 0;
            val.rotate_right(n)
        }
    }
}

/// レジスタ量シフト。amount==0 は値・キャリーとも不変、32 以上も定義どおり
pub(crate) fn shift_by_register(val: u32, ty: u32, amount: u32, carry: &mut bool) -> u32 {
    if amount == 0 {
        return val;
    }
    match ty {
        0 => match amount {
            1..=31 => {
                *carry = val & (1 << (32 - amount)) != 0;
                val << amount
            }
            32 => {
                *carry = val & 1 != 0;
                0
            }
            _ => {
                *carry = false;
                0
            }
        },
        1 => match amount {
            1..=31 => {
                *carry = val & (1 << (amount - 1)) != 0;
                val >> amount
            }
            32 => {
                *carry = val & 0x8000_0000 != 0;
                0
            }
            _ => {
                *carry = false;
                0
            }
        },
        2 => {
            if amount >= 32 {
                *carry = val & 0x8000_0000 != 0;
                ((val as i32) >> 31) as u32
            } else {
                *carry = val & (1 << (amount - 1)) != 0;
                ((val as i32) >> amount) as u32
            }
        }
        _ => {
            let n = amount & 31;
            if n == 0 {
                *carry = val & 0x8000_0000 != 0;
                val
            } else {
                *carry = val & (1 << (n - 1)) != 0;
                val.rotate_right(n)
            }
        }
    }
}
