//! オペランドの型定義。

/// オペランドの性質を1つの enum で表現(命令の合成要素)。
#[derive(Clone, Copy)]
pub enum Operand {
    None,
    Reg8(Reg8),
    Reg16(Reg16),
    Imm,
    Ind(Indirect),
    Cond(Cond),
    Bit(u8),
    Rst(u8),
}

impl Operand {
    pub fn reg16(self) -> Reg16 {
        match self {
            Operand::Reg16(r) => r,
            _ => unreachable!(),
        }
    }
    pub fn ind(self) -> Indirect {
        match self {
            Operand::Ind(i) => i,
            _ => unreachable!(),
        }
    }
    pub fn bit(self) -> u8 {
        match self {
            Operand::Bit(b) => b,
            _ => unreachable!(),
        }
    }
    pub fn rst(self) -> u8 {
        match self {
            Operand::Rst(a) => a,
            _ => unreachable!(),
        }
    }
    /// 条件を取り出す。None の場合は無条件(常に成立)。
    pub fn cond(self) -> Option<Cond> {
        match self {
            Operand::Cond(c) => Some(c),
            Operand::None => None,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Reg8 {
    A,
    B,
    C,
    D,
    E,
    H,
    L,
}

#[derive(Clone, Copy)]
pub enum Reg16 {
    AF,
    BC,
    DE,
    HL,
    SP,
}

#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)] // GB レジスタ表記の慣習に合わせる
pub enum Indirect {
    BC,
    DE,
    HL,
    HLI,
    HLD,
    /// 0xFF00 | C
    CFF,
}

#[derive(Clone, Copy)]
pub enum Cond {
    NZ,
    Z,
    NC,
    C,
}
