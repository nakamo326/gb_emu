/// ROM を CartridgeBus として提供するモジュール。
///
/// # Flash 埋め込み方式 (デフォルト)
///
/// `FlashCart` は `include_bytes!` でビルド時に Flash へ埋め込んだ ROM を使う。
/// SDカード回路が不要で、まず動作確認する際に使う。
///
/// # SDカード方式 (将来実装)
///
/// `teensy4-bsp 0.4` は `embedded-hal 0.2` を使っており、
/// `embedded-sdmmc 0.7` (embedded-hal 1.0 必須) とは非互換。
/// SDカード対応は以下のいずれかで解決後に実装予定:
/// - `teensy4-bsp` が embedded-hal 1.0 対応版 (0.5+) にアップデート
/// - `embedded-sdmmc 0.3` (embedded-hal 0.2 互換) + `embedded-hal-compat` で対応
use gb_core::platform::CartridgeBus;

// ─────────────────────────────────────────────────────────────────────────────
// FlashCart: include_bytes! で Flash に埋め込まれた ROM
// ─────────────────────────────────────────────────────────────────────────────

/// Flash に埋め込んだ ROM を CartridgeBus として提供する。
/// RomOnly (32KB) および MBC1 (最大 512KB, RAM なし) をサポート。
pub struct FlashCart {
    rom: &'static [u8],
    rom_bank: u8,
    mode: u8,
}

impl FlashCart {
    /// `rom` には `include_bytes!()` で取得した ROM スライスを渡す。
    pub const fn new(rom: &'static [u8]) -> Self {
        Self { rom, rom_bank: 1, mode: 0 }
    }

    fn cart_type(&self) -> u8 {
        if self.rom.len() >= 0x148 { self.rom[0x147] } else { 0x00 }
    }

    fn is_mbc1(&self) -> bool {
        matches!(self.cart_type(), 0x01..=0x03)
    }
}

impl CartridgeBus for FlashCart {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),

            0x4000..=0x7FFF => {
                let bank = if self.is_mbc1() {
                    let b = self.rom_bank as usize;
                    if b == 0 { 1 } else { b }
                } else {
                    1
                };
                let phys = bank * 0x4000 + (addr as usize - 0x4000);
                self.rom.get(phys).copied().unwrap_or(0xFF)
            }

            0xA000..=0xBFFF => 0xFF,

            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        if !self.is_mbc1() {
            return;
        }
        match addr {
            0x0000..=0x1FFF => {}
            0x2000..=0x3FFF => {
                self.rom_bank = (self.rom_bank & 0x60) | (val & 0x1F);
                if self.rom_bank & 0x1F == 0 {
                    self.rom_bank |= 1;
                }
            }
            0x4000..=0x5FFF => {
                if self.mode == 0 {
                    self.rom_bank = (self.rom_bank & 0x1F) | ((val & 0x03) << 5);
                }
            }
            0x6000..=0x7FFF => {
                self.mode = val & 0x01;
            }
            _ => {}
        }
    }
}
