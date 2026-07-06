//! CPU・バス・周辺を束ねるトップレベル。
//!
//! GB 側と違いプラットフォームトレイトは持たず、ホストが
//! `run_frame()` → `framebuffer()` 描画 → `set_keys()` を毎フレーム回す。
//! タイミングは PPU のサイクルカウントが基準（1 フレーム = 280896 サイクル）。

use alloc::vec::Vec;

use crate::bus::Bus;
use crate::cpu::Cpu;
use crate::cpu::swi::BIOS_IRQ_FLAGS;

pub const CYCLES_PER_FRAME: u32 = 280_896;
/// 16.777216 MHz
pub const CLOCK_HZ: u32 = 1 << 24;

pub struct Gba {
    pub cpu: Cpu,
    pub bus: Bus,
}

impl Gba {
    /// bios が None の場合は HLE BIOS で動作する（PC は ROM エントリから開始）。
    pub fn new(rom: Vec<u8>, bios: Option<Vec<u8>>) -> Self {
        let hle = bios.is_none();
        let mut cpu = Cpu::new();
        let bus = Bus::new(rom, bios);
        if hle {
            cpu.hle_bios = true;
            cpu.apply_post_bios_state();
        }
        Self { cpu, bus }
    }

    /// 1 命令（halt 中は数サイクル）進める。フレーム完成で true を返す。
    pub fn step(&mut self) -> bool {
        let pending = self.bus.ie & self.bus.if_ != 0;
        if pending {
            self.cpu.halted = false;
            if self.bus.ime {
                self.cpu.irq(&mut self.bus);
            }
        }
        // HLE IntrWait: ユーザーハンドラが BIOS フラグを立てたら待機解除
        if let Some(mask) = self.cpu.intr_wait_mask {
            let flags = self.bus.read16(BIOS_IRQ_FLAGS);
            if flags & mask != 0 {
                self.bus.write16(BIOS_IRQ_FLAGS, flags & !mask);
                self.cpu.intr_wait_mask = None;
                self.cpu.halted = false;
            }
        }
        if self.bus.halt_request {
            self.bus.halt_request = false;
            self.cpu.halted = true;
        }

        let cycles = if self.cpu.halted {
            16 // halt 中は割り込みチェック粒度だけ時間を進める
        } else {
            self.cpu.step(&mut self.bus)
        };

        self.bus.dma_service();
        self.bus.if_ |= self.bus.timers.step(cycles);
        let ev = self.bus.ppu.step(cycles);
        self.bus.if_ |= ev.irq;
        if ev.vblank_dma {
            self.bus.dma_trigger(1);
        }
        if ev.hblank_dma {
            self.bus.dma_trigger(2);
        }
        self.bus.dma_service();
        ev.frame_done
    }

    pub fn run_frame(&mut self) {
        while !self.step() {}
    }

    /// 240x160 の RGB555 フレームバッファ。
    pub fn framebuffer(&self) -> &[u16] {
        &self.bus.ppu.framebuffer
    }

    /// 押下中キーのビットマスク (bit0:A 1:B 2:Select 3:Start 4:→ 5:← 6:↑ 7:↓ 8:R 9:L)
    pub fn set_keys(&mut self, keys: u16) {
        self.bus.set_keys(keys);
    }
}
