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
    /// CGB ダブルスピードモードで動作中（メインループのタイミング調整に使用）
    pub double_speed: bool,
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
        // BootROM の有無にかかわらず ROM ヘッダで CGB モードを決定する。
        // DMG BootROM は CGB レジスタを初期化しないため、BootROM あり CGB ROM でも
        // cgb_mode だけは先に確定させる必要がある。
        let cgb_flag = mmu.cart.read(0x0143);
        let cgb_mode = cgb_flag == 0x80 || cgb_flag == 0xC0;
        mmu.set_cgb_mode(cgb_mode);
        if !mmu.bootrom.is_active() {
            // BootROM なし: ソフトウェアで起動直後のハードウェア状態を再現する
            if cgb_mode {
                cpu.apply_cgb_init();
                mmu.apply_cgb_init();
            } else {
                cpu.apply_dmg_init();
                mmu.apply_dmg_init();
            }
        }
        Self { cpu, mmu, display, audio, input }
    }

    /// MMU への不変参照（test-harness の出力監視等に使用）。
    pub fn mmu(&self) -> &Mmu<C> {
        &self.mmu
    }

    /// display への可変参照（プラットフォーム側の統計表示・計測に使用）。
    pub fn display_mut(&mut self) -> &mut D {
        &mut self.display
    }

    /// デバッグ用: CPU の (PC, HALT 中か, IME)。
    pub fn debug_cpu(&self) -> (u16, bool, bool) {
        self.cpu.debug_state()
    }

    /// デバッグ用: CPU の (A, HL, SP)。
    pub fn debug_regs(&self) -> (u8, u16, u16) {
        self.cpu.debug_regs()
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

        self.mmu.ppu.hblank_trigger = false;

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

        // HBlank DMA の 16 バイトブロック転送（PPU が HBlank に入ったタイミング）
        if self.mmu.ppu.hblank_trigger {
            self.mmu.step_hblank_dma();
        }

        result.double_speed = self.mmu.double_speed();
        result
    }
}
