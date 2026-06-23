/// ROM を CartridgeBus として提供するモジュール。
///
/// # Flash 埋め込み方式 (デフォルト)
///
/// `FlashCart` は `include_bytes!` でビルド時に Flash へ埋め込んだ ROM を使う。
/// SDカード回路が不要で、まず動作確認する際に使う。
///
/// # SDカード方式 (将来実装)
///
/// `teensy4-bsp 0.5` / `imxrt-hal 0.5` は embedded-hal 0.2 と 1.0 の両方を実装するため、
/// `embedded-sdmmc 0.7` (embedded-hal 1.0 必須) と組み合わせて実装可能。
/// SPI は `board::lpspi(...)` の戻り値を embedded-hal 1.0 の `SpiDevice` を満たすよう
/// ラップして渡す。
use gb_core::platform::CartridgeBus;

// MBC1 の外部 RAM 最大サイズ (4 バンク × 8KB = 32KB)
const MAX_RAM: usize = 0x8000;

static mut CART_RAM: [u8; MAX_RAM] = [0; MAX_RAM];

// ─────────────────────────────────────────────────────────────────────────────
// FlashCart: include_bytes! で Flash に埋め込まれた ROM
// ─────────────────────────────────────────────────────────────────────────────

/// Flash に埋め込んだ ROM を CartridgeBus として提供する。
/// RomOnly (32KB) および MBC1 (最大 2MB ROM + 32KB RAM) をサポート。
pub struct FlashCart {
    rom: &'static [u8],
    rom_bank: u8,
    ram_bank: u8,
    ram_enabled: bool,
    mode: bool,
}

impl FlashCart {
    /// `rom` には `include_bytes!()` で取得した ROM スライスを渡す。
    pub const fn new(rom: &'static [u8]) -> Self {
        Self {
            rom,
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            mode: false,
        }
    }

    fn cart_type(&self) -> u8 {
        if self.rom.len() >= 0x148 { self.rom[0x147] } else { 0x00 }
    }

    fn is_mbc1(&self) -> bool {
        matches!(self.cart_type(), 0x01..=0x03)
    }

    fn has_ram(&self) -> bool {
        matches!(self.cart_type(), 0x02 | 0x03)
    }

    fn ram_size(&self) -> usize {
        if !self.has_ram() || self.rom.len() < 0x14A {
            return 0;
        }
        match self.rom[0x149] {
            0x01 => 0x800,  //  2KB
            0x02 => 0x2000, //  8KB
            0x03 => 0x8000, // 32KB
            _ => 0,
        }
    }
}

impl CartridgeBus for FlashCart {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                if self.is_mbc1() {
                    let bank = if self.mode && self.rom.len() > 0x80000 {
                        (self.ram_bank << 5) & 0x60
                    } else {
                        0
                    };
                    let offset = bank as usize * 0x4000 + addr as usize;
                    if offset < self.rom.len() {
                        self.rom[offset]
                    } else {
                        0xFF
                    }
                } else {
                    self.rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }

            0x4000..=0x7FFF => {
                if self.is_mbc1() {
                    let mut bank = self.rom_bank;
                    if self.rom.len() > 0x80000 {
                        bank |= (self.ram_bank << 5) & 0x60;
                    }
                    if bank == 0 { bank = 1; }
                    let offset = bank as usize * 0x4000 + (addr as usize - 0x4000);
                    if offset < self.rom.len() {
                        self.rom[offset]
                    } else {
                        0xFF
                    }
                } else {
                    self.rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }

            0xA000..=0xBFFF => {
                let ram_size = self.ram_size();
                if self.is_mbc1() && self.ram_enabled && ram_size > 0 {
                    let bank = if self.mode { self.ram_bank } else { 0 };
                    let offset = bank as usize * 0x2000 + (addr as usize - 0xA000);
                    if offset < ram_size {
                        unsafe { CART_RAM[offset] }
                    } else {
                        0xFF
                    }
                } else {
                    0xFF
                }
            }

            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        if !self.is_mbc1() {
            return;
        }
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let bank = val & 0x1F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                self.ram_bank = val & 0x03;
            }
            0x6000..=0x7FFF => {
                self.mode = (val & 0x01) != 0;
            }
            0xA000..=0xBFFF => {
                let ram_size = self.ram_size();
                if self.ram_enabled && ram_size > 0 {
                    let bank = if self.mode { self.ram_bank } else { 0 };
                    let offset = bank as usize * 0x2000 + (addr as usize - 0xA000);
                    if offset < ram_size {
                        unsafe { CART_RAM[offset] = val; }
                    }
                }
            }
            _ => {}
        }
    }
}
