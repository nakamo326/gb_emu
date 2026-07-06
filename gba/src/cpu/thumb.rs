//! Thumb (16bit) 命令セットのデコードと実行。

use super::{Cpu, FLAG_C, FLAG_T, shift_by_immediate, shift_by_register};
use crate::bus::Bus;

impl Cpu {
    pub(super) fn exec_thumb(&mut self, bus: &mut Bus, op: u16) {
        let op = op as u32;
        match op >> 12 {
            0x0 | 0x1 => {
                if op >> 11 == 3 {
                    // ADD/SUB (レジスタ or 3bit 即値)
                    let rd = (op & 7) as usize;
                    let a = self.regs[(op >> 3 & 7) as usize];
                    let b = if op & 1 << 10 != 0 {
                        op >> 6 & 7
                    } else {
                        self.regs[(op >> 6 & 7) as usize]
                    };
                    self.regs[rd] = if op & 1 << 9 != 0 {
                        self.sub_flags(a, b, true)
                    } else {
                        self.add_flags(a, b, true)
                    };
                } else {
                    // LSL/LSR/ASR 即値シフト
                    let rd = (op & 7) as usize;
                    let mut carry = self.cpsr & FLAG_C != 0;
                    let r = shift_by_immediate(
                        self.regs[(op >> 3 & 7) as usize],
                        op >> 11 & 3,
                        op >> 6 & 0x1F,
                        &mut carry,
                    );
                    self.regs[rd] = r;
                    self.set_nz(r);
                    self.set_c(carry);
                }
            }
            0x2 | 0x3 => {
                // MOV/CMP/ADD/SUB 8bit 即値
                let rd = (op >> 8 & 7) as usize;
                let imm = op & 0xFF;
                match op >> 11 & 3 {
                    0 => {
                        self.regs[rd] = imm;
                        self.set_nz(imm);
                    }
                    1 => {
                        self.sub_flags(self.regs[rd], imm, true);
                    }
                    2 => self.regs[rd] = self.add_flags(self.regs[rd], imm, true),
                    _ => self.regs[rd] = self.sub_flags(self.regs[rd], imm, true),
                }
            }
            0x4 => {
                if op & 0xFC00 == 0x4000 {
                    self.thumb_alu(op);
                } else if op & 0xFC00 == 0x4400 {
                    self.thumb_hi_reg(op);
                } else {
                    // LDR pc 相対（pc は 4 バイト境界へ切り下げ）
                    let rd = (op >> 8 & 7) as usize;
                    let addr = (self.regs[15] & !2).wrapping_add((op & 0xFF) * 4);
                    self.regs[rd] = bus.read32(addr);
                    self.extra_cycles += 1;
                }
            }
            0x5 => {
                // レジスタオフセットのロード/ストア (bit11-9 がオペコード)
                let addr = self.regs[(op >> 3 & 7) as usize]
                    .wrapping_add(self.regs[(op >> 6 & 7) as usize]);
                let rd = (op & 7) as usize;
                match op >> 9 & 7 {
                    0 => bus.write32(addr, self.regs[rd]),
                    1 => bus.write16(addr, self.regs[rd] as u16),
                    2 => bus.write8(addr, self.regs[rd] as u8),
                    3 => self.regs[rd] = bus.read8(addr) as i8 as u32, // LDSB
                    4 => self.regs[rd] = bus.read32(addr).rotate_right(8 * (addr & 3)),
                    5 => self.regs[rd] = (bus.read16(addr) as u32).rotate_right(8 * (addr & 1)),
                    6 => self.regs[rd] = bus.read8(addr) as u32,
                    _ => {
                        // LDSH: 奇数アドレスは LDSB 相当 (ARM7 の挙動)
                        self.regs[rd] = if addr & 1 != 0 {
                            bus.read8(addr) as i8 as u32
                        } else {
                            bus.read16(addr) as i16 as u32
                        };
                    }
                }
                self.extra_cycles += 1;
            }
            0x6 | 0x7 => {
                // 5bit 即値オフセットの word/byte ロード/ストア
                let off = op >> 6 & 0x1F;
                let byte = op & 1 << 12 != 0;
                let rd = (op & 7) as usize;
                let addr = self.regs[(op >> 3 & 7) as usize]
                    .wrapping_add(if byte { off } else { off * 4 });
                match (op & 1 << 11 != 0, byte) {
                    (false, false) => bus.write32(addr, self.regs[rd]),
                    (false, true) => bus.write8(addr, self.regs[rd] as u8),
                    (true, false) => {
                        self.regs[rd] = bus.read32(addr).rotate_right(8 * (addr & 3))
                    }
                    (true, true) => self.regs[rd] = bus.read8(addr) as u32,
                }
                self.extra_cycles += 1;
            }
            0x8 => {
                // STRH/LDRH 即値オフセット
                let addr = self.regs[(op >> 3 & 7) as usize].wrapping_add((op >> 6 & 0x1F) * 2);
                let rd = (op & 7) as usize;
                if op & 1 << 11 != 0 {
                    self.regs[rd] = (bus.read16(addr) as u32).rotate_right(8 * (addr & 1));
                } else {
                    bus.write16(addr, self.regs[rd] as u16);
                }
                self.extra_cycles += 1;
            }
            0x9 => {
                // SP 相対ロード/ストア
                let rd = (op >> 8 & 7) as usize;
                let addr = self.regs[13].wrapping_add((op & 0xFF) * 4);
                if op & 1 << 11 != 0 {
                    self.regs[rd] = bus.read32(addr).rotate_right(8 * (addr & 3));
                } else {
                    bus.write32(addr, self.regs[rd]);
                }
                self.extra_cycles += 1;
            }
            0xA => {
                // ADD rd, pc/sp, #imm
                let rd = (op >> 8 & 7) as usize;
                let base = if op & 1 << 11 != 0 { self.regs[13] } else { self.regs[15] & !2 };
                self.regs[rd] = base.wrapping_add((op & 0xFF) * 4);
            }
            0xB => {
                if op & 0xFF00 == 0xB000 {
                    // ADD SP, #±imm
                    let off = (op & 0x7F) * 4;
                    self.regs[13] = if op & 1 << 7 != 0 {
                        self.regs[13].wrapping_sub(off)
                    } else {
                        self.regs[13].wrapping_add(off)
                    };
                } else if op & 0xF600 == 0xB400 {
                    self.thumb_push_pop(bus, op);
                }
                // それ以外（BKPT 等）は ARMv4T に存在しない
            }
            0xC => self.thumb_block_transfer(bus, op),
            0xD => {
                let cond = op >> 8 & 0xF;
                if cond == 0xF {
                    self.do_swi(bus, op & 0xFF);
                } else if cond != 0xE && self.check_cond(cond) {
                    let off = (op as u8 as i8 as i32 * 2) as u32;
                    self.regs[15] = self.regs[15].wrapping_add(off);
                    self.branched = true;
                }
            }
            0xE => {
                // B (11bit 符号付き ×2)
                let off = ((op as i32) << 21 >> 20) as u32;
                self.regs[15] = self.regs[15].wrapping_add(off);
                self.branched = true;
            }
            _ => {
                // BL は 2 命令ペア
                if op & 1 << 11 == 0 {
                    // 前半: LR = PC + (符号付き 11bit << 12)
                    let off = ((op as i32) << 21 >> 9) as u32;
                    self.regs[14] = self.regs[15].wrapping_add(off);
                } else {
                    // 後半: 分岐して LR に戻り番地 (Thumb ビット付き)
                    let ret = self.regs[15].wrapping_sub(2) | 1;
                    self.regs[15] = self.regs[14].wrapping_add((op & 0x7FF) * 2);
                    self.regs[14] = ret;
                    self.branched = true;
                }
            }
        }
    }

    fn thumb_alu(&mut self, op: u32) {
        let rd = (op & 7) as usize;
        let a = self.regs[rd];
        let b = self.regs[(op >> 3 & 7) as usize];
        let mut carry = self.cpsr & FLAG_C != 0;
        let logical = |cpu: &mut Self, r: u32| {
            cpu.regs[rd] = r;
            cpu.set_nz(r);
        };
        match op >> 6 & 0xF {
            0x0 => logical(self, a & b),
            0x1 => logical(self, a ^ b),
            0x2 | 0x3 | 0x4 | 0x7 => {
                // LSL/LSR/ASR/ROR (シフト量レジスタ)
                let ty = match op >> 6 & 0xF {
                    0x2 => 0,
                    0x3 => 1,
                    0x4 => 2,
                    _ => 3,
                };
                let r = shift_by_register(a, ty, b & 0xFF, &mut carry);
                self.regs[rd] = r;
                self.set_nz(r);
                self.set_c(carry);
                self.extra_cycles += 1;
            }
            0x5 => self.regs[rd] = self.adc_flags(a, b, true),
            0x6 => self.regs[rd] = self.sbc_flags(a, b, true),
            0x8 => self.set_nz(a & b), // TST
            0x9 => self.regs[rd] = self.sub_flags(0, b, true), // NEG
            0xA => {
                self.sub_flags(a, b, true); // CMP
            }
            0xB => {
                self.add_flags(a, b, true); // CMN
            }
            0xC => logical(self, a | b),
            0xD => {
                let r = a.wrapping_mul(b);
                logical(self, r);
                self.extra_cycles += 2;
            }
            0xE => logical(self, a & !b),
            _ => logical(self, !b),
        }
    }

    fn thumb_hi_reg(&mut self, op: u32) {
        let rd = (op & 7 | op >> 4 & 8) as usize;
        let rs = (op >> 3 & 0xF) as usize;
        match op >> 8 & 3 {
            0 => {
                let r = self.regs[rd].wrapping_add(self.regs[rs]);
                self.regs[rd] = r;
                if rd == 15 {
                    self.branched = true;
                }
            }
            1 => {
                self.sub_flags(self.regs[rd], self.regs[rs], true); // CMP
            }
            2 => {
                self.regs[rd] = self.regs[rs];
                if rd == 15 {
                    self.branched = true;
                }
            }
            _ => {
                // BX
                let v = self.regs[rs];
                if v & 1 != 0 {
                    self.regs[15] = v & !1;
                } else {
                    self.cpsr &= !FLAG_T;
                    self.regs[15] = v & !3;
                }
                self.branched = true;
            }
        }
    }

    fn thumb_push_pop(&mut self, bus: &mut Bus, op: u32) {
        let list = op & 0xFF;
        let extra = op & 1 << 8 != 0; // PUSH なら LR、POP なら PC
        let n = list.count_ones() + extra as u32;
        if op & 1 << 11 != 0 {
            let mut addr = self.regs[13];
            for r in 0..8 {
                if list & 1 << r != 0 {
                    self.regs[r] = bus.read32(addr);
                    addr = addr.wrapping_add(4);
                }
            }
            if extra {
                self.regs[15] = bus.read32(addr) & !1;
                addr = addr.wrapping_add(4);
                self.branched = true;
            }
            self.regs[13] = addr;
        } else {
            let mut addr = self.regs[13].wrapping_sub(4 * n);
            self.regs[13] = addr;
            for r in 0..8 {
                if list & 1 << r != 0 {
                    bus.write32(addr, self.regs[r]);
                    addr = addr.wrapping_add(4);
                }
            }
            if extra {
                bus.write32(addr, self.regs[14]);
            }
        }
        self.extra_cycles += n;
    }

    fn thumb_block_transfer(&mut self, bus: &mut Bus, op: u32) {
        let rb = (op >> 8 & 7) as usize;
        let list = op & 0xFF;
        let n = list.count_ones();
        if n == 0 {
            return; // 空リストの特殊挙動は未対応
        }
        let base = self.regs[rb];
        let mut addr = base;
        let new_base = base.wrapping_add(4 * n);
        if op & 1 << 11 != 0 {
            // LDMIA: ライトバックはロード前（rb がリストにあればロード値が勝つ）
            self.regs[rb] = new_base;
            for r in 0..8 {
                if list & 1 << r != 0 {
                    self.regs[r] = bus.read32(addr);
                    addr = addr.wrapping_add(4);
                }
            }
        } else {
            let first = list.trailing_zeros() as usize;
            for r in 0..8 {
                if list & 1 << r == 0 {
                    continue;
                }
                // rb 自身の格納値: リスト先頭なら旧値、以降なら書き戻し後の値
                let v = if r == rb && r != first { new_base } else { self.regs[r] };
                bus.write32(addr, v);
                addr = addr.wrapping_add(4);
            }
            self.regs[rb] = new_base;
        }
        self.extra_cycles += n;
    }
}
