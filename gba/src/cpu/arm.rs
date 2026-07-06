//! ARM (32bit) 命令セットのデコードと実行。
//!
//! デコードはビットパターンのマッチ順に依存する: multiply / swap は
//! halfword 転送パターン (bit7=1, bit4=1) の部分集合、MRS/MSR は
//! データ処理の S=0 テスト命令のエンコーディングを流用しているため、
//! 特殊な形を先に判定する。

use super::{
    Cpu, FLAG_C, FLAG_T, MODE_SVC, MODE_USR, shift_by_immediate, shift_by_register,
};
use crate::bus::Bus;

impl Cpu {
    pub(super) fn exec_arm(&mut self, bus: &mut Bus, op: u32) {
        if op & 0x0FFF_FFF0 == 0x012F_FF10 {
            self.arm_bx(op)
        } else if op & 0x0FC0_00F0 == 0x0000_0090 {
            self.arm_multiply(op)
        } else if op & 0x0F80_00F0 == 0x0080_0090 {
            self.arm_multiply_long(op)
        } else if op & 0x0FB0_0FF0 == 0x0100_0090 {
            self.arm_swap(bus, op)
        } else if op & 0x0E00_0090 == 0x0000_0090 {
            self.arm_halfword(bus, op)
        } else if op & 0x0FBF_0FFF == 0x010F_0000 {
            self.arm_mrs(op)
        } else if op & 0x0FB0_FFF0 == 0x0120_F000 {
            self.arm_msr(op, false)
        } else if op & 0x0FB0_F000 == 0x0320_F000 {
            self.arm_msr(op, true)
        } else if op & 0x0C00_0000 == 0x0000_0000 {
            self.arm_data_processing(op)
        } else if op & 0x0C00_0000 == 0x0400_0000 {
            self.arm_single_transfer(bus, op)
        } else if op & 0x0E00_0000 == 0x0800_0000 {
            self.arm_block_transfer(bus, op)
        } else if op & 0x0E00_0000 == 0x0A00_0000 {
            self.arm_branch(op)
        } else if op & 0x0F00_0000 == 0x0F00_0000 {
            self.do_swi(bus, op >> 16 & 0xFF)
        }
        // 残り（コプロセッサ・未定義）: GBA に実機能はないため無視
    }

    fn arm_bx(&mut self, op: u32) {
        let v = self.regs[(op & 0xF) as usize];
        if v & 1 != 0 {
            self.cpsr |= FLAG_T;
            self.regs[15] = v & !1;
        } else {
            self.cpsr &= !FLAG_T;
            self.regs[15] = v & !3;
        }
        self.branched = true;
    }

    fn arm_branch(&mut self, op: u32) {
        // 24bit 符号付きオフセット ×4
        let off = ((op as i32) << 8 >> 6) as u32;
        if op & 1 << 24 != 0 {
            self.regs[14] = self.regs[15].wrapping_sub(4);
        }
        self.regs[15] = self.regs[15].wrapping_add(off);
        self.branched = true;
    }

    /// オペランド 2 のレジスタシフト形。シフト量レジスタ指定時は r15 が +12 で読める。
    fn shifted_register(&mut self, op: u32, carry: &mut bool) -> u32 {
        let rm = (op & 0xF) as usize;
        let ty = op >> 5 & 3;
        if op & 1 << 4 != 0 {
            let amount = self.regs[(op >> 8 & 0xF) as usize] & 0xFF;
            let mut val = self.regs[rm];
            if rm == 15 {
                val = val.wrapping_add(4);
            }
            self.extra_cycles += 1;
            shift_by_register(val, ty, amount, carry)
        } else {
            shift_by_immediate(self.regs[rm], ty, op >> 7 & 0x1F, carry)
        }
    }

    fn arm_data_processing(&mut self, op: u32) {
        let opcode = op >> 21 & 0xF;
        let s = op & 1 << 20 != 0;
        let rn = (op >> 16 & 0xF) as usize;
        let rd = (op >> 12 & 0xF) as usize;
        let mut carry = self.cpsr & FLAG_C != 0;
        let reg_shift = op & 1 << 25 == 0 && op & 1 << 4 != 0;

        let op2 = if op & 1 << 25 != 0 {
            let rot = (op >> 8 & 0xF) * 2;
            let v = (op & 0xFF).rotate_right(rot);
            if rot != 0 {
                carry = v >> 31 != 0;
            }
            v
        } else {
            self.shifted_register(op, &mut carry)
        };
        let mut a = self.regs[rn];
        if rn == 15 && reg_shift {
            a = a.wrapping_add(4);
        }

        let result = match opcode {
            0x0 | 0x8 => a & op2,                       // AND / TST
            0x1 | 0x9 => a ^ op2,                       // EOR / TEQ
            0x2 | 0xA => self.sub_flags(a, op2, s),     // SUB / CMP
            0x3 => self.sub_flags(op2, a, s),           // RSB
            0x4 | 0xB => self.add_flags(a, op2, s),     // ADD / CMN
            0x5 => self.adc_flags(a, op2, s),           // ADC
            0x6 => self.sbc_flags(a, op2, s),           // SBC
            0x7 => self.sbc_flags(op2, a, s),           // RSC
            0xC => a | op2,                             // ORR
            0xD => op2,                                 // MOV
            0xE => a & !op2,                            // BIC
            _ => !op2,                                  // MVN
        };
        if s && matches!(opcode, 0x0 | 0x1 | 0x8 | 0x9 | 0xC | 0xD | 0xE | 0xF) {
            self.set_nz(result);
            self.set_c(carry);
        }
        if !(0x8..=0xB).contains(&opcode) {
            self.regs[rd] = result;
            if rd == 15 {
                self.branched = true;
                if s {
                    self.restore_cpsr();
                }
            }
        }
    }

    fn arm_multiply(&mut self, op: u32) {
        let rd = (op >> 16 & 0xF) as usize;
        let rn = (op >> 12 & 0xF) as usize;
        let rs = (op >> 8 & 0xF) as usize;
        let rm = (op & 0xF) as usize;
        let mut r = self.regs[rm].wrapping_mul(self.regs[rs]);
        if op & 1 << 21 != 0 {
            r = r.wrapping_add(self.regs[rn]);
        }
        self.regs[rd] = r;
        if op & 1 << 20 != 0 {
            self.set_nz(r);
        }
        self.extra_cycles += 2;
    }

    fn arm_multiply_long(&mut self, op: u32) {
        let rdhi = (op >> 16 & 0xF) as usize;
        let rdlo = (op >> 12 & 0xF) as usize;
        let rs = (op >> 8 & 0xF) as usize;
        let rm = (op & 0xF) as usize;
        let mut r = if op & 1 << 22 != 0 {
            (self.regs[rm] as i32 as i64).wrapping_mul(self.regs[rs] as i32 as i64) as u64
        } else {
            self.regs[rm] as u64 * self.regs[rs] as u64
        };
        if op & 1 << 21 != 0 {
            r = r.wrapping_add((self.regs[rdhi] as u64) << 32 | self.regs[rdlo] as u64);
        }
        self.regs[rdhi] = (r >> 32) as u32;
        self.regs[rdlo] = r as u32;
        if op & 1 << 20 != 0 {
            self.cpsr = self.cpsr & !(super::FLAG_N | super::FLAG_Z)
                | if r == 0 { super::FLAG_Z } else { 0 }
                | ((r >> 32) as u32 & super::FLAG_N);
        }
        self.extra_cycles += 3;
    }

    fn arm_swap(&mut self, bus: &mut Bus, op: u32) {
        let rn = (op >> 16 & 0xF) as usize;
        let rd = (op >> 12 & 0xF) as usize;
        let rm = (op & 0xF) as usize;
        let addr = self.regs[rn];
        if op & 1 << 22 != 0 {
            let v = bus.read8(addr);
            bus.write8(addr, self.regs[rm] as u8);
            self.regs[rd] = v as u32;
        } else {
            let v = bus.read32(addr).rotate_right(8 * (addr & 3));
            bus.write32(addr, self.regs[rm]);
            self.regs[rd] = v;
        }
        self.extra_cycles += 2;
    }

    fn arm_mrs(&mut self, op: u32) {
        let rd = (op >> 12 & 0xF) as usize;
        self.regs[rd] = if op & 1 << 22 != 0 { self.spsr() } else { self.cpsr };
    }

    fn arm_msr(&mut self, op: u32, imm: bool) {
        let val = if imm {
            (op & 0xFF).rotate_right((op >> 8 & 0xF) * 2)
        } else {
            self.regs[(op & 0xF) as usize]
        };
        let mut mask = 0u32;
        for (bit, m) in [(16, 0xFFu32), (17, 0xFF00), (18, 0xFF_0000), (19, 0xFF00_0000)] {
            if op & 1 << bit != 0 {
                mask |= m;
            }
        }
        if op & 1 << 22 != 0 {
            self.set_spsr(self.spsr() & !mask | val & mask);
        } else {
            if self.mode() == MODE_USR {
                mask &= 0xFF00_0000; // ユーザーモードは制御ビット変更不可
            }
            if mask & 0x1F != 0 {
                self.set_mode(val & 0x1F);
            }
            // T ビットは MSR で変更できない
            let mask = mask & !FLAG_T;
            self.cpsr = self.cpsr & !mask | val & mask;
        }
    }

    fn arm_halfword(&mut self, bus: &mut Bus, op: u32) {
        let pre = op & 1 << 24 != 0;
        let up = op & 1 << 23 != 0;
        let wb = op & 1 << 21 != 0;
        let load = op & 1 << 20 != 0;
        let rn = (op >> 16 & 0xF) as usize;
        let rd = (op >> 12 & 0xF) as usize;
        let off = if op & 1 << 22 != 0 {
            op >> 4 & 0xF0 | op & 0xF
        } else {
            self.regs[(op & 0xF) as usize]
        };
        let base = self.regs[rn];
        let target = if up { base.wrapping_add(off) } else { base.wrapping_sub(off) };
        let addr = if pre { target } else { base };

        if load {
            let v = match op >> 5 & 3 {
                // LDRH: 奇数アドレスはローテートして読める (ARM7 の挙動)
                1 => (bus.read16(addr) as u32).rotate_right(8 * (addr & 1)),
                2 => bus.read8(addr) as i8 as u32,
                // LDRSH: 奇数アドレスは LDRSB 相当
                _ => {
                    if addr & 1 != 0 {
                        bus.read8(addr) as i8 as u32
                    } else {
                        bus.read16(addr) as i16 as u32
                    }
                }
            };
            if (!pre || wb) && rn != rd {
                self.regs[rn] = target;
            }
            self.regs[rd] = v;
            if rd == 15 {
                self.branched = true;
            }
        } else {
            // STRH のみ（SH=2,3 のストアは ARMv4 では未定義）
            let v = self.regs[rd].wrapping_add(if rd == 15 { 4 } else { 0 });
            bus.write16(addr, v as u16);
            if !pre || wb {
                self.regs[rn] = target;
            }
        }
        self.extra_cycles += 1;
    }

    fn arm_single_transfer(&mut self, bus: &mut Bus, op: u32) {
        let pre = op & 1 << 24 != 0;
        let up = op & 1 << 23 != 0;
        let byte = op & 1 << 22 != 0;
        let wb = op & 1 << 21 != 0;
        let load = op & 1 << 20 != 0;
        let rn = (op >> 16 & 0xF) as usize;
        let rd = (op >> 12 & 0xF) as usize;
        let off = if op & 1 << 25 != 0 {
            // レジスタオフセットは即値シフト形のみ（キャリーは捨てる）
            let mut c = self.cpsr & FLAG_C != 0;
            shift_by_immediate(self.regs[(op & 0xF) as usize], op >> 5 & 3, op >> 7 & 0x1F, &mut c)
        } else {
            op & 0xFFF
        };
        let base = self.regs[rn];
        let target = if up { base.wrapping_add(off) } else { base.wrapping_sub(off) };
        let addr = if pre { target } else { base };

        if load {
            let v = if byte {
                bus.read8(addr) as u32
            } else {
                // 非アラインロードはローテートして読める
                bus.read32(addr).rotate_right(8 * (addr & 3))
            };
            if (!pre || wb) && rn != rd {
                self.regs[rn] = target;
            }
            self.regs[rd] = v;
            if rd == 15 {
                self.branched = true;
            }
        } else {
            // STR r15 は PC+12 を格納する
            let v = self.regs[rd].wrapping_add(if rd == 15 { 4 } else { 0 });
            if byte {
                bus.write8(addr, v as u8);
            } else {
                bus.write32(addr, v);
            }
            if !pre || wb {
                self.regs[rn] = target;
            }
        }
        self.extra_cycles += 1;
    }

    fn arm_block_transfer(&mut self, bus: &mut Bus, op: u32) {
        let pre = op & 1 << 24 != 0;
        let up = op & 1 << 23 != 0;
        let s = op & 1 << 22 != 0;
        let wb = op & 1 << 21 != 0;
        let load = op & 1 << 20 != 0;
        let rn = (op >> 16 & 0xF) as usize;
        let list = op & 0xFFFF;
        let n = list.count_ones();
        if n == 0 {
            return; // 空リストの特殊挙動（r15 転送 + base±0x40）は未対応
        }
        let base = self.regs[rn];
        // 転送は常に昇順アドレスで行い、開始位置だけをモードで変える
        let (mut addr, new_base) = if up {
            (if pre { base.wrapping_add(4) } else { base }, base.wrapping_add(4 * n))
        } else {
            let low = base.wrapping_sub(4 * n);
            (if pre { low } else { low.wrapping_add(4) }, low)
        };
        // S ビット: LDM で r15 を含む場合は SPSR 復帰、それ以外はユーザーバンク転送
        let user_bank = s && !(load && list & 0x8000 != 0);
        let first = list.trailing_zeros() as usize;

        if load {
            if wb && list & 1 << rn == 0 {
                self.regs[rn] = new_base;
            }
            for r in 0..16 {
                if list & 1 << r == 0 {
                    continue;
                }
                let v = bus.read32(addr);
                addr = addr.wrapping_add(4);
                if user_bank {
                    self.set_user_reg(r, v);
                } else {
                    self.regs[r] = v;
                }
                if r == 15 {
                    self.branched = true;
                    if s {
                        self.restore_cpsr();
                    }
                }
            }
        } else {
            for r in 0..16 {
                if list & 1 << r == 0 {
                    continue;
                }
                let v = if r == 15 {
                    self.regs[15].wrapping_add(4) // PC+12
                } else if r == rn {
                    // ベースがリスト先頭なら旧値、それ以外は書き戻し後の値が格納される
                    if r == first { base } else { new_base }
                } else if user_bank {
                    self.user_reg(r)
                } else {
                    self.regs[r]
                };
                bus.write32(addr, v);
                addr = addr.wrapping_add(4);
            }
            if wb {
                self.regs[rn] = new_base;
            }
        }
        self.extra_cycles += n + 1;
    }

    pub(super) fn do_swi(&mut self, bus: &mut Bus, num: u32) {
        if self.hle_bios {
            super::swi::execute(self, bus, num);
        } else {
            let lr = self.regs[15].wrapping_sub(if self.thumb() { 2 } else { 4 });
            self.exception(MODE_SVC, 0x08, lr);
        }
    }
}
