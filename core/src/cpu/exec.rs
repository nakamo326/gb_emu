//! 命令カテゴリ別の実行関数 `exec_*` と共通ヘルパー。
//! 各 exec は `cpu.instr.step` を見て分岐し、1 M-cycle分の処理を行い、命令完了で `true` を返す。
//! メモリアクセスは1ステップ(=1 M-cycle)を消費し、レジスタアクセスは0サイクル(同ステップで連鎖)。

use super::Cpu;
use super::decode;
use super::operand::{Cond, Indirect, Operand, Reg8, Reg16};
use crate::mmu::MemoryBus;

// ──────────────────────────────────────────────
// Cpu ヘルパー(オペランドアクセス・演算)
// ──────────────────────────────────────────────
impl Cpu {
    fn read_reg8(&self, r: Reg8) -> u8 {
        match r {
            Reg8::A => self.regs.a,
            Reg8::B => self.regs.b,
            Reg8::C => self.regs.c,
            Reg8::D => self.regs.d,
            Reg8::E => self.regs.e,
            Reg8::H => self.regs.h,
            Reg8::L => self.regs.l,
        }
    }
    fn write_reg8(&mut self, r: Reg8, v: u8) {
        match r {
            Reg8::A => self.regs.a = v,
            Reg8::B => self.regs.b = v,
            Reg8::C => self.regs.c = v,
            Reg8::D => self.regs.d = v,
            Reg8::E => self.regs.e = v,
            Reg8::H => self.regs.h = v,
            Reg8::L => self.regs.l = v,
        }
    }
    fn read_reg16(&self, r: Reg16) -> u16 {
        match r {
            Reg16::AF => self.regs.af(),
            Reg16::BC => self.regs.bc(),
            Reg16::DE => self.regs.de(),
            Reg16::HL => self.regs.hl(),
            Reg16::SP => self.regs.sp,
        }
    }
    fn write_reg16(&mut self, r: Reg16, v: u16) {
        match r {
            Reg16::AF => self.regs.write_af(v),
            Reg16::BC => self.regs.write_bc(v),
            Reg16::DE => self.regs.write_de(v),
            Reg16::HL => self.regs.write_hl(v),
            Reg16::SP => self.regs.sp = v,
        }
    }
    /// 間接アドレスを返す。HLI/HLD は HL を増減する副作用を伴う(呼び出しは1アクセスにつき1回)。
    fn ind_addr(&mut self, i: Indirect) -> u16 {
        match i {
            Indirect::BC => self.regs.bc(),
            Indirect::DE => self.regs.de(),
            Indirect::HL => self.regs.hl(),
            Indirect::HLI => {
                let a = self.regs.hl();
                self.regs.write_hl(a.wrapping_add(1));
                a
            }
            Indirect::HLD => {
                let a = self.regs.hl();
                self.regs.write_hl(a.wrapping_sub(1));
                a
            }
            Indirect::CFF => 0xFF00 | self.regs.c as u16,
        }
    }
    fn cond(&self, c: Cond) -> bool {
        match c {
            Cond::NZ => !self.regs.zf(),
            Cond::Z => self.regs.zf(),
            Cond::NC => !self.regs.cf(),
            Cond::C => self.regs.cf(),
        }
    }
    /// PC から1バイト読み、PC を進める
    fn imm8(&mut self, bus: &dyn MemoryBus) -> u8 {
        let v = bus.read(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        v
    }

    // ── 8bit ALU(A レジスタ対象) ──
    fn alu_add(&mut self, v: u8) {
        let a = self.regs.a;
        let (r, c) = a.overflowing_add(v);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(false);
        self.regs.set_hf((a & 0x0F) + (v & 0x0F) > 0x0F);
        self.regs.set_cf(c);
        self.regs.a = r;
    }
    fn alu_adc(&mut self, v: u8) {
        let a = self.regs.a;
        let c = self.regs.cf() as u8;
        let r = a.wrapping_add(v).wrapping_add(c);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(false);
        self.regs.set_hf((a & 0x0F) + (v & 0x0F) + c > 0x0F);
        self.regs.set_cf((a as u16) + (v as u16) + (c as u16) > 0xFF);
        self.regs.a = r;
    }
    fn alu_sub(&mut self, v: u8) {
        let a = self.regs.a;
        let (r, c) = a.overflowing_sub(v);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(true);
        self.regs.set_hf((a & 0x0F) < (v & 0x0F));
        self.regs.set_cf(c);
        self.regs.a = r;
    }
    fn alu_sbc(&mut self, v: u8) {
        let a = self.regs.a;
        let c = self.regs.cf() as u8;
        let r = a.wrapping_sub(v).wrapping_sub(c);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(true);
        self.regs.set_hf((a & 0x0F) < (v & 0x0F) + c);
        self.regs.set_cf((a as u16) < (v as u16) + (c as u16));
        self.regs.a = r;
    }
    fn alu_and(&mut self, v: u8) {
        self.regs.a &= v;
        self.regs.set_zf(self.regs.a == 0);
        self.regs.set_nf(false);
        self.regs.set_hf(true);
        self.regs.set_cf(false);
    }
    fn alu_or(&mut self, v: u8) {
        self.regs.a |= v;
        self.regs.set_zf(self.regs.a == 0);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(false);
    }
    fn alu_xor(&mut self, v: u8) {
        self.regs.a ^= v;
        self.regs.set_zf(self.regs.a == 0);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(false);
    }
    fn alu_cp(&mut self, v: u8) {
        let a = self.regs.a;
        let (r, c) = a.overflowing_sub(v);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(true);
        self.regs.set_hf((a & 0x0F) < (v & 0x0F));
        self.regs.set_cf(c);
    }
    fn inc8_val(&mut self, v: u8) -> u8 {
        let r = v.wrapping_add(1);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(false);
        self.regs.set_hf((v & 0x0F) == 0x0F);
        r
    }
    fn dec8_val(&mut self, v: u8) -> u8 {
        let r = v.wrapping_sub(1);
        self.regs.set_zf(r == 0);
        self.regs.set_nf(true);
        self.regs.set_hf((v & 0x0F) == 0);
        r
    }

    // ── CB ローテート/シフト変換(フラグ設定込み) ──
    fn rlc_t(&mut self, v: u8) -> u8 {
        let c = v & 0x80 != 0;
        let r = (v << 1) | c as u8;
        self.set_logic_flags(r, c);
        r
    }
    fn rrc_t(&mut self, v: u8) -> u8 {
        let c = v & 0x01 != 0;
        let r = (v >> 1) | ((c as u8) << 7);
        self.set_logic_flags(r, c);
        r
    }
    fn rl_t(&mut self, v: u8) -> u8 {
        let r = (v << 1) | self.regs.cf() as u8;
        self.set_logic_flags(r, v & 0x80 != 0);
        r
    }
    fn rr_t(&mut self, v: u8) -> u8 {
        let oc = self.regs.cf() as u8;
        let nc = v & 0x01 != 0;
        let r = (v >> 1) | (oc << 7);
        self.set_logic_flags(r, nc);
        r
    }
    fn sla_t(&mut self, v: u8) -> u8 {
        let c = v & 0x80 != 0;
        let r = v << 1;
        self.set_logic_flags(r, c);
        r
    }
    fn sra_t(&mut self, v: u8) -> u8 {
        let c = v & 0x01 != 0;
        let r = (v >> 1) | (v & 0x80);
        self.set_logic_flags(r, c);
        r
    }
    fn swap_t(&mut self, v: u8) -> u8 {
        let r = v.rotate_left(4); // 上位/下位ニブル入れ替え
        self.regs.set_zf(r == 0);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(false);
        r
    }
    fn srl_t(&mut self, v: u8) -> u8 {
        let c = v & 0x01 != 0;
        let r = v >> 1;
        self.set_logic_flags(r, c);
        r
    }
    /// ローテート/シフト系の共通フラグ(Z=結果0, N=0, H=0, C=carry)
    fn set_logic_flags(&mut self, r: u8, carry: bool) {
        self.regs.set_zf(r == 0);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(carry);
    }
    fn bit_test(&mut self, v: u8, n: u8) {
        self.regs.set_zf(v & (1 << n) == 0);
        self.regs.set_nf(false);
        self.regs.set_hf(true);
    }
}

/// src オペランドの値を `cpu.wz` に読み込む。
/// レジスタなら0サイクルで `true`、Imm/Ind なら1サイクル消費して `false` を返す。
fn load_src8(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.src {
        Operand::Reg8(r) => {
            cpu.wz = cpu.read_reg8(r) as u16;
            true
        }
        Operand::Imm => {
            cpu.wz = cpu.imm8(bus) as u16;
            false
        }
        Operand::Ind(i) => {
            let a = cpu.ind_addr(i);
            cpu.wz = bus.read(a) as u16;
            false
        }
        _ => unreachable!(),
    }
}

// ──────────────────────────────────────────────
// LD
// ──────────────────────────────────────────────
pub(crate) fn exec_nop(_cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    true
}

/// LD 8bit: r,r / r,n / r,(ind) / (ind),r / (ind),n / A,(C) / (C),A
pub(crate) fn exec_ld8(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    loop {
        match cpu.instr.step {
            0 => match cpu.instr.src {
                Operand::Reg8(r) => {
                    cpu.wz = cpu.read_reg8(r) as u16;
                    cpu.instr.step = 1;
                }
                Operand::Imm => {
                    cpu.wz = cpu.imm8(bus) as u16;
                    cpu.instr.step = 1;
                    return false;
                }
                Operand::Ind(i) => {
                    let a = cpu.ind_addr(i);
                    cpu.wz = bus.read(a) as u16;
                    cpu.instr.step = 1;
                    return false;
                }
                _ => unreachable!(),
            },
            1 => match cpu.instr.dst {
                Operand::Reg8(r) => {
                    cpu.write_reg8(r, cpu.wz as u8);
                    return true;
                }
                Operand::Ind(i) => {
                    let a = cpu.ind_addr(i);
                    bus.write(a, cpu.wz as u8);
                    cpu.instr.step = 2;
                    return false;
                }
                _ => unreachable!(),
            },
            2 => return true,
            _ => unreachable!(),
        }
    }
}

/// LD A,(a16) (0xFA)
pub(crate) fn exec_ld_a_abs(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (cpu.imm8(bus) as u16) << 8;
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.regs.a = bus.read(cpu.wz);
            cpu.instr.step = 3;
            false
        }
        3 => true,
        _ => unreachable!(),
    }
}

/// LD (a16),A (0xEA)
pub(crate) fn exec_ld_abs_a(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (cpu.imm8(bus) as u16) << 8;
            cpu.instr.step = 2;
            false
        }
        2 => {
            bus.write(cpu.wz, cpu.regs.a);
            cpu.instr.step = 3;
            false
        }
        3 => true,
        _ => unreachable!(),
    }
}

/// LDH A,(a8) (0xF0)
pub(crate) fn exec_ldh_a_n(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = 0xFF00 | cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.regs.a = bus.read(cpu.wz);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

/// LDH (a8),A (0xE0)
pub(crate) fn exec_ldh_n_a(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = 0xFF00 | cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            bus.write(cpu.wz, cpu.regs.a);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

/// LD rr,d16
pub(crate) fn exec_ld16(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (cpu.imm8(bus) as u16) << 8;
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.write_reg16(cpu.instr.dst.reg16(), cpu.wz);
            true
        }
        _ => unreachable!(),
    }
}

/// LD (a16),SP (0x08)
pub(crate) fn exec_ld_nn_sp(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (cpu.imm8(bus) as u16) << 8;
            cpu.instr.step = 2;
            false
        }
        2 => {
            bus.write(cpu.wz, cpu.regs.sp as u8);
            cpu.instr.step = 3;
            false
        }
        3 => {
            bus.write(cpu.wz.wrapping_add(1), (cpu.regs.sp >> 8) as u8);
            cpu.instr.step = 4;
            false
        }
        4 => true,
        _ => unreachable!(),
    }
}

/// LD SP,HL (0xF9)
pub(crate) fn exec_ld_sp_hl(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.regs.sp = cpu.regs.hl();
            cpu.instr.step = 1;
            false
        }
        1 => true,
        _ => unreachable!(),
    }
}

/// LD HL,SP+e8 (0xF8)
pub(crate) fn exec_ldhl(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            let e = cpu.imm8(bus) as i8 as i16 as u16;
            let sp = cpu.regs.sp;
            cpu.regs.set_zf(false);
            cpu.regs.set_nf(false);
            cpu.regs.set_hf((sp & 0x000F) + (e & 0x000F) > 0x000F);
            cpu.regs.set_cf((sp & 0x00FF) + (e & 0x00FF) > 0x00FF);
            cpu.regs.write_hl(sp.wrapping_add(e));
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

// ──────────────────────────────────────────────
// 8bit ALU
// ──────────────────────────────────────────────
macro_rules! alu_exec {
    ($name:ident, $op:ident) => {
        pub(crate) fn $name(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
            match cpu.instr.step {
                0 => {
                    if load_src8(cpu, bus) {
                        cpu.$op(cpu.wz as u8);
                        true
                    } else {
                        cpu.instr.step = 1;
                        false
                    }
                }
                1 => {
                    cpu.$op(cpu.wz as u8);
                    true
                }
                _ => unreachable!(),
            }
        }
    };
}
alu_exec!(exec_add, alu_add);
alu_exec!(exec_adc, alu_adc);
alu_exec!(exec_sub, alu_sub);
alu_exec!(exec_sbc, alu_sbc);
alu_exec!(exec_and, alu_and);
alu_exec!(exec_or, alu_or);
alu_exec!(exec_xor, alu_xor);
alu_exec!(exec_cp, alu_cp);

/// INC r / INC (HL)
pub(crate) fn exec_inc8(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => match cpu.instr.dst {
            Operand::Reg8(r) => {
                let v = cpu.read_reg8(r);
                let nv = cpu.inc8_val(v);
                cpu.write_reg8(r, nv);
                true
            }
            Operand::Ind(i) => {
                let a = cpu.ind_addr(i);
                cpu.wz = bus.read(a) as u16;
                cpu.instr.step = 1;
                false
            }
            _ => unreachable!(),
        },
        1 => {
            let nv = cpu.inc8_val(cpu.wz as u8);
            let a = cpu.ind_addr(cpu.instr.dst.ind());
            bus.write(a, nv);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

/// DEC r / DEC (HL)
pub(crate) fn exec_dec8(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => match cpu.instr.dst {
            Operand::Reg8(r) => {
                let v = cpu.read_reg8(r);
                let nv = cpu.dec8_val(v);
                cpu.write_reg8(r, nv);
                true
            }
            Operand::Ind(i) => {
                let a = cpu.ind_addr(i);
                cpu.wz = bus.read(a) as u16;
                cpu.instr.step = 1;
                false
            }
            _ => unreachable!(),
        },
        1 => {
            let nv = cpu.dec8_val(cpu.wz as u8);
            let a = cpu.ind_addr(cpu.instr.dst.ind());
            bus.write(a, nv);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

/// INC rr (2 M-cycle)
pub(crate) fn exec_inc16(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            let r = cpu.instr.dst.reg16();
            let v = cpu.read_reg16(r);
            cpu.write_reg16(r, v.wrapping_add(1));
            cpu.instr.step = 1;
            false
        }
        1 => true,
        _ => unreachable!(),
    }
}

/// DEC rr (2 M-cycle)
pub(crate) fn exec_dec16(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            let r = cpu.instr.dst.reg16();
            let v = cpu.read_reg16(r);
            cpu.write_reg16(r, v.wrapping_sub(1));
            cpu.instr.step = 1;
            false
        }
        1 => true,
        _ => unreachable!(),
    }
}

/// ADD HL,rr (2 M-cycle)
pub(crate) fn exec_add_hl(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            let v = cpu.read_reg16(cpu.instr.src.reg16());
            let hl = cpu.regs.hl();
            let (r, c) = hl.overflowing_add(v);
            cpu.regs.set_nf(false);
            cpu.regs.set_hf((hl & 0x0FFF) + (v & 0x0FFF) > 0x0FFF);
            cpu.regs.set_cf(c);
            cpu.regs.write_hl(r);
            cpu.instr.step = 1;
            false
        }
        1 => true,
        _ => unreachable!(),
    }
}

/// ADD SP,e8 (4 M-cycle)
pub(crate) fn exec_add_sp(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            let e = cpu.imm8(bus) as i8 as i16 as u16;
            let sp = cpu.regs.sp;
            cpu.regs.set_zf(false);
            cpu.regs.set_nf(false);
            cpu.regs.set_hf((sp & 0x000F) + (e & 0x000F) > 0x000F);
            cpu.regs.set_cf((sp & 0x00FF) + (e & 0x00FF) > 0x00FF);
            cpu.regs.sp = sp.wrapping_add(e);
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.instr.step = 3;
            false
        }
        3 => true,
        _ => unreachable!(),
    }
}

// ──────────────────────────────────────────────
// フラグ操作・A ローテート(各1 M-cycle)
// ──────────────────────────────────────────────
pub(crate) fn exec_daa(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    let mut a = cpu.regs.a;
    if !cpu.regs.nf() {
        if cpu.regs.cf() || a > 0x99 {
            a = a.wrapping_add(0x60);
            cpu.regs.set_cf(true);
        }
        if cpu.regs.hf() || (a & 0x0F) > 0x09 {
            a = a.wrapping_add(0x06);
        }
    } else {
        if cpu.regs.cf() {
            a = a.wrapping_sub(0x60);
        }
        if cpu.regs.hf() {
            a = a.wrapping_sub(0x06);
        }
    }
    cpu.regs.set_zf(a == 0);
    cpu.regs.set_hf(false);
    cpu.regs.a = a;
    true
}

pub(crate) fn exec_cpl(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    cpu.regs.a = !cpu.regs.a;
    cpu.regs.set_nf(true);
    cpu.regs.set_hf(true);
    true
}

pub(crate) fn exec_scf(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    cpu.regs.set_nf(false);
    cpu.regs.set_hf(false);
    cpu.regs.set_cf(true);
    true
}

pub(crate) fn exec_ccf(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    let c = cpu.regs.cf();
    cpu.regs.set_nf(false);
    cpu.regs.set_hf(false);
    cpu.regs.set_cf(!c);
    true
}

pub(crate) fn exec_rlca(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    let a = cpu.regs.a;
    let c = a & 0x80 != 0;
    cpu.regs.a = (a << 1) | c as u8;
    cpu.regs.set_zf(false);
    cpu.regs.set_nf(false);
    cpu.regs.set_hf(false);
    cpu.regs.set_cf(c);
    true
}

pub(crate) fn exec_rrca(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    let a = cpu.regs.a;
    let c = a & 0x01 != 0;
    cpu.regs.a = (a >> 1) | ((c as u8) << 7);
    cpu.regs.set_zf(false);
    cpu.regs.set_nf(false);
    cpu.regs.set_hf(false);
    cpu.regs.set_cf(c);
    true
}

pub(crate) fn exec_rla(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    let a = cpu.regs.a;
    let oc = cpu.regs.cf() as u8;
    let nc = a & 0x80 != 0;
    cpu.regs.a = (a << 1) | oc;
    cpu.regs.set_zf(false);
    cpu.regs.set_nf(false);
    cpu.regs.set_hf(false);
    cpu.regs.set_cf(nc);
    true
}

pub(crate) fn exec_rra(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    let a = cpu.regs.a;
    let oc = cpu.regs.cf() as u8;
    let nc = a & 0x01 != 0;
    cpu.regs.a = (a >> 1) | (oc << 7);
    cpu.regs.set_zf(false);
    cpu.regs.set_nf(false);
    cpu.regs.set_hf(false);
    cpu.regs.set_cf(nc);
    true
}

// ──────────────────────────────────────────────
// CB プリフィックス
// ──────────────────────────────────────────────
/// CB プリフィックス: 次バイトを読んで本来の CB 命令へ差し替える(1 M-cycle)
pub(crate) fn exec_cb_prefix(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    let cb = cpu.imm8(bus);
    cpu.instr = decode::decode_cb(cb);
    false
}

/// CB ローテート/シフト系の read-modify-write 共通処理
fn cb_rmw(cpu: &mut Cpu, bus: &mut dyn MemoryBus, f: fn(&mut Cpu, u8) -> u8) -> bool {
    match cpu.instr.step {
        0 => match cpu.instr.dst {
            Operand::Reg8(r) => {
                let v = cpu.read_reg8(r);
                let nv = f(cpu, v);
                cpu.write_reg8(r, nv);
                true
            }
            Operand::Ind(i) => {
                let a = cpu.ind_addr(i);
                cpu.wz = bus.read(a) as u16;
                cpu.instr.step = 1;
                false
            }
            _ => unreachable!(),
        },
        1 => {
            let nv = f(cpu, cpu.wz as u8);
            let a = cpu.ind_addr(cpu.instr.dst.ind());
            bus.write(a, nv);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

macro_rules! cb_exec {
    ($name:ident, $t:ident) => {
        pub(crate) fn $name(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
            cb_rmw(cpu, bus, Cpu::$t)
        }
    };
}
cb_exec!(exec_cb_rlc, rlc_t);
cb_exec!(exec_cb_rrc, rrc_t);
cb_exec!(exec_cb_rl, rl_t);
cb_exec!(exec_cb_rr, rr_t);
cb_exec!(exec_cb_sla, sla_t);
cb_exec!(exec_cb_sra, sra_t);
cb_exec!(exec_cb_swap, swap_t);
cb_exec!(exec_cb_srl, srl_t);

/// BIT n,r / BIT n,(HL)
pub(crate) fn exec_cb_bit(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    let n = cpu.instr.src.bit();
    match cpu.instr.step {
        0 => match cpu.instr.dst {
            Operand::Reg8(r) => {
                let v = cpu.read_reg8(r);
                cpu.bit_test(v, n);
                true
            }
            Operand::Ind(i) => {
                let a = cpu.ind_addr(i);
                cpu.wz = bus.read(a) as u16;
                cpu.instr.step = 1;
                false
            }
            _ => unreachable!(),
        },
        1 => {
            cpu.bit_test(cpu.wz as u8, n);
            true
        }
        _ => unreachable!(),
    }
}

/// RES/SET n の共通処理
fn cb_setres(cpu: &mut Cpu, bus: &mut dyn MemoryBus, set: bool) -> bool {
    let n = cpu.instr.src.bit();
    let apply = |v: u8| if set { v | (1 << n) } else { v & !(1 << n) };
    match cpu.instr.step {
        0 => match cpu.instr.dst {
            Operand::Reg8(r) => {
                let v = cpu.read_reg8(r);
                cpu.write_reg8(r, apply(v));
                true
            }
            Operand::Ind(i) => {
                let a = cpu.ind_addr(i);
                cpu.wz = bus.read(a) as u16;
                cpu.instr.step = 1;
                false
            }
            _ => unreachable!(),
        },
        1 => {
            let nv = apply(cpu.wz as u8);
            let a = cpu.ind_addr(cpu.instr.dst.ind());
            bus.write(a, nv);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}
pub(crate) fn exec_cb_res(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    cb_setres(cpu, bus, false)
}
pub(crate) fn exec_cb_set(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    cb_setres(cpu, bus, true)
}

// ──────────────────────────────────────────────
// ジャンプ・コール・リターン
// ──────────────────────────────────────────────
fn taken(cpu: &Cpu) -> bool {
    match cpu.instr.src.cond() {
        Some(c) => cpu.cond(c),
        None => true,
    }
}

/// JP nn / JP cc,nn
pub(crate) fn exec_jp(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (cpu.imm8(bus) as u16) << 8;
            cpu.instr.step = if taken(cpu) { 2 } else { 3 };
            false
        }
        2 => {
            cpu.regs.pc = cpu.wz;
            cpu.instr.step = 3;
            false
        }
        3 => true,
        _ => unreachable!(),
    }
}

/// JP HL (1 M-cycle)
pub(crate) fn exec_jp_hl(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    cpu.regs.pc = cpu.regs.hl();
    true
}

/// JR e / JR cc,e
pub(crate) fn exec_jr(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as i8 as u16;
            cpu.instr.step = if taken(cpu) { 1 } else { 2 };
            false
        }
        1 => {
            cpu.regs.pc = cpu.regs.pc.wrapping_add(cpu.wz);
            cpu.instr.step = 2;
            false
        }
        2 => true,
        _ => unreachable!(),
    }
}

/// CALL nn / CALL cc,nn
pub(crate) fn exec_call(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.imm8(bus) as u16;
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (cpu.imm8(bus) as u16) << 8;
            cpu.instr.step = if taken(cpu) { 2 } else { 5 };
            false
        }
        2 => {
            cpu.instr.step = 3;
            false
        }
        3 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, (cpu.regs.pc >> 8) as u8);
            cpu.instr.step = 4;
            false
        }
        4 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, cpu.regs.pc as u8);
            cpu.regs.pc = cpu.wz;
            cpu.instr.step = 5;
            false
        }
        5 => true,
        _ => unreachable!(),
    }
}

/// RET (4 M-cycle)
pub(crate) fn exec_ret(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = bus.read(cpu.regs.sp) as u16;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (bus.read(cpu.regs.sp) as u16) << 8;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.instr.step = 3;
            false
        }
        3 => {
            cpu.regs.pc = cpu.wz;
            true
        }
        _ => unreachable!(),
    }
}

/// RETI (4 M-cycle)
pub(crate) fn exec_reti(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = bus.read(cpu.regs.sp) as u16;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (bus.read(cpu.regs.sp) as u16) << 8;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.instr.step = 3;
            false
        }
        3 => {
            cpu.regs.pc = cpu.wz;
            cpu.ime = true;
            true
        }
        _ => unreachable!(),
    }
}

/// RET cc (条件付き)
pub(crate) fn exec_ret_cc(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.instr.step = if taken(cpu) { 1 } else { 5 };
            false
        }
        1 => {
            cpu.wz = bus.read(cpu.regs.sp) as u16;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.wz |= (bus.read(cpu.regs.sp) as u16) << 8;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 3;
            false
        }
        3 => {
            cpu.instr.step = 4;
            false
        }
        4 => {
            cpu.regs.pc = cpu.wz;
            true
        }
        5 => true,
        _ => unreachable!(),
    }
}

/// RST n (4 M-cycle)
pub(crate) fn exec_rst(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, (cpu.regs.pc >> 8) as u8);
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, cpu.regs.pc as u8);
            cpu.regs.pc = cpu.instr.dst.rst() as u16;
            cpu.instr.step = 3;
            false
        }
        3 => true,
        _ => unreachable!(),
    }
}

// ──────────────────────────────────────────────
// スタック
// ──────────────────────────────────────────────
/// PUSH rr (4 M-cycle)
pub(crate) fn exec_push(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = cpu.read_reg16(cpu.instr.src.reg16());
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, (cpu.wz >> 8) as u8);
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, cpu.wz as u8);
            cpu.instr.step = 3;
            false
        }
        3 => true,
        _ => unreachable!(),
    }
}

/// POP rr (3 M-cycle)
pub(crate) fn exec_pop(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            cpu.wz = bus.read(cpu.regs.sp) as u16;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.wz |= (bus.read(cpu.regs.sp) as u16) << 8;
            cpu.regs.sp = cpu.regs.sp.wrapping_add(1);
            cpu.instr.step = 2;
            false
        }
        2 => {
            cpu.write_reg16(cpu.instr.dst.reg16(), cpu.wz);
            true
        }
        _ => unreachable!(),
    }
}

// ──────────────────────────────────────────────
// 制御・割り込み
// ──────────────────────────────────────────────
pub(crate) fn exec_halt(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    let pending = bus.ie() & bus.if_() & 0x1F;
    if !cpu.ime && pending != 0 {
        // HALT バグ: HALT に入らず、次の fetch で PC をインクリメントしない
        cpu.halt_bug = true;
    } else {
        cpu.halted = true;
    }
    true
}

pub(crate) fn exec_stop(_cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    // CGB: KEY1 (0xFF4D) bit0 が立っていればダブルスピード切替。
    // bit7（速度フラグ）は通常の write() では変更できない内部状態のため専用メソッドを使う。
    bus.perform_speed_switch();
    true
}

pub(crate) fn exec_di(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    cpu.ime = false;
    cpu.ei_delay = false;
    true
}

pub(crate) fn exec_ei(cpu: &mut Cpu, _bus: &mut dyn MemoryBus) -> bool {
    // IME は次の命令終了後に有効化
    cpu.ei_delay = true;
    true
}

/// 割り込みディスパッチ（実機に合わせ5 M-cycle）
/// M1: 内部NOP (IMEクリア・ベクタ確定・IFビットクリア)
/// M2: 内部NOP
/// M3: PCH プッシュ
/// M4: PCL プッシュ
/// M5: PC ← ベクタ
pub(crate) fn exec_interrupt(cpu: &mut Cpu, bus: &mut dyn MemoryBus) -> bool {
    match cpu.instr.step {
        0 => {
            let pending = bus.ie() & bus.if_() & 0x1F;
            cpu.ime = false;
            for bit in 0..5u8 {
                if pending & (1 << bit) != 0 {
                    bus.set_if(bus.if_() & !(1 << bit));
                    cpu.wz = 0x0040u16 + (bit as u16) * 0x08;
                    break;
                }
            }
            cpu.instr.step = 1;
            false
        }
        1 => {
            cpu.instr.step = 2;
            false
        }
        2 => {
            // fetch 済みのため戻るべき命令アドレスは PC - 1
            let ret = cpu.regs.pc.wrapping_sub(1);
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, (ret >> 8) as u8);
            cpu.instr.step = 3;
            false
        }
        3 => {
            let ret = cpu.regs.pc.wrapping_sub(1);
            cpu.regs.sp = cpu.regs.sp.wrapping_sub(1);
            bus.write(cpu.regs.sp, ret as u8);
            cpu.instr.step = 4;
            false
        }
        4 => {
            cpu.regs.pc = cpu.wz;
            true
        }
        _ => unreachable!(),
    }
}
