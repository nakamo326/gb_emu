use super::{
    Cpu,
    operand::{Cond, IO8, IO16, Imm8, Imm16, Reg16},
};

use crate::mmu::Mmu;

use std::sync::atomic::{AtomicU8, AtomicU16, Ordering::Relaxed};

macro_rules! step {
    ($d:expr, {$($c:tt : $e:expr,)*}) => {
        static STEP: AtomicU8 = AtomicU8::new(0);
        #[allow(dead_code)]
        static VAL8: AtomicU8 = AtomicU8::new(0);
        #[allow(dead_code)]
        static VAL16: AtomicU16 = AtomicU16::new(0);
        $(if STEP.load(Relaxed) == $c { $e })* else { return $d; }
    };
}
pub(crate) use step;

macro_rules! go {
    ($e:expr) => {
        STEP.store($e, Relaxed);
    };
}
pub(crate) use go;

impl Cpu {
    pub fn nop(&mut self, bus: &mut Mmu) {
        self.fetch(bus);
    }

    // ──────────────────────────────────────────────
    // LD
    // ──────────────────────────────────────────────

    pub fn ld<D: Copy, S: Copy>(&mut self, bus: &mut Mmu, dst: D, src: S)
    where
        Self: IO8<D> + IO8<S>,
    {
        step!((), {
            0: if let Some(val) = self.read8(bus, src) {
                VAL8.store(val, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, dst, VAL8.load(Relaxed)).is_some() {
                go!(2);
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn ld16<D: Copy, S: Copy>(&mut self, bus: &mut Mmu, dst: D, src: S)
    where
        Self: IO16<D> + IO16<S>,
    {
        step!((), {
            0: if let Some(v) = self.read16(bus, src) {
                VAL16.store(v, Relaxed);
                go!(1);
            },
            1: if self.write16(bus, dst, VAL16.load(Relaxed)).is_some() {
                go!(2);
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// LD HL, SP+e8
    pub fn ldhl(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(v) = self.read8(bus, Imm8) {
                let e = v as i8 as i16 as u16;
                let sp = self.regs.sp;
                self.regs.set_zf(false);
                self.regs.set_nf(false);
                self.regs.set_hf((sp & 0x000F) + (e & 0x000F) > 0x000F);
                self.regs.set_cf((sp & 0x00FF) + (e & 0x00FF) > 0x00FF);
                self.regs.write_hl(sp.wrapping_add(e));
                go!(1);
            },
            1: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    // ──────────────────────────────────────────────
    // 8bit ALU
    // ──────────────────────────────────────────────

    pub fn add<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            let a = self.regs.a;
            let (result, carry) = a.overflowing_add(v);
            self.regs.set_zf(result == 0);
            self.regs.set_nf(false);
            self.regs.set_hf((a & 0x0F) + (v & 0x0F) > 0x0F);
            self.regs.set_cf(carry);
            self.regs.a = result;
            self.fetch(bus);
        }
    }

    pub fn adc<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            let a = self.regs.a;
            let c = self.regs.cf() as u8;
            let result = a.wrapping_add(v).wrapping_add(c);
            self.regs.set_zf(result == 0);
            self.regs.set_nf(false);
            self.regs.set_hf((a & 0x0F) + (v & 0x0F) + c > 0x0F);
            self.regs.set_cf((a as u16) + (v as u16) + (c as u16) > 0xFF);
            self.regs.a = result;
            self.fetch(bus);
        }
    }

    pub fn sub<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            let a = self.regs.a;
            let (result, carry) = a.overflowing_sub(v);
            self.regs.set_zf(result == 0);
            self.regs.set_nf(true);
            self.regs.set_hf((a & 0x0F) < (v & 0x0F));
            self.regs.set_cf(carry);
            self.regs.a = result;
            self.fetch(bus);
        }
    }

    pub fn sbc<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            let a = self.regs.a;
            let c = self.regs.cf() as u8;
            let result = a.wrapping_sub(v).wrapping_sub(c);
            self.regs.set_zf(result == 0);
            self.regs.set_nf(true);
            self.regs.set_hf((a & 0x0F) < (v & 0x0F) + c);
            self.regs.set_cf((a as u16) < (v as u16) + (c as u16));
            self.regs.a = result;
            self.fetch(bus);
        }
    }

    pub fn and<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            self.regs.a &= v;
            self.regs.set_zf(self.regs.a == 0);
            self.regs.set_nf(false);
            self.regs.set_hf(true);
            self.regs.set_cf(false);
            self.fetch(bus);
        }
    }

    pub fn or<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            self.regs.a |= v;
            self.regs.set_zf(self.regs.a == 0);
            self.regs.set_nf(false);
            self.regs.set_hf(false);
            self.regs.set_cf(false);
            self.fetch(bus);
        }
    }

    pub fn xor<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            self.regs.a ^= v;
            self.regs.set_zf(self.regs.a == 0);
            self.regs.set_nf(false);
            self.regs.set_hf(false);
            self.regs.set_cf(false);
            self.fetch(bus);
        }
    }

    pub fn cp<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            let (result, carry) = self.regs.a.overflowing_sub(v);
            self.regs.set_zf(result == 0);
            self.regs.set_nf(true);
            self.regs.set_hf((self.regs.a & 0x0F) < (v & 0x0F));
            self.regs.set_cf(carry);
            self.fetch(bus);
        }
    }

    pub fn inc<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let result = v.wrapping_add(1);
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf((v & 0x0F) == 0x0F);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn inc16<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO16<S>,
    {
        step!((), {
            0: if let Some(v) = self.read16(bus, src) {
                VAL16.store(v.wrapping_add(1), Relaxed);
                go!(1);
            },
            1: if self.write16(bus, src, VAL16.load(Relaxed)).is_some() {
                go!(2);
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn dec<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let result = v.wrapping_sub(1);
                self.regs.set_zf(result == 0);
                self.regs.set_nf(true);
                self.regs.set_hf(v & 0x0F == 0);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn dec16<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO16<S>,
    {
        step!((), {
            0: if let Some(v) = self.read16(bus, src) {
                VAL16.store(v.wrapping_sub(1), Relaxed);
                go!(1);
            },
            1: if self.write16(bus, src, VAL16.load(Relaxed)).is_some() {
                go!(2);
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    // ──────────────────────────────────────────────
    // 16bit ALU
    // ──────────────────────────────────────────────

    /// ADD HL, r16
    pub fn add_hl<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO16<S>,
    {
        step!((), {
            0: {
                let v = self.read16(bus, src).unwrap();
                let hl = self.regs.hl();
                let (result, carry) = hl.overflowing_add(v);
                self.regs.set_nf(false);
                self.regs.set_hf((hl & 0x0FFF) + (v & 0x0FFF) > 0x0FFF);
                self.regs.set_cf(carry);
                self.regs.write_hl(result);
                go!(1);
                return;
            },
            1: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// ADD SP, e8
    pub fn add_sp(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(v) = self.read8(bus, Imm8) {
                let e = v as i8 as i16 as u16;
                let sp = self.regs.sp;
                self.regs.set_zf(false);
                self.regs.set_nf(false);
                self.regs.set_hf((sp & 0x000F) + (e & 0x000F) > 0x000F);
                self.regs.set_cf((sp & 0x00FF) + (e & 0x00FF) > 0x00FF);
                self.regs.sp = sp.wrapping_add(e);
                go!(1);
            },
            1: {
                go!(2);
                return;
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    // ──────────────────────────────────────────────
    // フラグ操作
    // ──────────────────────────────────────────────

    pub fn daa(&mut self, bus: &mut Mmu) {
        let mut a = self.regs.a;
        if !self.regs.nf() {
            if self.regs.cf() || a > 0x99 {
                a = a.wrapping_add(0x60);
                self.regs.set_cf(true);
            }
            if self.regs.hf() || (a & 0x0F) > 0x09 {
                a = a.wrapping_add(0x06);
            }
        } else {
            if self.regs.cf() {
                a = a.wrapping_sub(0x60);
            }
            if self.regs.hf() {
                a = a.wrapping_sub(0x06);
            }
        }
        self.regs.set_zf(a == 0);
        self.regs.set_hf(false);
        self.regs.a = a;
        self.fetch(bus);
    }

    pub fn cpl(&mut self, bus: &mut Mmu) {
        self.regs.a = !self.regs.a;
        self.regs.set_nf(true);
        self.regs.set_hf(true);
        self.fetch(bus);
    }

    pub fn scf(&mut self, bus: &mut Mmu) {
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(true);
        self.fetch(bus);
    }

    pub fn ccf(&mut self, bus: &mut Mmu) {
        let c = self.regs.cf();
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(!c);
        self.fetch(bus);
    }

    // ──────────────────────────────────────────────
    // ローテーション (非プリフィックス)
    // ──────────────────────────────────────────────

    pub fn rlca(&mut self, bus: &mut Mmu) {
        let a = self.regs.a;
        let carry = (a & 0x80) != 0;
        self.regs.a = (a << 1) | carry as u8;
        self.regs.set_zf(false);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(carry);
        self.fetch(bus);
    }

    pub fn rrca(&mut self, bus: &mut Mmu) {
        let a = self.regs.a;
        let carry = (a & 0x01) != 0;
        self.regs.a = (a >> 1) | ((carry as u8) << 7);
        self.regs.set_zf(false);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(carry);
        self.fetch(bus);
    }

    pub fn rla(&mut self, bus: &mut Mmu) {
        let a = self.regs.a;
        let old_carry = self.regs.cf() as u8;
        let new_carry = (a & 0x80) != 0;
        self.regs.a = (a << 1) | old_carry;
        self.regs.set_zf(false);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(new_carry);
        self.fetch(bus);
    }

    pub fn rra(&mut self, bus: &mut Mmu) {
        let a = self.regs.a;
        let old_carry = self.regs.cf() as u8;
        let new_carry = (a & 0x01) != 0;
        self.regs.a = (a >> 1) | (old_carry << 7);
        self.regs.set_zf(false);
        self.regs.set_nf(false);
        self.regs.set_hf(false);
        self.regs.set_cf(new_carry);
        self.fetch(bus);
    }

    // ──────────────────────────────────────────────
    // CB プリフィックス命令
    // ──────────────────────────────────────────────

    pub fn rlc<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let carry = (v & 0x80) != 0;
                let result = (v << 1) | carry as u8;
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(carry);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn rrc<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let carry = (v & 0x01) != 0;
                let result = (v >> 1) | ((carry as u8) << 7);
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(carry);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn rl<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let result = (v << 1) | self.regs.cf() as u8;
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(v & 0x80 != 0);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn rr<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let old_carry = self.regs.cf() as u8;
                let new_carry = (v & 0x01) != 0;
                let result = (v >> 1) | (old_carry << 7);
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(new_carry);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn sla<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let carry = (v & 0x80) != 0;
                let result = v << 1;
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(carry);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn sra<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let carry = (v & 0x01) != 0;
                let result = (v >> 1) | (v & 0x80);
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(carry);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn swap<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let result = (v >> 4) | (v << 4);
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(false);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn srl<S: Copy>(&mut self, bus: &mut Mmu, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                let carry = (v & 0x01) != 0;
                let result = v >> 1;
                self.regs.set_zf(result == 0);
                self.regs.set_nf(false);
                self.regs.set_hf(false);
                self.regs.set_cf(carry);
                VAL8.store(result, Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn bit<S: Copy>(&mut self, bus: &mut Mmu, bit: usize, src: S)
    where
        Self: IO8<S>,
    {
        if let Some(v) = self.read8(bus, src) {
            self.regs.set_zf(v & (1 << bit) == 0);
            self.regs.set_nf(false);
            self.regs.set_hf(true);
            self.fetch(bus);
        }
    }

    pub fn res<S: Copy>(&mut self, bus: &mut Mmu, bit: u8, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                VAL8.store(v & !(1 << bit), Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn set_bit<S: Copy>(&mut self, bus: &mut Mmu, bit: u8, src: S)
    where
        Self: IO8<S>,
    {
        step!((), {
            0: if let Some(v) = self.read8(bus, src) {
                VAL8.store(v | (1 << bit), Relaxed);
                go!(1);
            },
            1: if self.write8(bus, src, VAL8.load(Relaxed)).is_some() {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    // ──────────────────────────────────────────────
    // スタック操作
    // ──────────────────────────────────────────────

    pub fn push16(&mut self, bus: &mut Mmu, val: u16) -> Option<()> {
        step!(None, {
            0: {
                go!(1);
                return None;
            },
            1: {
                let [lo, hi] = u16::to_le_bytes(val);
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                bus.write(self.regs.sp, hi);
                VAL8.store(lo, Relaxed);
                go!(2);
                return None;
            },
            2: {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                bus.write(self.regs.sp, VAL8.load(Relaxed));
                go!(3);
                return None;
            },
            3: {
                go!(0);
                return Some(());
            },
        });
    }

    pub fn push(&mut self, bus: &mut Mmu, src: Reg16) {
        step!((), {
            0:{
                VAL16.store(self.read16(bus, src).unwrap(), Relaxed);
                go!(1);
            },
            1: if self.push16(bus, VAL16.load(Relaxed)).is_some() {
                go!(2);
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn pop16(&mut self, bus: &mut Mmu) -> Option<u16> {
        step!(None, {
            0: {
                VAL8.store(bus.read(self.regs.sp), Relaxed);
                self.regs.sp = self.regs.sp.wrapping_add(1);
                go!(1);
                return None;
            },
            1: {
                let hi = bus.read(self.regs.sp);
                self.regs.sp = self.regs.sp.wrapping_add(1);
                VAL16.store(u16::from_le_bytes([VAL8.load(Relaxed), hi]), Relaxed);
                go!(2);
                return None;
            },
            2: {
                go!(0);
                return Some(VAL16.load(Relaxed));
            },
        });
    }

    pub fn pop(&mut self, bus: &mut Mmu, dst: Reg16) {
        if let Some(v) = self.pop16(bus) {
            self.write16(bus, dst, v);
            self.fetch(bus);
        }
    }

    // ──────────────────────────────────────────────
    // ジャンプ
    // ──────────────────────────────────────────────

    pub fn jr(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(offset) = self.read8(bus, Imm8) {
                self.regs.pc = self.regs.pc.wrapping_add(offset as i8 as u16);
                go!(1);
            },
            1: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    fn cond(&self, cond: Cond) -> bool {
        match cond {
            Cond::NZ => !self.regs.zf(),
            Cond::Z => self.regs.zf(),
            Cond::NC => !self.regs.cf(),
            Cond::C => self.regs.cf(),
        }
    }

    pub fn jr_c(&mut self, bus: &mut Mmu, c: Cond) {
        step!((), {
            0: if let Some(offset) = self.read8(bus, Imm8) {
                go!(1);
                if self.cond(c) {
                    self.regs.pc = self.regs.pc.wrapping_add(offset as i8 as u16);
                    return;
                }
            },
            1: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// JP nn (無条件)
    pub fn jp(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(v) = self.read16(bus, Imm16) {
                self.regs.pc = v;
                go!(1);
            },
            1: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// JP HL
    pub fn jp_hl(&mut self, bus: &mut Mmu) {
        self.regs.pc = self.regs.hl();
        self.fetch(bus);
    }

    /// JP cc, nn (条件付き)
    pub fn jp_c(&mut self, bus: &mut Mmu, c: Cond) {
        step!((), {
            0: if let Some(v) = self.read16(bus, Imm16) {
                if self.cond(c) {
                    VAL16.store(v, Relaxed);
                    go!(1);
                    return;
                }
                go!(0);
                self.fetch(bus);
            },
            1: {
                self.regs.pc = VAL16.load(Relaxed);
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn call(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(v) = self.read16(bus, Imm16) {
                VAL16.store(v, Relaxed);
                go!(1);
            },
            1: if self.push16(bus, self.regs.pc).is_some() {
                self.regs.pc = VAL16.load(Relaxed);
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// CALL cc, nn (条件付き)
    pub fn call_c(&mut self, bus: &mut Mmu, c: Cond) {
        step!((), {
            0: if let Some(v) = self.read16(bus, Imm16) {
                if self.cond(c) {
                    VAL16.store(v, Relaxed);
                    go!(1);
                    return;
                }
                go!(0);
                self.fetch(bus);
            },
            1: if self.push16(bus, self.regs.pc).is_some() {
                self.regs.pc = VAL16.load(Relaxed);
                go!(0);
                self.fetch(bus);
            },
        });
    }

    pub fn ret(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(v) = self.pop16(bus) {
                self.regs.pc = v;
                go!(1);
            },
            1: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// RET cc (条件付き)
    pub fn ret_c(&mut self, bus: &mut Mmu, c: Cond) {
        step!((), {
            0: {
                go!(1);
                if !self.cond(c) {
                    go!(0);
                    self.fetch(bus);
                }
            },
            1: if let Some(v) = self.pop16(bus) {
                self.regs.pc = v;
                go!(2);
            },
            2: {
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// RETI
    pub fn reti(&mut self, bus: &mut Mmu) {
        step!((), {
            0: if let Some(v) = self.pop16(bus) {
                self.regs.pc = v;
                go!(1);
            },
            1: {
                self.ime = true;
                go!(0);
                self.fetch(bus);
            },
        });
    }

    /// RST n
    pub fn rst(&mut self, bus: &mut Mmu, addr: u8) {
        step!((), {
            0: {
                go!(1);
                return;
            },
            1: if self.push16(bus, self.regs.pc).is_some() {
                self.regs.pc = addr as u16;
                go!(0);
                self.fetch(bus);
            },
        });
    }

    // ──────────────────────────────────────────────
    // 制御
    // ──────────────────────────────────────────────

    pub fn halt(&mut self, bus: &mut Mmu) {
        self.halted = true;
        self.fetch(bus);
    }

    pub fn stop(&mut self, bus: &mut Mmu) {
        // STOP は通常 NOP として扱う（GBC速度切替は未実装）
        self.fetch(bus);
    }

    pub fn di(&mut self, bus: &mut Mmu) {
        self.ime = false;
        self.ei_delay = false;
        self.fetch(bus);
    }

    pub fn ei(&mut self, bus: &mut Mmu) {
        // IME は次の命令終了後に有効化
        self.ei_delay = true;
        self.fetch(bus);
    }
}
