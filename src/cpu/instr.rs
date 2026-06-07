use super::Cpu;
use super::exec;
use super::operand::Operand;
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
