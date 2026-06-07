use crate::cpu::Cpu;
use crate::input::InputSource;
use crate::mmu::Mmu;
use crate::platform::{AudioSink, CartridgeBus, Display};

pub const CPU_CLOCK_HZ: u32 = 4_194_304;
pub const M_CYCLE_CLOCK: u32 = 4;

/// 1 M-cycle 進めた結果のイベント通知。
#[derive(Default, Clone, Copy)]
pub struct StepResult {
    /// このサイクルでフレームが完成し、display へ draw 済み
    pub frame_ready: bool,
    /// 終了要求（入力の quit が立った）
    pub quit: bool,
}

/// CPU・MMU と各プラットフォーム実装（表示・音声・入力・カート）を束ねる。
///
/// タイミング駆動（M-cycle 周期のスリープ等）は行わず、[`GameBoy::step`] を
/// 各プラットフォームの main ループが必要なペースで呼ぶ。
pub struct GameBoy<C: CartridgeBus, D: Display, A: AudioSink, I: InputSource> {
    cpu: Cpu,
    mmu: Mmu<C>,
    display: D,
    audio: A,
    input: I,
}

impl<C: CartridgeBus, D: Display, A: AudioSink, I: InputSource> GameBoy<C, D, A, I> {
    pub fn new(mut mmu: Mmu<C>, display: D, audio: A, input: I) -> Self {
        let mut cpu = Cpu::new();
        if !mmu.bootrom.is_active() {
            cpu.apply_dmg_init();
            mmu.apply_dmg_init();
        }
        Self { cpu, mmu, display, audio, input }
    }

    /// MMU への不変参照（test-harness の出力監視等に使用）。
    pub fn mmu(&self) -> &Mmu<C> {
        &self.mmu
    }

    /// 1 M-cycle 進める。フレーム完成時に display へ draw し、入力をポーリングする。
    pub fn step(&mut self) -> StepResult {
        let mut result = StepResult::default();

        self.cpu.emulate_cycle(&mut self.mmu);

        // タイマー割り込み
        if self.mmu.timer.emulate_cycle() {
            self.mmu.if_ |= 0x04;
        }

        // APU サンプル生成
        if let Some((l, r)) = self.mmu.apu.emulate_cycle() {
            self.audio.push(l, r);
        }

        // PPU: フレーム完成で描画 & 入力ポーリング
        if self.mmu.ppu.emulate_cycle() {
            self.display.draw(self.mmu.ppu.pixel_buffer());
            let btn = self.input.poll();
            if btn.quit {
                result.quit = true;
            }
            self.mmu.update_joypad(&btn);
            result.frame_ready = true;
        }
        if self.mmu.ppu.vblank_irq {
            self.mmu.ppu.vblank_irq = false;
            self.mmu.if_ |= 0x01;
        }
        if self.mmu.ppu.stat_irq {
            self.mmu.ppu.stat_irq = false;
            self.mmu.if_ |= 0x02;
        }

        result
    }
}
