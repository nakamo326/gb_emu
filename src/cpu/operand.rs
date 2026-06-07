//! CPU命令の型定義。
//! 命令は「現在のステップ(状態)・オペランド(参照)・実行関数」を持つ自己完結ユニット `Instr` として表現する。
//! オペランドの性質(Reg/Imm/Ind/...)と命令の性質(exec関数)を分離し、オペコードはその合成。

use super::Cpu;
use super::exec;
use crate::mmu::Mmu;

/// 1 M-cycle分の制御を行い、命令完了で `true` を返す実行関数。
pub type ExecFn = fn(&mut Cpu, &mut Mmu) -> bool;

/// 自己完結した命令実行ユニット。
#[derive(Clone, Copy)]
pub struct Instr {
    /// 現在のマイクロステップ(命令自身の状態)
    pub step: u8,
    /// 書き込み先オペランド
    pub dst: Operand,
    /// 読み出し元オペランド / 補助オペランド(条件・ビット番号など)
    pub src: Operand,
    /// 各ステップの制御内容。完了で true
    pub exec: ExecFn,
}

impl Instr {
    /// オペランドなしの命令(NOP / フラグ操作 / 制御命令など)
    pub fn simple(exec: ExecFn) -> Self {
        Self { step: 0, dst: Operand::None, src: Operand::None, exec }
    }
    /// dst/src を伴う命令
    pub fn with(dst: Operand, src: Operand, exec: ExecFn) -> Self {
        Self { step: 0, dst, src, exec }
    }
    /// 割り込みディスパッチ(疑似命令)
    pub fn interrupt() -> Self {
        Self::simple(exec::exec_interrupt)
    }
    /// 初期値(最初の emulate_cycle で decode により上書きされる)
    pub fn nop() -> Self {
        Self::simple(exec::exec_nop)
    }
}

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
